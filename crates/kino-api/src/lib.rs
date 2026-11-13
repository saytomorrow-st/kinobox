//! HTTP-клиент, зеркала, сессия и парсинг HDrezka.

pub mod client;
pub mod deobfuscate;
pub mod mirrors;

pub use client::{ClientConfig, HttpClient, Response};
pub use deobfuscate::deobfuscate;
pub use mirrors::{
    MemoryMirrorStore, MirrorConfig, MirrorPool, MirrorStatus, MirrorStore, LOGIN_STUB_MARKERS,
};
