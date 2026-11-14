//! Ошибка для UI: тип причины и текст без секретов.

use std::fmt;

use kino_core::KinoError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IpcErrorKind {
    InvalidInput,
    LoginRejected,
    AuthRequired,
    Challenge,
    Network,
    Keychain,
    NoMirrors,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub kind: IpcErrorKind,
    pub message: String,
}

impl IpcError {
    pub fn new(kind: IpcErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(IpcErrorKind::InvalidInput, message)
    }
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for IpcError {}

/// Текст берётся из `Display` ошибки: в нём пароля нет по контракту `kino-api`.
impl From<KinoError> for IpcError {
    fn from(err: KinoError) -> Self {
        let kind = match &err {
            KinoError::Network(_) | KinoError::Http { .. } => IpcErrorKind::Network,
            KinoError::Parse(_) | KinoError::Deobfuscation(_) | KinoError::NotFound => {
                IpcErrorKind::Internal
            }
            KinoError::AllMirrorsUnavailable => IpcErrorKind::NoMirrors,
            KinoError::AuthRequired => IpcErrorKind::AuthRequired,
            KinoError::Challenge => IpcErrorKind::Challenge,
            KinoError::LoginRejected(_) => IpcErrorKind::LoginRejected,
            KinoError::Keychain(_) => IpcErrorKind::Keychain,
        };
        Self {
            kind,
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kino_errors_map_to_kinds_and_keep_display() {
        let cases = [
            (KinoError::Network("x".into()), IpcErrorKind::Network),
            (KinoError::Http { status: 503 }, IpcErrorKind::Network),
            (KinoError::Parse("x".into()), IpcErrorKind::Internal),
            (KinoError::Deobfuscation("x".into()), IpcErrorKind::Internal),
            (KinoError::NotFound, IpcErrorKind::Internal),
            (KinoError::AllMirrorsUnavailable, IpcErrorKind::NoMirrors),
            (KinoError::AuthRequired, IpcErrorKind::AuthRequired),
            (KinoError::Challenge, IpcErrorKind::Challenge),
            (
                KinoError::LoginRejected("x".into()),
                IpcErrorKind::LoginRejected,
            ),
            (KinoError::Keychain("x".into()), IpcErrorKind::Keychain),
        ];
        for (source, kind) in cases {
            let text = source.to_string();
            let err = IpcError::from(source);
            assert_eq!(err.kind, kind, "{text}");
            assert_eq!(err.message, text);
            assert_eq!(err.to_string(), text);
        }
    }

    #[test]
    fn json_shape_is_camel_case() {
        let err = IpcError::new(IpcErrorKind::LoginRejected, "нет");
        let value = serde_json::to_value(&err).expect("json");
        assert_eq!(
            value,
            serde_json::json!({"kind": "loginRejected", "message": "нет"})
        );
    }
}
