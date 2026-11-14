//! Контракт источника данных: `kino-api` реализует, `kino-ipc` использует.

use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::media::{Episode, MediaDetails, MediaItem, MediaKind};
use crate::stream::StreamQuality;

/// Запрос страницы каталога.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogQuery {
    /// `None` — все типы.
    pub kind: Option<MediaKind>,
    /// Нумерация с 1.
    pub page: u32,
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

    /// Серии сериала для перевода по умолчанию. Для фильма (`seasons` пусто)
    /// возвращается `Ok(vec![])`. Может вернуть `NotFound`, `Parse`.
    fn episodes(&self, details: &MediaDetails)
        -> impl Future<Output = Result<Vec<Episode>>> + Send;

    /// Потоки уже после деобфускации, не сырая строка сайта. Для фильма `episode`
    /// — `None`, для сериала — выбранная серия. Может вернуть `NotFound`, `Parse`,
    /// `Deobfuscation` и `AuthRequired`.
    fn streams(
        &self,
        details: &MediaDetails,
        episode: Option<&Episode>,
    ) -> impl Future<Output = Result<Vec<StreamQuality>>> + Send;
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
        episodes: Vec<Episode>,
        qualities: Vec<StreamQuality>,
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
                    seasons: vec![crate::media::Season {
                        number: 1,
                        episodes: vec![1, 2],
                    }],
                    raw_html: None,
                },
                episodes: vec![
                    Episode {
                        season_id: 1,
                        episode_id: 1,
                        title: "1 серия".to_string(),
                    },
                    Episode {
                        season_id: 1,
                        episode_id: 2,
                        title: "2 серия".to_string(),
                    },
                ],
                qualities: vec![StreamQuality {
                    label: "720p".to_string(),
                    url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
                    premium: false,
                    hls: None,
                    fallbacks: vec![],
                }],
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

        async fn episodes(&self, details: &MediaDetails) -> Result<Vec<Episode>> {
            if details.item.id == self.details.item.id && !details.seasons.is_empty() {
                Ok(self.episodes.clone())
            } else if details.item.id == self.details.item.id {
                Ok(vec![])
            } else {
                Err(KinoError::NotFound)
            }
        }

        async fn streams(
            &self,
            details: &MediaDetails,
            _episode: Option<&Episode>,
        ) -> Result<Vec<StreamQuality>> {
            if details.item.id == self.details.item.id {
                Ok(self.qualities.clone())
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
            pollster::block_on(provider.episodes(&provider.details)).unwrap(),
            provider.episodes
        );
        let movie_details = MediaDetails {
            seasons: vec![],
            ..provider.details.clone()
        };
        assert!(pollster::block_on(provider.episodes(&movie_details))
            .unwrap()
            .is_empty());

        let episode = provider.episodes[0].clone();
        assert_eq!(
            pollster::block_on(provider.streams(&provider.details, Some(&episode))).unwrap(),
            provider.qualities
        );
        assert_eq!(
            pollster::block_on(provider.streams(&provider.details, None)).unwrap(),
            provider.qualities
        );
    }

    #[test]
    fn provider_futures_are_send() {
        let provider = MockProvider::new();
        let episode = provider.episodes[0].clone();
        assert_send(&provider.search("x"));
        assert_send(&provider.episodes(&provider.details));
        assert_send(&provider.streams(&provider.details, None));
        assert_send(&provider.streams(&provider.details, Some(&episode)));
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
}
