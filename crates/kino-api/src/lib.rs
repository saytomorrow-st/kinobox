//! HTTP-клиент, зеркала, сессия и парсинг HDrezka.

pub mod client;
pub mod deobfuscate;

pub use client::{ClientConfig, HttpClient, Response};
pub use deobfuscate::deobfuscate;
