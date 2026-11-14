//! HTTP-клиент, зеркала, сессия и парсинг HDrezka.

pub mod api;
pub mod client;
pub mod deobfuscate;
pub mod mirrors;
pub mod parse;
pub mod sanitize;
pub mod session;

pub use api::HdrezkaApi;
pub use client::{ClientConfig, HttpClient, Response};
pub use deobfuscate::deobfuscate;
pub use mirrors::{
    MemoryMirrorStore, MirrorConfig, MirrorPool, MirrorState, MirrorStatus, MirrorStore,
    ResolvedMirror, LOGIN_STUB_MARKERS,
};
pub use parse::{catalog_url, parse_cards, parse_details, parse_last_page, search_url};
pub use sanitize::{contains_email, sanitize_page};
pub use session::{
    ensure_logged_in, login, login_and_remember, Credentials, KeychainSecretStore,
    MemorySecretStore, Password, SecretStore,
};
