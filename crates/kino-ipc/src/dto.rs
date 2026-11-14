//! DTO для UI. Пароля здесь нет ни в одном типе.

use std::time::Duration;

use kino_api::{MirrorState, MirrorStatus, ResolvedMirror};
use kino_core::UserSession;
use serde::{Deserialize, Serialize};
use url::Url;

const NEEDS_LOGIN_REASON: &str = "требуется вход";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub user_id: u64,
    pub username: String,
}

impl From<&UserSession> for UserDto {
    fn from(session: &UserSession) -> Self {
        Self {
            user_id: session.user_id,
            username: session.username.clone(),
        }
    }
}

/// Состояние при запуске: что показать пользователю первым.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum BootState {
    /// Нужен экран входа; `email` — сохранённый адрес, если сайт отклонил вход.
    NeedsLogin {
        email: Option<String>,
    },
    SignedIn {
        user: UserDto,
    },
    /// Зеркала или сеть недоступны. Сохранённые данные не тронуты.
    Unavailable {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MirrorStateDto {
    Unknown,
    Open,
    NeedsLogin,
    Challenge,
    Dead,
}

impl From<MirrorState> for MirrorStateDto {
    fn from(state: MirrorState) -> Self {
        match state {
            MirrorState::Open => Self::Open,
            MirrorState::NeedsLogin => Self::NeedsLogin,
            MirrorState::Challenge => Self::Challenge,
            MirrorState::Dead => Self::Dead,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorDto {
    pub url: String,
    pub state: MirrorStateDto,
    pub reason: Option<String>,
    pub latency_ms: Option<u64>,
    pub active: bool,
    /// Запись получена пробой standby-адреса, а не из списка известных зеркал.
    pub standby: bool,
}

impl MirrorDto {
    pub(crate) fn unknown(url: &Url, active: bool) -> Self {
        Self {
            url: url.to_string(),
            state: MirrorStateDto::Unknown,
            reason: None,
            latency_ms: None,
            active,
            standby: false,
        }
    }

    pub(crate) fn from_status(status: &MirrorStatus, active: Option<&Url>, standby: bool) -> Self {
        Self {
            url: status.url.to_string(),
            state: status.state.into(),
            reason: status.reason.clone(),
            latency_ms: status.latency.map(millis),
            active: active.is_some_and(|active| same_mirror(active, &status.url)),
            standby,
        }
    }

    /// Зеркало, ставшее активным после `add_manual`.
    pub(crate) fn from_resolved(mirror: &ResolvedMirror) -> Self {
        Self {
            url: mirror.url.to_string(),
            state: mirror.state.into(),
            reason: (mirror.state == MirrorState::NeedsLogin)
                .then(|| NEEDS_LOGIN_REASON.to_string()),
            latency_ms: None,
            active: true,
            standby: false,
        }
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Те же зеркало: схема, хост и порт; путь и запрос не важны.
pub(crate) fn same_mirror(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::json;

    use super::*;

    fn roundtrip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let text = serde_json::to_string(value).expect("json");
        let back: T = serde_json::from_str(&text).expect("разбор");
        assert_eq!(*value, back);
    }

    fn user() -> UserDto {
        UserDto {
            user_id: 42,
            username: "kino".to_string(),
        }
    }

    #[test]
    fn boot_state_json_is_tagged_camel_case() {
        let cases = [
            (
                BootState::NeedsLogin { email: None },
                json!({"state": "needsLogin", "email": null}),
            ),
            (
                BootState::NeedsLogin {
                    email: Some("a@b.c".to_string()),
                },
                json!({"state": "needsLogin", "email": "a@b.c"}),
            ),
            (
                BootState::SignedIn { user: user() },
                json!({"state": "signedIn", "user": {"userId": 42, "username": "kino"}}),
            ),
            (
                BootState::Unavailable {
                    message: "нет сети".to_string(),
                },
                json!({"state": "unavailable", "message": "нет сети"}),
            ),
        ];
        for (state, expected) in cases {
            assert_eq!(serde_json::to_value(&state).expect("json"), expected);
            roundtrip(&state);
        }
    }

    #[test]
    fn mirror_json_is_camel_case() {
        let mirror = MirrorDto {
            url: "https://kvk.pub/".to_string(),
            state: MirrorStateDto::NeedsLogin,
            reason: None,
            latency_ms: Some(120),
            active: true,
            standby: false,
        };
        assert_eq!(
            serde_json::to_value(&mirror).expect("json"),
            json!({
                "url": "https://kvk.pub/",
                "state": "needsLogin",
                "reason": null,
                "latencyMs": 120,
                "active": true,
                "standby": false,
            })
        );
        roundtrip(&mirror);
    }

    #[test]
    fn mirror_states_serialize_as_camel_case_words() {
        let all = [
            (MirrorStateDto::Unknown, "unknown"),
            (MirrorStateDto::Open, "open"),
            (MirrorStateDto::NeedsLogin, "needsLogin"),
            (MirrorStateDto::Challenge, "challenge"),
            (MirrorStateDto::Dead, "dead"),
        ];
        for (state, word) in all {
            assert_eq!(serde_json::to_value(state).expect("json"), json!(word));
        }
    }

    #[test]
    fn same_mirror_ignores_path_and_default_port() {
        let parse = |raw: &str| Url::parse(raw).expect("url");
        assert!(same_mirror(
            &parse("https://kvk.pub/"),
            &parse("https://kvk.pub:443/films/")
        ));
        assert!(!same_mirror(
            &parse("https://kvk.pub/"),
            &parse("http://kvk.pub/")
        ));
        assert!(!same_mirror(
            &parse("https://kvk.pub/"),
            &parse("https://rezka.fi/")
        ));
    }
}
