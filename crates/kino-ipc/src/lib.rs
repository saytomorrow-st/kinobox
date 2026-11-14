//! Типизированные Tauri-команды — мост Rust и TypeScript.
//!
//! Сервисный слой без зависимости от Tauri: `AppService` и DTO с `serde`.

mod dto;
mod error;
mod mirrors;
mod service;
mod session;
#[cfg(test)]
mod tests;

pub use dto::{BootState, MirrorDto, MirrorStateDto, UserDto};
pub use error::{IpcError, IpcErrorKind};
pub use service::AppService;
