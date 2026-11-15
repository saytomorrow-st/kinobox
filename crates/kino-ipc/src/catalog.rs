//! Каталог, поиск, детали и серии. Тонкий слой над `KinoProvider`: вся логика парсинга и
//! сети — в `kino-api`, здесь только DTO и выбор зеркала.

use kino_api::{MirrorStore, SecretStore};
use kino_core::{CatalogQuery, KinoProvider};

use crate::dto::{EpisodeDto, MediaDetailsDto, MediaItemDto, MediaKindDto};
use crate::error::IpcError;
use crate::service::AppService;

impl<S: SecretStore, M: MirrorStore> AppService<S, M> {
    /// Страница каталога; `page` нумеруется с 1. `kind` — `None` для всех типов.
    pub async fn catalog(
        &self,
        kind: Option<MediaKindDto>,
        page: u32,
    ) -> Result<Vec<MediaItemDto>, IpcError> {
        let provider = self.provider().await?;
        let query = CatalogQuery {
            kind: kind.map(Into::into),
            page,
        };
        let items = provider.catalog(query).await?;
        Ok(items.iter().map(MediaItemDto::from).collect())
    }

    /// Поиск по строке как есть; пустой запрос — `Ok(vec![])` без обращения к сети.
    pub async fn search(&self, query: String) -> Result<Vec<MediaItemDto>, IpcError> {
        let provider = self.provider().await?;
        // Явный путь трейта: у `HdrezkaApi` свой inherent `search(query, page)`,
        // который перекрывает одноимённый метод `KinoProvider` без пагинации.
        let items = KinoProvider::search(&provider, &query).await?;
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
        Ok(episodes.iter().map(EpisodeDto::from).collect())
    }
}
