//! Медиа: карточки каталога и подробная информация о произведении.

use serde::{Deserialize, Serialize};
use url::Url;

/// Тип произведения в каталоге HDrezka.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Movie,
    Series,
    Cartoon,
    Anime,
}

/// Карточка произведения в каталоге, поиске или избранном.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: u64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<u16>,
    pub kind: MediaKind,
    pub poster_url: Option<Url>,
    pub page_url: Url,
    pub rating: Option<f32>,
}

/// Полная карточка произведения со страницы деталей.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaDetails {
    pub item: MediaItem,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub duration_minutes: Option<u32>,
    pub translators: Vec<Translator>,
    /// Пусто для фильмов; у сериалов — сезоны с номерами серий.
    pub seasons: Vec<Season>,
    /// Сырой HTML страницы деталей. Нужен для извлечения данных плеера
    /// (`initCDN...Events`) при запросе потоков фильма. В сеть не сериализуется:
    /// это кэш, а не данные.
    #[serde(skip)]
    pub raw_html: Option<String>,
}

/// Перевод (озвучка), доступный для произведения.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Translator {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Season {
    pub number: u16,
    pub episodes: Vec<u16>,
}

/// Одна серия сериала, полученная из `action=get_episodes` (`/ajax/get_cdn_series/`).
///
/// В отличие от [`Season`] (список номеров серий со страницы деталей), `Episode`
/// несёт заголовок серии из `<li>`-разметки и идентифицируется парой
/// `(season_id, episode_id)`. Используется `KinoProvider::episodes`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Episode {
    pub season_id: u32,
    pub episode_id: u32,
    pub title: String,
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

    fn sample_item() -> MediaItem {
        MediaItem {
            id: 1234,
            title: "Интерстеллар".to_string(),
            original_title: Some("Interstellar".to_string()),
            year: Some(2014),
            kind: MediaKind::Movie,
            poster_url: Some(Url::parse("https://hdrezka.ag/poster/1.jpg").unwrap()),
            page_url: Url::parse("https://hdrezka.ag/films/1234-interstellar.html").unwrap(),
            rating: Some(8.7),
        }
    }

    #[test]
    fn media_kind_roundtrip_and_snake_case() {
        for (kind, json) in [
            (MediaKind::Movie, "\"movie\""),
            (MediaKind::Series, "\"series\""),
            (MediaKind::Cartoon, "\"cartoon\""),
            (MediaKind::Anime, "\"anime\""),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), json);
            roundtrip(&kind);
        }
    }

    #[test]
    fn media_item_roundtrip() {
        roundtrip(&sample_item());

        let minimal = MediaItem {
            id: 1,
            title: "Без года".to_string(),
            original_title: None,
            year: None,
            kind: MediaKind::Anime,
            poster_url: None,
            page_url: Url::parse("https://hdrezka.ag/anime/1.html").unwrap(),
            rating: None,
        };
        roundtrip(&minimal);
    }

    #[test]
    fn translator_roundtrip() {
        roundtrip(&Translator {
            id: 55,
            name: "Дублированный".to_string(),
        });
    }

    #[test]
    fn season_roundtrip() {
        roundtrip(&Season {
            number: 2,
            episodes: vec![1, 2, 3, 4],
        });
    }

    #[test]
    fn episode_roundtrip() {
        roundtrip(&Episode {
            season_id: 1,
            episode_id: 3,
            title: "3 серия".to_string(),
        });
    }

    #[test]
    fn media_details_roundtrip() {
        roundtrip(&MediaDetails {
            item: sample_item(),
            description: Some("Описание".to_string()),
            genres: vec!["фантастика".to_string(), "драма".to_string()],
            countries: vec!["США".to_string()],
            duration_minutes: Some(169),
            translators: vec![Translator {
                id: 1,
                name: "Дублированный".to_string(),
            }],
            seasons: vec![],
            raw_html: None,
        });

        roundtrip(&MediaDetails {
            item: MediaItem {
                kind: MediaKind::Series,
                ..sample_item()
            },
            description: None,
            genres: vec![],
            countries: vec![],
            duration_minutes: None,
            translators: vec![],
            seasons: vec![Season {
                number: 1,
                episodes: vec![1, 2],
            }],
            raw_html: None,
        });
    }

    #[test]
    fn media_details_raw_html_is_skipped_in_serde() {
        let details = MediaDetails {
            item: sample_item(),
            description: None,
            genres: vec![],
            countries: vec![],
            duration_minutes: None,
            translators: vec![],
            seasons: vec![],
            raw_html: Some("<html>страница</html>".to_string()),
        };
        let json = serde_json::to_string(&details).unwrap();
        assert!(!json.contains("raw_html"));
        assert!(!json.contains("страница"));
        // После десериализации кэш пуст, но остальные данные целы.
        let back: MediaDetails = serde_json::from_str(&json).unwrap();
        assert_eq!(back.item.id, details.item.id);
        assert!(back.raw_html.is_none());
    }
}
