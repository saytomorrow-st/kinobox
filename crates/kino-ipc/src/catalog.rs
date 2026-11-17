//! Каталог, поиск, детали, серии и потоки. Тонкий слой над `KinoProvider`: вся логика парсинга и
//! сети — в `kino-api`, здесь только DTO и выбор зеркала.

use kino_api::{MirrorStore, SecretStore};
use kino_core::{CatalogQuery, Episode, KinoProvider};

use crate::dto::{
    CatalogSortDto, EpisodeDto, MediaDetailsDto, MediaItemDto, MediaKindDto, StreamQualityDto,
};
use crate::error::IpcError;
use crate::service::AppService;

impl<S: SecretStore, M: MirrorStore> AppService<S, M> {
    /// Страница каталога; `page` нумеруется с 1. `kind` — `None` для всех типов,
    /// `genre` — slug жанра (`None` — без фильтра), `sort` — `None` для дефолта сайта.
    pub async fn catalog(
        &self,
        kind: Option<MediaKindDto>,
        page: u32,
        sort: Option<CatalogSortDto>,
        genre: Option<String>,
    ) -> Result<Vec<MediaItemDto>, IpcError> {
        let provider = self.provider().await?;
        let query = CatalogQuery {
            kind: kind.map(Into::into),
            page,
            sort: sort.map(Into::into),
            genre,
        };
        let items = provider.catalog(query).await?;
        log::debug!("catalog: {} карточек", items.len());
        Ok(items.iter().map(MediaItemDto::from).collect())
    }

    /// Поиск по строке как есть; пустой запрос — `Ok(vec![])` без обращения к сети.
    pub async fn search(&self, query: String) -> Result<Vec<MediaItemDto>, IpcError> {
        let provider = self.provider().await?;
        // Явный путь трейта: у `HdrezkaApi` свой inherent `search(query, page)`,
        // который перекрывает одноимённый метод `KinoProvider` без пагинации.
        let items = KinoProvider::search(&provider, &query).await?;
        log::debug!("search: {} результатов", items.len());
        Ok(items.iter().map(MediaItemDto::from).collect())
    }

    /// Подробная карточка по элементу каталога.
    pub async fn details(&self, item: MediaItemDto) -> Result<MediaDetailsDto, IpcError> {
        let provider = self.provider().await?;
        let item = item.try_into_media_item()?;
        let details = provider.details(&item).await?;
        Ok(MediaDetailsDto::from(&details))
    }

    /// Серии сериала для перевода по умолчанию; для фильма — `Ok(vec![])`.
    pub async fn episodes(&self, details: MediaDetailsDto) -> Result<Vec<EpisodeDto>, IpcError> {
        let provider = self.provider().await?;
        let details = details.try_into_media_details()?;
        let episodes = provider.episodes(&details).await?;
        log::info!("episodes: {} серий", episodes.len());
        Ok(episodes.iter().map(EpisodeDto::from).collect())
    }

    /// Потоки уже после деобфускации для фильма (`episode = None`) или серии сериала
    /// (`episode = Some`). `translator_id = None` — перевод по умолчанию. Субтитры этим
    /// методом не возвращаются (`KinoProvider::streams` отдаёт только `Vec<StreamQuality>`).
    ///
    /// Детали перезапрашиваются внутри: для потоков фильма нужен `raw_html` страницы
    /// (данные плеера), а `MediaDetailsDto` его физически не несёт. `raw_html` остаётся в
    /// памяти и никогда не пересекает IPC — по сети он не сериализуется.
    pub async fn streams(
        &self,
        item: MediaItemDto,
        translator_id: Option<u64>,
        episode: Option<EpisodeDto>,
    ) -> Result<Vec<StreamQualityDto>, IpcError> {
        let provider = self.provider().await?;
        let item = item.try_into_media_item()?;
        let details = provider.details(&item).await?;
        let episode: Option<Episode> = episode.as_ref().map(Episode::from);
        let streams = provider
            .streams(&details, translator_id, episode.as_ref())
            .await?;
        log::info!("streams: {} качеств", streams.len());
        Ok(streams.iter().map(StreamQualityDto::from).collect())
    }
}
