//! Встраивание рендера `libmpv` (OpenGL render API) в произвольное окно через
//! `raw-window-handle` — без привязки к Tauri/`WKWebView`.
//!
//! Архитектурная причина и область действия подробно описаны в `CRATE.md` (раздел
//! «Встраивание в окно») и `ARCHITECTURE.md` (раздел «Встраивание libmpv в окно»): `libmpv` не
//! может рисовать внутри `WKWebView`, поэтому окно `player` в `src-tauri` создаётся без
//! веб-контента, а эта функция отдаёт ему рендер-контекст, который рисует прямо в его `NSView`.
//!
//! Платформенная часть (сейчас — только macOS, `OpenGL`/`NSOpenGLContext`) изолирована в
//! `render::macos` и не протекает в публичный API этого модуля.

#[cfg(target_os = "macos")]
mod macos;

use libmpv2::render::{OpenGLInitParams, RenderParam, RenderParamApiType};
use libmpv2::Mpv;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("не удалось получить хендл окна: {0}")]
    HandleUnavailable(#[from] raw_window_handle::HandleError),
    /// Хендл окна не того типа, что ожидается на текущей платформе (например, не `AppKit` на
    /// macOS) — значит, окно создано не тем бэкендом, что рассчитывала эта функция.
    #[error("хендл окна не подходит для текущей платформы")]
    UnsupportedHandleKind,
    #[error("хендл окна не указывает на валидный NSView")]
    InvalidWindowHandle,
    #[error("встраивание рендера поддерживается только из главного потока приложения")]
    NotMainThread,
    #[error("не удалось создать GL-контекст")]
    GlContextInitFailed,
    #[error("не удалось загрузить OpenGL.framework")]
    GlLoadFailed,
    #[error("libmpv вернул ошибку: {0}")]
    Mpv(#[from] libmpv2::Error),
    #[error("встраивание рендера в окно не реализовано для этой платформы")]
    UnsupportedPlatform,
}

/// Платформенная сторона GL-контекста, привязанного к `NSView` окна. На не-macOS платформах
/// пока не собирается — крейт целиком заточен под macOS (`ARCHITECTURE.md`), но тип не протекает
/// платформенные детали наружу через `PlayerRenderContext`.
#[cfg(target_os = "macos")]
type PlatformTarget = macos::MacosRenderTarget;

/// GL-окружение конкретного окна: `NSView` + `NSOpenGLContext`, готовые принять кадры `libmpv`.
pub struct WindowRenderTarget {
    #[cfg(target_os = "macos")]
    inner: PlatformTarget,
}

impl WindowRenderTarget {
    /// Строит GL-окружение для хендла окна. Хендл должен быть получен с главного потока —
    /// `AppKitWindowHandle`/`NSView` не потокобезопасны (см. `raw_window_handle::AppKitWindowHandle`).
    pub fn new(handle: impl HasWindowHandle) -> Result<Self, RenderError> {
        let raw = handle.window_handle()?.as_raw();
        Self::from_raw(raw)
    }

    /// То же самое, но из уже полученного `RawWindowHandle` — используется и `new`, и тестами,
    /// которым не нужен настоящий `HasWindowHandle`.
    fn from_raw(raw: RawWindowHandle) -> Result<Self, RenderError> {
        #[cfg(target_os = "macos")]
        {
            let RawWindowHandle::AppKit(appkit) = raw else {
                return Err(RenderError::UnsupportedHandleKind);
            };
            let inner = macos::MacosRenderTarget::new(appkit)?;
            Ok(Self { inner })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = raw;
            Err(RenderError::UnsupportedPlatform)
        }
    }
}

/// Готовый рендер-контекст `libmpv`, привязанный к конкретному окну. Снаружи — только `render`:
/// вызов `make_current`/`flush`/вычисление размера скрыты внутри.
pub struct PlayerRenderContext<'a> {
    ctx: libmpv2::render::RenderContext<'a>,
    #[cfg(target_os = "macos")]
    target: PlatformTarget,
}

impl<'a> PlayerRenderContext<'a> {
    /// Создаёт render-контекст `libmpv` поверх GL-окружения `target` и подключает его к `mpv_handle`.
    pub fn attach(mpv_handle: &'a Mpv, target: WindowRenderTarget) -> Result<Self, RenderError> {
        #[cfg(target_os = "macos")]
        {
            let WindowRenderTarget { inner: target } = target;
            target.make_current();

            // `GlProcLookup` владеет `dlopen`-хендлом и передаётся в сам `OpenGLInitParams::ctx`:
            // `libmpv2` хранит его внутри `mpv_render_context` и отдаёт обратно в
            // `get_proc_address` на каждый резолв символа, пока жив `RenderContext`.
            let proc_lookup = macos::GlProcLookup::open()?;
            let ctx = mpv_handle.create_render_context(vec![
                RenderParam::ApiType(RenderParamApiType::OpenGl),
                RenderParam::InitParams(OpenGLInitParams {
                    get_proc_address: macos::get_proc_address,
                    ctx: proc_lookup,
                }),
            ])?;

            Ok(Self { ctx, target })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (mpv_handle, target);
            Err(RenderError::UnsupportedPlatform)
        }
    }

    /// Рисует текущий кадр в `fbo` (0 — задний буфер контекста окна) и выгружает буфер.
    /// `width`/`height` — размер фреймбуфера в пикселях (не в точках).
    pub fn render(&self, width: i32, height: i32) -> Result<(), RenderError> {
        #[cfg(target_os = "macos")]
        {
            self.target.make_current();
            self.ctx
                .render::<macos::GlProcLookup>(0, width, height, true)?;
            self.target.flush();
            self.ctx.report_swap();
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (width, height);
            Err(RenderError::UnsupportedPlatform)
        }
    }

    /// Текущий размер `NSView` в точках (без учёта `backingScaleFactor`) — вызывающая сторона
    /// (#021) решает, как переводить его в пиксели фреймбуфера для `render`.
    #[cfg(target_os = "macos")]
    pub fn view_size(&self) -> (f64, f64) {
        self.target.frame_size()
    }

    /// Подписка на «кадр готов»/«нужен редрав» — вызывающая сторона должна запланировать
    /// повторный `render` на главном потоке. Самого redraw-цикла здесь нет (см. #021/#024).
    pub fn set_update_callback<F: Fn() + Send + 'static>(&mut self, callback: F) {
        self.ctx.set_update_callback(callback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use raw_window_handle::UiKitWindowHandle;
    #[cfg(not(target_os = "macos"))]
    use raw_window_handle::Win32WindowHandle;

    /// На не-macOS платформе любой хендл отвергается одинаково: `UnsupportedPlatform`.
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn rejects_any_handle_on_non_macos() {
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(
            std::num::NonZeroIsize::new(1).unwrap(),
        ));
        assert!(matches!(
            WindowRenderTarget::from_raw(raw),
            Err(RenderError::UnsupportedPlatform)
        ));
    }

    /// На macOS хендл чужой платформы (`UiKit`) должен аккуратно отвергаться без паники и без
    /// обращения к objc2 — проверка типа варианта происходит раньше любого FFI-вызова. Указатель
    /// фиктивный и никогда не разыменовывается, поэтому допустим dangling-указатель, а не адрес
    /// реального объекта.
    #[test]
    #[cfg(target_os = "macos")]
    fn rejects_non_appkit_handle_on_macos() {
        let raw = RawWindowHandle::UiKit(UiKitWindowHandle::new(std::ptr::NonNull::dangling()));
        assert!(matches!(
            WindowRenderTarget::from_raw(raw),
            Err(RenderError::UnsupportedHandleKind)
        ));
    }
}
