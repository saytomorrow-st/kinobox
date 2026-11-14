//! HTTP-клиент, зеркала, сессия и парсинг HDrezka.

pub mod client;
pub mod deobfuscate;
pub mod mirrors;
pub mod session;

pub use client::{ClientConfig, HttpClient, Response};
pub use deobfuscate::deobfuscate;
pub use mirrors::{
    MemoryMirrorStore, MirrorConfig, MirrorPool, MirrorState, MirrorStatus, MirrorStore,
    ResolvedMirror, LOGIN_STUB_MARKERS,
};
pub use session::{
    ensure_logged_in, login, login_and_remember, Credentials, KeychainSecretStore,
    MemorySecretStore, Password, SecretStore,
};
