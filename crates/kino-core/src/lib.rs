//! Доменные типы и трейты KinoBox без сетевых зависимостей.

pub mod error;
pub mod media;
pub mod provider;
pub mod stream;
pub mod user;

pub use error::{KinoError, Result};
pub use media::{Episode, MediaDetails, MediaItem, MediaKind, Season, Translator};
pub use provider::{CatalogQuery, KinoProvider};
pub use stream::{StreamQuality, StreamSource, SubtitleTrack};
pub use user::UserSession;
