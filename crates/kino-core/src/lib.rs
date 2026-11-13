//! Доменные типы и трейты KinoBox без сетевых зависимостей.

pub mod media;
pub mod stream;
pub mod user;

pub use media::{MediaDetails, MediaItem, MediaKind, Season, Translator};
pub use stream::{StreamQuality, StreamSource, SubtitleTrack};
pub use user::UserSession;
