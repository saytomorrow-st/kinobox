//! Пользователь и его сессия.

use serde::{Deserialize, Serialize};

/// Сессия пользователя. Пароль, куки и токены живут только в macOS Keychain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserSession {
    pub user_id: u64,
    pub username: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;

    fn roundtrip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(*value, back);
    }

    fn sample() -> UserSession {
        UserSession {
            user_id: 42,
            username: "kinolover".to_string(),
        }
    }

    #[test]
    fn user_session_roundtrip() {
        roundtrip(&sample());
    }

    #[test]
    fn user_session_json_has_no_secrets() {
        let value = serde_json::to_value(sample()).unwrap();
        let object = value.as_object().unwrap();
        let forbidden = ["password", "passwd", "token", "hash", "cookie", "secret"];

        for key in object.keys() {
            let lower = key.to_ascii_lowercase();
            assert!(
                !forbidden.iter().any(|word| lower.contains(word)),
                "сессия не должна содержать поле {key:?}"
            );
        }
    }
}
