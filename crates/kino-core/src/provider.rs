//! Контракт источника данных: `kino-api` реализует, `kino-ipc` использует.

use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::media::{MediaDetails, MediaItem, MediaKind, Season};
use crate::stream::StreamSource;

/// Запрос страницы каталога.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogQuery {
    /// `None` — все типы.
    pub kind: Option<MediaKind>,
    /// Нумерация с 1.
    pub page: u32,
}

/// Запрос потока для выбранного перевода и серии.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamRequest {
    pub id: u64,
    pub translator_id: u64,
    /// Для фильмов `None`.
    pub season: Option<u16>,
    /// Для фильмов `None`.
    pub episode: Option<u16>,
}

/// Источник каталога и потоков.
///
/// Возвращаемые future — `Send`, чтобы реализации можно было запускать на
/// многопоточном рантайме. Любой метод вправе вернуть `Network`, `Http`,
/// `AllMirrorsUnavailable` и `Challenge`; специфичные ошибки указаны у методов.
pub trait KinoProvider: Send + Sync {
    /// Поиск по строке как есть: запрос не нормализуется и не обрезается.
    /// Пустой результат — `Ok(vec![])`, а не `NotFound`. Может вернуть `Parse`.
    fn search(&self, query: &str) -> impl Future<Output = Result<Vec<MediaItem>>> + Send;

    /// Страница каталога; `page` нумеруется с 1. Может вернуть `Parse`.
    fn catalog(&self, query: CatalogQuery) -> impl Future<Output = Result<Vec<MediaItem>>> + Send;

    /// Подробная карточка. Может вернуть `NotFound` и `Parse`.
    fn details(&self, item: &MediaItem) -> impl Future<Output = Result<MediaDetails>> + Send;

    /// Сезоны и серии для выбранного перевода. Может вернуть `NotFound`, `Parse`.
    fn episodes(
        &self,
        id: u64,
        translator_id: u64,
    ) -> impl Future<Output = Result<Vec<Season>>> + Send;

    /// Потоки уже после деобфускации, не сырая строка сайта. Может вернуть
    /// `NotFound`, `Parse`, `Deobfuscation` и `AuthRequired`.
    fn streams(&self, request: StreamRequest) -> impl Future<Output = Result<StreamSource>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::KinoError;
    use crate::media::Translator;
    use crate::stream::StreamQuality;
    use serde::de::DeserializeOwned;
    use url::Url;

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
            id: 7,
            title: "Тест".to_string(),
            original_title: None,
            year: Some(2020),
            kind: MediaKind::Series,
            poster_url: None,
            page_url: Url::parse("https://hdrezka.ag/series/7.html").unwrap(),
            rating: None,
        }
    }

    struct MockProvider {
        items: Vec<MediaItem>,
        details: MediaDetails,
        seasons: Vec<Season>,
        source: StreamSource,
    }

    impl MockProvider {
        fn new() -> Self {
            let item = sample_item();
            Self {
                items: vec![item.clone()],
                details: MediaDetails {
                    item,
                    description: None,
                    genres: vec![],
                    countries: vec![],
                    duration_minutes: None,
                    translators: vec![Translator {
                        id: 1,
                        name: "Дублированный".to_string(),
                    }],
                    seasons: vec![Season {
                        number: 1,
                        episodes: vec![1, 2],
                    }],
                },
                seasons: vec![Season {
                    number: 1,
                    episodes: vec![1, 2],
                }],
                source: StreamSource {
                    qualities: vec![StreamQuality {
                        label: "720p".to_string(),
                        url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
                    }],
                    subtitles: vec![],
                    default_quality: Some("720p".to_string()),
                },
            }
        }
    }

    impl KinoProvider for MockProvider {
        async fn search(&self, query: &str) -> Result<Vec<MediaItem>> {
            if query.is_empty() {
                return Ok(vec![]);
            }
            Ok(self.items.clone())
        }

        async fn catalog(&self, query: CatalogQuery) -> Result<Vec<MediaItem>> {
            if query.page == 0 {
                return Err(KinoError::NotFound);
            }
            Ok(self
                .items
                .iter()
                .filter(|i| query.kind.is_none_or(|k| k == i.kind))
                .cloned()
                .collect())
        }

        async fn details(&self, item: &MediaItem) -> Result<MediaDetails> {
            if item.id == self.details.item.id {
                Ok(self.details.clone())
            } else {
                Err(KinoError::NotFound)
            }
        }

        async fn episodes(&self, id: u64, translator_id: u64) -> Result<Vec<Season>> {
            if id == self.details.item.id && translator_id == 1 {
                Ok(self.seasons.clone())
            } else {
                Err(KinoError::NotFound)
            }
        }

        async fn streams(&self, request: StreamRequest) -> Result<StreamSource> {
            if request.id == self.details.item.id {
                Ok(self.source.clone())
            } else {
                Err(KinoError::NotFound)
            }
        }
    }

    fn assert_send<T: Send>(_: &T) {}

    #[test]
    fn mock_provider_exercises_all_methods() {
        let provider = MockProvider::new();
        let item = sample_item();

        assert_eq!(
            pollster::block_on(provider.search("тест")).unwrap(),
            provider.items
        );
        assert!(pollster::block_on(provider.search("")).unwrap().is_empty());

        let page = CatalogQuery {
            kind: Some(MediaKind::Series),
            page: 1,
        };
        assert_eq!(
            pollster::block_on(provider.catalog(page)).unwrap(),
            provider.items
        );
        let other_kind = CatalogQuery {
            kind: Some(MediaKind::Anime),
            page: 1,
        };
        assert!(pollster::block_on(provider.catalog(other_kind))
            .unwrap()
            .is_empty());
        let zero_page = CatalogQuery {
            kind: None,
            page: 0,
        };
        assert_eq!(
            pollster::block_on(provider.catalog(zero_page)),
            Err(KinoError::NotFound)
        );

        assert_eq!(
            pollster::block_on(provider.details(&item)).unwrap(),
            provider.details
        );
        let missing = MediaItem { id: 999, ..item };
        assert_eq!(
            pollster::block_on(provider.details(&missing)),
            Err(KinoError::NotFound)
        );

        assert_eq!(
            pollster::block_on(provider.episodes(7, 1)).unwrap(),
            provider.seasons
        );
        assert_eq!(
            pollster::block_on(provider.episodes(7, 2)),
            Err(KinoError::NotFound)
        );

        let request = StreamRequest {
            id: 7,
            translator_id: 1,
            season: Some(1),
            episode: Some(2),
        };
        assert_eq!(
            pollster::block_on(provider.streams(request)).unwrap(),
            provider.source
        );
    }

    #[test]
    fn provider_futures_are_send() {
        let provider = MockProvider::new();
        assert_send(&provider.search("x"));
        assert_send(&provider.streams(StreamRequest {
            id: 1,
            translator_id: 1,
            season: None,
            episode: None,
        }));
    }

    #[test]
    fn catalog_query_roundtrip() {
        roundtrip(&CatalogQuery {
            kind: Some(MediaKind::Anime),
            page: 3,
        });
        roundtrip(&CatalogQuery {
            kind: None,
            page: 1,
        });
    }

    #[test]
    fn stream_request_roundtrip() {
        roundtrip(&StreamRequest {
            id: 7,
            translator_id: 1,
            season: Some(2),
            episode: Some(5),
        });
        roundtrip(&StreamRequest {
            id: 7,
            translator_id: 1,
            season: None,
            episode: None,
        });
    }
}
