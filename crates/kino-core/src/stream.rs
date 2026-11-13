//! Поток: варианты качества и дорожки субтитров, полученные после деобфускации.

use serde::{Deserialize, Serialize};
use url::Url;

/// Один вариант качества: метка для UI и прямой адрес потока.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamQuality {
    pub label: String,
    pub url: Url,
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
        });
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
                },
                StreamQuality {
                    label: "720p".to_string(),
                    url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
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
