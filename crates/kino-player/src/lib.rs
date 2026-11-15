//! Обёртка libmpv: воспроизведение, качество, субтитры, встраивание рендера в окно.
//!
//! `Player` (#017) — headless-ядро на `libmpv2::Mpv`: play/pause/seek/volume, загрузка
//! `StreamQuality`/`SubtitleTrack` из `kino-core`, поток событий воспроизведения. Модуль
//! `render` (#018) встраивает OpenGL render API `libmpv` в `NSView` произвольного окна через
//! `raw-window-handle`, независимо от `Player` — `src-tauri` (#021) связывает их: создаёт
//! `Player`, получает хендл окна `player` и передаёт `mpv_handle` в `PlayerRenderContext::attach`.

mod events;
mod mpv;
pub mod render;
mod source;

pub use events::{EndReason, PlayerEvent};
pub use mpv::Player;
pub use render::{PlayerRenderContext, RenderError, WindowRenderTarget};
