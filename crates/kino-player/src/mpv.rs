//! Обёртка `libmpv2::Mpv`: создание, play/pause/seek/volume.
//!
//! На этом этапе (#017) плеер headless: без видеовыхода и без привязки к окну — это #018.
//! Тесты и `cargo test -p kino-player` используют `vo=null`/`ao=null` (см. [`Player::headless`]).

use libmpv2::Mpv;

/// Обёртка над `libmpv2::Mpv` с удобным API для KinoBox: play/pause/seek/volume,
/// загрузка потока ([`crate::source`]) и события ([`crate::events`]).
pub struct Player {
    mpv: Mpv,
}

impl Player {
    /// Создаёт плеер без видео- и аудиовыхода (`vo=null`, `ao=null`): не рисует окно, не требует
    /// звукового устройства. Используется в тестах — для реального видеовыхода в окне (#021)
    /// нужен [`Player::windowed`], не этот конструктор.
    pub fn headless() -> libmpv2::Result<Self> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("vo", "null")?;
            init.set_option("ao", "null")?;
            Ok(())
        })?;
        Ok(Self { mpv })
    }

    /// Создаёт плеер с видеовыходом через render API libmpv (`vo=libmpv`) и обычным звуковым
    /// устройством (`ao` не переопределяется — платформенный выбор mpv). `vo=libmpv` обязателен
    /// для [`crate::render::PlayerRenderContext::attach`]: без него `create_render_context`
    /// возвращает ошибку, так же как без этой опции рисовать в собственный `NSView` окна
    /// невозможно (см. пример `libmpv2` `examples/opengl.rs` — тот же `vo=libmpv` до создания
    /// render-контекста). Нужно выставить до старта mpv, поэтому отдельный конструктор, а не
    /// опция, которую можно сменить после `headless()`.
    pub fn windowed() -> libmpv2::Result<Self> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("vo", "libmpv")?;
            Ok(())
        })?;
        Ok(Self { mpv })
    }

    /// Доступ к внутреннему `Mpv` для модулей `events`/`source` этого крейта.
    pub(crate) fn mpv(&self) -> &Mpv {
        &self.mpv
    }

    /// Публичный доступ к внутреннему `libmpv2::Mpv` — нужен `src-tauri` (#021), чтобы передать
    /// хендл в [`crate::render::PlayerRenderContext::attach`] без протечки деталей `Player`
    /// наружу иначе, чем через сам тип `Mpv` (уже часть публичного API `render`).
    pub fn mpv_handle(&self) -> &Mpv {
        &self.mpv
    }

    /// Запускает воспроизведение, если оно на паузе.
    pub fn play(&self) -> libmpv2::Result<()> {
        self.mpv.set_property("pause", false)
    }

    /// Ставит воспроизведение на паузу.
    pub fn pause(&self) -> libmpv2::Result<()> {
        self.mpv.set_property("pause", true)
    }

    /// `true`, если воспроизведение на паузе.
    pub fn is_paused(&self) -> libmpv2::Result<bool> {
        self.mpv.get_property("pause")
    }

    /// Перематывает на абсолютную позицию в секундах от начала файла.
    pub fn seek_to(&self, seconds: f64) -> libmpv2::Result<()> {
        self.mpv
            .command("seek", &[&seconds.to_string(), "absolute"])
    }

    /// Текущая позиция воспроизведения в секундах; `None`, пока файл не загружен.
    pub fn position(&self) -> libmpv2::Result<Option<f64>> {
        get_optional(&self.mpv, "time-pos")
    }

    /// Длительность текущего файла в секундах; `None`, пока она не известна.
    pub fn duration(&self) -> libmpv2::Result<Option<f64>> {
        get_optional(&self.mpv, "duration")
    }

    /// Громкость, 0–100 (свойство mpv `volume`, может превышать 100 при усилении).
    pub fn volume(&self) -> libmpv2::Result<f64> {
        self.mpv.get_property("volume")
    }

    /// Устанавливает громкость. mpv сам ограничивает диапазон по `volume-max` (по умолчанию 130).
    pub fn set_volume(&self, volume: f64) -> libmpv2::Result<()> {
        self.mpv.set_property("volume", volume)
    }
}

/// Свойства `time-pos`/`duration` возвращают ошибку `PropertyUnavailable`, пока файл не загрузился
/// или не определена длительность (например прямые потоки без точного `Content-Length`).
/// Это ожидаемое промежуточное состояние, а не сбой, поэтому оно превращается в `None`.
fn get_optional(mpv: &Mpv, name: &str) -> libmpv2::Result<Option<f64>> {
    match mpv.get_property::<f64>(name) {
        Ok(value) => Ok(Some(value)),
        Err(libmpv2::Error::Raw(code)) if code == libmpv2::mpv_error::PropertyUnavailable => {
            Ok(None)
        }
        Err(err) => Err(err),
    }
}
