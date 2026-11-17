//! Коллекции. Тонкий слой над `KinoProvider`: вся логика парсинга и сети — в `kino-api`,
//! здесь только DTO и выбор зеркала.

use kino_api::{MirrorStore, SecretStore};

use crate::dto::{CollectionDto, MediaItemDto};
use crate::error::IpcError;
use crate::service::AppService;

impl<S: SecretStore, M: MirrorStore> AppService<S, M> {
    /// Список курируемых коллекций с главной страницы.
    pub async fn collections(&self) -> Result<Vec<CollectionDto>, IpcError> {
        let provider = self.provider().await?;
        let collections = provider.collections().await?;
        log::debug!("collections: {} коллекций", collections.len());
        Ok(collections.iter().map(CollectionDto::from).collect())
    }

    /// Постеры конкретной коллекции, с пагинацией (`page` нумеруется с 1).
    pub async fn collection_items(
        &self,
        collection: CollectionDto,
        page: u32,
    ) -> Result<Vec<MediaItemDto>, IpcError> {
        let provider = self.provider().await?;
        let collection = collection.try_into_collection()?;
        let items = provider.collection_items(&collection, page).await?;
        log::debug!("collection_items: {} карточек", items.len());
        Ok(items.iter().map(MediaItemDto::from).collect())
    }
}
