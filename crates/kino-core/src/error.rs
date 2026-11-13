//! Ошибки провайдера: единый контракт между `kino-api` и `kino-ipc`.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, KinoError>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KinoError {
    /// Транспортная ошибка (DNS, таймаут, соединение); строка — только описание.
    #[error("сетевая ошибка: {0}")]
    Network(String),
    #[error("сервер ответил статусом {status}")]
    Http { status: u16 },
    /// Сменилась вёрстка или формат ответа неожиданный.
    #[error("не удалось разобрать ответ сайта: {0}")]
    Parse(String),
    #[error("не удалось разобрать ссылку потока: {0}")]
    Deobfuscation(String),
    #[error("ни одно зеркало не отвечает")]
    AllMirrorsUnavailable,
    #[error("требуется авторизация")]
    AuthRequired,
    /// Сайт потребовал защитный челлендж (например, Anubis PoW), клиент его не прошёл.
    #[error("сайт требует пройти защитную проверку")]
    Challenge,
    #[error("не найдено")]
    NotFound,
}

impl KinoError {
    /// Имеет ли смысл повторить запрос: сетевые сбои, 429 и 5xx.
    pub fn is_retryable(&self) -> bool {
        match self {
            KinoError::Network(_) => true,
            KinoError::Http { status } => *status == 429 || (500..=599).contains(status),
            KinoError::Parse(_)
            | KinoError::Deobfuscation(_)
            | KinoError::AllMirrorsUnavailable
            | KinoError::AuthRequired
            | KinoError::Challenge
            | KinoError::NotFound => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_is_retryable() {
        assert!(KinoError::Network("timeout".to_string()).is_retryable());
    }

    #[test]
    fn http_retryable_boundaries() {
        for status in [429, 500, 502, 503, 599] {
            assert!(KinoError::Http { status }.is_retryable(), "{status}");
        }
        for status in [400, 401, 403, 404, 428, 430, 499, 600] {
            assert!(!KinoError::Http { status }.is_retryable(), "{status}");
        }
    }

    #[test]
    fn other_variants_are_not_retryable() {
        for err in [
            KinoError::Parse("x".to_string()),
            KinoError::Deobfuscation("x".to_string()),
            KinoError::AllMirrorsUnavailable,
            KinoError::AuthRequired,
            KinoError::Challenge,
            KinoError::NotFound,
        ] {
            assert!(!err.is_retryable(), "{err:?}");
        }
    }

    #[test]
    fn display_is_not_empty() {
        for err in [
            KinoError::Network("x".to_string()),
            KinoError::Http { status: 503 },
            KinoError::Parse("x".to_string()),
            KinoError::Deobfuscation("x".to_string()),
            KinoError::AllMirrorsUnavailable,
            KinoError::AuthRequired,
            KinoError::Challenge,
            KinoError::NotFound,
        ] {
            assert!(!err.to_string().is_empty(), "{err:?}");
        }
    }

    #[test]
    fn http_display_contains_status() {
        assert!(KinoError::Http { status: 503 }.to_string().contains("503"));
    }
}
