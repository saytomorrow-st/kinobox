//! Поток: варианты качества и дорожки субтитров, полученные после деобфускации.

use serde::{Deserialize, Serialize};
use url::Url;

/// Один вариант качества потока.
///
/// `url` — основной прямой адрес mp4. `hls` — первый HLS-манифест, если
/// качество его предоставляет. `fallbacks` — резервные прямые mp4 в исходном
/// порядке (после основного `url`). `premium` — признак премиум-качества
/// (метка сайта содержала класс `pjs-prem-quality`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamQuality {
    pub label: String,
    /// Основной прямой mp4. Если прямых адресов нет, но есть HLS, сюда
    /// кладётся адрес HLS без суффикса `:hls:manifest.m3u8` (см. `kino-api`).
    pub url: Url,
    /// Премиум-качество (метка содержала класс `pjs-prem-quality`).
    #[serde(default)]
    pub premium: bool,
    /// Первый HLS-манифест, если качество его предоставляет.
    #[serde(default)]
    pub hls: Option<Url>,
    /// Резервные прямые mp4 после основного `url`, в исходном порядке.
    #[serde(default)]
    pub fallbacks: Vec<Url>,
}

/// Дорожка субтитров; метка языка сохраняется как пришла с сайта.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub lang: String,
    pub label: String,
    pub url: Url,
    pub is_default: bool,
}

/// Набор доступных потоков для выбранного перевода и серии.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSource {
    pub qualities: Vec<StreamQuality>,
    pub subtitles: Vec<SubtitleTrack>,
    /// Метка качества, которую выбрал сайт; `None` — выбирает плеер.
    pub default_quality: Option<String>,
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

    #[test]
    fn stream_quality_roundtrip() {
        roundtrip(&StreamQuality {
            label: "720p".to_string(),
            url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
            premium: false,
            hls: Some(Url::parse("https://cdn.example.com/720.mp4:hls:manifest.m3u8").unwrap()),
            fallbacks: vec![Url::parse("https://cdn.example.com/720-b.mp4").unwrap()],
        });
        roundtrip(&StreamQuality {
            label: "1080p Ultra".to_string(),
            url: Url::parse("https://cdn.example.com/1080.mp4").unwrap(),
            premium: true,
            hls: None,
            fallbacks: vec![],
        });
    }

    #[test]
    fn stream_quality_deserializes_old_shape() {
        // Старые данные имели только `label` и `url`; новые поля получают
        // значения по умолчанию.
        let json = r#"{"label":"720p","url":"https://cdn.example.com/720.mp4"}"#;
        let back: StreamQuality = serde_json::from_str(json).unwrap();
        assert_eq!(back.label, "720p");
        assert_eq!(back.url.as_str(), "https://cdn.example.com/720.mp4");
        assert!(!back.premium);
        assert!(back.hls.is_none());
        assert!(back.fallbacks.is_empty());
    }

    #[test]
    fn subtitle_track_roundtrip() {
        roundtrip(&SubtitleTrack {
            lang: "rus".to_string(),
            label: "Русские".to_string(),
            url: Url::parse("https://cdn.example.com/sub.vtt").unwrap(),
            is_default: true,
        });
    }

    #[test]
    fn stream_source_roundtrip() {
        roundtrip(&StreamSource {
            qualities: vec![
                StreamQuality {
                    label: "360p".to_string(),
                    url: Url::parse("https://cdn.example.com/360.mp4").unwrap(),
                    premium: false,
                    hls: None,
                    fallbacks: vec![],
                },
                StreamQuality {
                    label: "720p".to_string(),
                    url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
                    premium: false,
                    hls: None,
                    fallbacks: vec![],
                },
            ],
            subtitles: vec![SubtitleTrack {
                lang: "eng".to_string(),
                label: "English".to_string(),
                url: Url::parse("https://cdn.example.com/eng.vtt").unwrap(),
                is_default: false,
            }],
            default_quality: Some("720p".to_string()),
        });

        roundtrip(&StreamSource {
            qualities: vec![],
            subtitles: vec![],
            default_quality: None,
        });
    }
}
