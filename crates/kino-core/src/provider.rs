//! Контракт источника данных: `kino-api` реализует, `kino-ipc` использует.

use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::media::{Collection, Episode, MediaDetails, MediaItem, MediaKind};
use crate::stream::StreamQuality;

/// Сортировка/фильтр каталога (соответствует `filter=` HDrezka).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSort {
    Last,
    Popular,
    Soon,
    Watching,
}

impl CatalogSort {
    /// Значение для query-параметра `filter=` (совпадает с serde `snake_case`).
    pub fn as_query_value(self) -> &'static str {
        match self {
            CatalogSort::Last => "last",
            CatalogSort::Popular => "popular",
            CatalogSort::Soon => "soon",
            CatalogSort::Watching => "watching",
        }
    }
}

/// Запрос страницы каталога.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogQuery {
    /// `None` — все типы.
    pub kind: Option<MediaKind>,
    /// Нумерация с 1.
    pub page: u32,
    /// `None` — дефолт сайта (`last`), в URL не попадает.
    pub sort: Option<CatalogSort>,
    /// Slug жанра (напр. `"action"`); `None` — без фильтра.
    pub genre: Option<String>,
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

    /// Страница каталога; `page` нумеруется с 1. `query.sort` — сортировка
    /// (`None` — дефолт сайта), `query.genre` — slug жанра (`None` — без фильтра).
    /// Может вернуть `Parse`.
    fn catalog(&self, query: CatalogQuery) -> impl Future<Output = Result<Vec<MediaItem>>> + Send;

    /// Подробная карточка. Может вернуть `NotFound` и `Parse`.
    fn details(&self, item: &MediaItem) -> impl Future<Output = Result<MediaDetails>> + Send;

    /// Серии сериала для перевода по умолчанию. Для фильма (`seasons` пусто)
    /// возвращается `Ok(vec![])`. Может вернуть `NotFound`, `Parse`.
    fn episodes(&self, details: &MediaDetails)
        -> impl Future<Output = Result<Vec<Episode>>> + Send;

    /// Потоки уже после деобфускации, не сырая строка сайта. Для фильма `episode`
    /// — `None`, для сериала — выбранная серия. `translator_id` — `None` берёт перевод
    /// по умолчанию (`details.translators[0]`); `Some(id)` должен быть среди
    /// `details.translators`, иначе `Parse`. Для фильма (`episode = None`) выбор другого
    /// перевода не реализован (нужен `action=get_movie`, см. `ARCHITECTURE.md`) — `Some(id)`,
    /// отличный от перевода встроенных на странице данных плеера, тоже даёт `Parse`.
    /// Может вернуть `NotFound`, `Parse`, `Deobfuscation` и `AuthRequired`.
    fn streams(
        &self,
        details: &MediaDetails,
        translator_id: Option<u64>,
        episode: Option<&Episode>,
    ) -> impl Future<Output = Result<Vec<StreamQuality>>> + Send;

    /// Список курируемых коллекций с главной страницы. Может вернуть `Parse`.
    fn collections(&self) -> impl Future<Output = Result<Vec<Collection>>> + Send;

    /// Постеры конкретной коллекции, с пагинацией (`page` нумеруется с 1).
    /// Может вернуть `Parse`.
    fn collection_items(
        &self,
        collection: &Collection,
        page: u32,
    ) -> impl Future<Output = Result<Vec<MediaItem>>> + Send;
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
        collections: Vec<Collection>,
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
                        premium: false,
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
                collections: vec![Collection {
                    id: 5,
                    title: "Оскар 2026".to_string(),
                    url: Url::parse("https://kvk.pub/collections/5-oskar-2026/").unwrap(),
                    poster_url: Some(Url::parse("https://kvk.pub/collections/5.jpg").unwrap()),
                    items_count: Some(24),
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
            translator_id: Option<u64>,
            _episode: Option<&Episode>,
        ) -> Result<Vec<StreamQuality>> {
            if details.item.id != self.details.item.id {
                return Err(KinoError::NotFound);
            }
            if let Some(id) = translator_id {
                if !details.translators.iter().any(|t| t.id == id) {
                    return Err(KinoError::Parse("перевод не найден".to_string()));
                }
            }
            Ok(self.qualities.clone())
        }

        async fn collections(&self) -> Result<Vec<Collection>> {
            Ok(self.collections.clone())
        }

        async fn collection_items(
            &self,
            collection: &Collection,
            page: u32,
        ) -> Result<Vec<MediaItem>> {
            if page == 0 {
                return Err(KinoError::NotFound);
            }
            if collection.id == self.collections[0].id {
                Ok(self.items.clone())
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
            sort: None,
            genre: None,
        };
        assert_eq!(
            pollster::block_on(provider.catalog(page)).unwrap(),
            provider.items
        );
        let other_kind = CatalogQuery {
            kind: Some(MediaKind::Anime),
            page: 1,
            sort: None,
            genre: None,
        };
        assert!(pollster::block_on(provider.catalog(other_kind))
            .unwrap()
            .is_empty());
        let zero_page = CatalogQuery {
            kind: None,
            page: 0,
            sort: None,
            genre: None,
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
            pollster::block_on(provider.streams(&provider.details, None, Some(&episode))).unwrap(),
            provider.qualities
        );
        assert_eq!(
            pollster::block_on(provider.streams(&provider.details, None, None)).unwrap(),
            provider.qualities
        );
        let translator_id = provider.details.translators[0].id;
        assert_eq!(
            pollster::block_on(provider.streams(&provider.details, Some(translator_id), None))
                .unwrap(),
            provider.qualities
        );
        assert_eq!(
            pollster::block_on(provider.streams(&provider.details, Some(999), None)),
            Err(KinoError::Parse("перевод не найден".to_string()))
        );

        assert_eq!(
            pollster::block_on(provider.collections()).unwrap(),
            provider.collections
        );
        let collection = provider.collections[0].clone();
        assert_eq!(
            pollster::block_on(provider.collection_items(&collection, 1)).unwrap(),
            provider.items
        );
        assert_eq!(
            pollster::block_on(provider.collection_items(&collection, 0)),
            Err(KinoError::NotFound)
        );
        let missing = Collection {
            id: 999,
            ..collection
        };
        assert_eq!(
            pollster::block_on(provider.collection_items(&missing, 1)),
            Err(KinoError::NotFound)
        );
    }

    #[test]
    fn provider_futures_are_send() {
        let provider = MockProvider::new();
        let episode = provider.episodes[0].clone();
        let collection = provider.collections[0].clone();
        assert_send(&provider.search("x"));
        assert_send(&provider.episodes(&provider.details));
        assert_send(&provider.streams(&provider.details, None, None));
        assert_send(&provider.streams(&provider.details, None, Some(&episode)));
        assert_send(&provider.collections());
        assert_send(&provider.collection_items(&collection, 1));
    }

    #[test]
    fn catalog_query_roundtrip() {
        roundtrip(&CatalogQuery {
            kind: Some(MediaKind::Anime),
            page: 3,
            sort: Some(CatalogSort::Popular),
            genre: Some("action".to_string()),
        });
        roundtrip(&CatalogQuery {
            kind: None,
            page: 1,
            sort: None,
            genre: None,
        });
    }

    #[test]
    fn catalog_sort_roundtrip_and_snake_case() {
        for (sort, json) in [
            (CatalogSort::Last, "\"last\""),
            (CatalogSort::Popular, "\"popular\""),
            (CatalogSort::Soon, "\"soon\""),
            (CatalogSort::Watching, "\"watching\""),
        ] {
            assert_eq!(serde_json::to_string(&sort).unwrap(), json);
            roundtrip(&sort);
            assert_eq!(sort.as_query_value(), json.trim_matches('"'));
        }
    }
}
