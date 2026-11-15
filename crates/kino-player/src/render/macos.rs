//! macOS-реализация встраивания OpenGL-рендера `libmpv` в `NSView` окна.
//!
//! `NSOpenGLContext`/`NSOpenGLPixelFormat` помечены Apple как deprecated (с Catalina) в пользу
//! Metal, но остаются рабочими минимум до текущих версий macOS — см. `ARCHITECTURE.md`, раздел
//! «Встраивание libmpv в окно». `get_proc_address` берёт указатели функций через `dlsym` на
//! системном `OpenGL.framework` — это не требует собственного GL-контекста для резолва символов
//! (сам контекст нужен только для `makeCurrentContext`/`flushBuffer`) и не тянет лишних фич
//! `objc2-open-gl`.
//!
//! Весь модуль использует deprecated OpenGL API (`NSOpenGLContext` и смежные) осознанно —
//! см. обоснование выше и в `ARCHITECTURE.md` — отсюда `#![allow(deprecated)]` на уровне файла.

#![allow(deprecated)]

use std::ffi::{c_void, CString};
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSOpenGLContext, NSOpenGLPixelFormat, NSOpenGLPixelFormatAttribute, NSView};
use raw_window_handle::AppKitWindowHandle;

use super::RenderError;

/// Атрибуты пиксельного формата: двойная буферизация + 24-битный цвет + 24-битная глубина.
/// `0` — терминатор списка, как требует `NSOpenGLPixelFormat::initWithAttributes`.
const PIXEL_FORMAT_ATTRIBUTES: [NSOpenGLPixelFormatAttribute; 7] = [
    objc2_app_kit::NSOpenGLPFADoubleBuffer,
    objc2_app_kit::NSOpenGLPFAColorSize,
    24,
    objc2_app_kit::NSOpenGLPFADepthSize,
    24,
    objc2_app_kit::NSOpenGLPFAAccelerated,
    0,
];

/// Контекст, который `get_proc_address` libmpv получает на каждый вызов.
/// Сам указатель на `dlopen`-хендл `OpenGL.framework` живёт здесь же.
pub struct GlProcLookup {
    handle: *mut c_void,
}

// `dlopen`-хендл безопасно использовать из того потока, где `render()` вызывает libmpv; сам
// рендер-поток для libmpv — один и тот же на всё время жизни контекста.
unsafe impl Send for GlProcLookup {}

impl GlProcLookup {
    pub fn open() -> Result<Self, RenderError> {
        // Путь абсолютный и системный: это не поиск стороннего dylib, а резолв символов GL,
        // которые приложение не линкует напрямую (см. комментарий в начале файла).
        let path = CString::new("/System/Library/Frameworks/OpenGL.framework/OpenGL")
            .expect("путь без NUL-байт");
        let handle = unsafe { libc_dlopen(path.as_ptr(), RTLD_NOW) };
        if handle.is_null() {
            return Err(RenderError::GlLoadFailed);
        }
        Ok(Self { handle })
    }
}

impl Drop for GlProcLookup {
    fn drop(&mut self) {
        unsafe { libc_dlclose(self.handle) };
    }
}

// Пара сырых FFI-привязок к dlsym/dlopen/dlclose: добавлять зависимость `libc` только под них
// избыточно, а сигнатуры стабильны и минимальны.
const RTLD_NOW: i32 = 0x2;

extern "C" {
    #[link_name = "dlopen"]
    fn libc_dlopen(path: *const i8, mode: i32) -> *mut c_void;
    #[link_name = "dlsym"]
    fn libc_dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
    #[link_name = "dlclose"]
    fn libc_dlclose(handle: *mut c_void) -> i32;
}

pub fn get_proc_address(ctx: &GlProcLookup, name: &str) -> *mut c_void {
    let Ok(symbol) = CString::new(name) else {
        return std::ptr::null_mut();
    };
    unsafe { libc_dlsym(ctx.handle, symbol.as_ptr()) }
}

/// GL-контекст и `NSView`, в который он рисует. Держит обе ссылки живыми на время работы
/// рендера: `NSOpenGLContext::setView` не удерживает `view` сильной ссылкой.
pub struct MacosRenderTarget {
    gl_context: Retained<NSOpenGLContext>,
    view: Retained<NSView>,
}

impl MacosRenderTarget {
    /// Строит GL-контекст поверх `NSView` из хендла. Требует главный поток — как `NSView`,
    /// так и `setView` на нём работают только там (см. `raw_window_handle::AppKitWindowHandle`).
    pub fn new(handle: AppKitWindowHandle) -> Result<Self, RenderError> {
        let mtm = MainThreadMarker::new().ok_or(RenderError::NotMainThread)?;

        // SAFETY: хендл пришёл из `raw-window-handle`, которая гарантирует, что `ns_view`
        // указывает на живой `NSView` на момент вызова; `retain` только поднимает refcount.
        let view: Retained<NSView> = unsafe {
            Retained::retain(handle.ns_view.as_ptr().cast())
                .ok_or(RenderError::InvalidWindowHandle)?
        };

        let pixel_format = unsafe {
            NSOpenGLPixelFormat::initWithAttributes(
                NSOpenGLPixelFormat::alloc(),
                NonNull::from(&PIXEL_FORMAT_ATTRIBUTES).cast(),
            )
        }
        .ok_or(RenderError::GlContextInitFailed)?;

        let gl_context = NSOpenGLContext::initWithFormat_shareContext(
            NSOpenGLContext::alloc(),
            &pixel_format,
            None,
        )
        .ok_or(RenderError::GlContextInitFailed)?;

        gl_context.setView(Some(&view), mtm);

        Ok(Self { gl_context, view })
    }

    pub fn make_current(&self) {
        self.gl_context.makeCurrentContext();
    }

    pub fn flush(&self) {
        self.gl_context.flushBuffer();
    }

    /// Текущий размер `NSView` в точках. Для перевода в пиксели фреймбуфера вызывающая сторона
    /// должна учитывать `backingScaleFactor` окна — здесь не делается, т.к. `NSView` не
    /// гарантированно уже вставлен в окно/экран на момент вызова.
    pub fn frame_size(&self) -> (f64, f64) {
        let frame = self.view.frame();
        (frame.size.width, frame.size.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Намеренно нет теста с мусорным/произвольным указателем в `AppKitWindowHandle`:
    // `Retained::retain` шлёт Objective-C сообщение по этому адресу, и для невалидного
    // указателя это неопределённое поведение (может завершить процесс сигналом, а не
    // вернуть ошибку) — проверить "не паникует" таким способом нельзя. Валидация варианта
    // `RawWindowHandle` (AppKit/не-AppKit) без обращения к objc2 — в `render.rs`.

    #[test]
    fn gl_proc_lookup_resolves_known_symbol() {
        let lookup = GlProcLookup::open().expect("OpenGL.framework доступен на macOS runner");
        let ptr = get_proc_address(&lookup, "glClear");
        assert!(!ptr.is_null());
    }

    #[test]
    fn gl_proc_lookup_unknown_symbol_is_null_not_panic() {
        let lookup = GlProcLookup::open().expect("OpenGL.framework доступен на macOS runner");
        let ptr = get_proc_address(&lookup, "this_symbol_does_not_exist_in_opengl");
        assert!(ptr.is_null());
    }
}
