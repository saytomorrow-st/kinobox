//! Реализация трейта [`kino_core::KinoProvider`] для [`HdrezkaApi`].
//!
//! `search`/`catalog`/`details` делегируют одноимённым inherent-методам
//! [`HdrezkaApi`]. `episodes` и `streams` используют данные плеера со страницы
//! (`cdn::extract_player_data`) и AJAX `/ajax/get_cdn_series/`.

use kino_core::{
    CatalogQuery, Collection, Episode, KinoError, KinoProvider, MediaDetails, MediaItem, Result,
    StreamQuality,
};

use crate::api::HdrezkaApi;
use crate::cdn::extract_player_data;
use crate::deobfuscate::deobfuscate;
use crate::parse::extract_favs;

impl KinoProvider for HdrezkaApi {
    async fn search(&self, query: &str) -> Result<Vec<MediaItem>> {
        HdrezkaApi::search(self, query, 1).await
    }

    async fn catalog(&self, query: CatalogQuery) -> Result<Vec<MediaItem>> {
        HdrezkaApi::catalog(self, query).await
    }

    async fn details(&self, item: &MediaItem) -> Result<MediaDetails> {
        HdrezkaApi::details(self, item).await
    }

    async fn collections(&self) -> Result<Vec<Collection>> {
        HdrezkaApi::collections(self).await
    }

    async fn collection_items(&self, collection: &Collection, page: u32) -> Result<Vec<MediaItem>> {
        HdrezkaApi::collection_items(self, collection, page).await
    }
    async fn episodes(&self, details: &MediaDetails) -> Result<Vec<Episode>> {
        if details.seasons.is_empty() {
            return Ok(Vec::new());
        }
        let translator_id = resolve_translator_id(details, None)?;
        let favs = details
            .raw_html
            .as_deref()
            .and_then(extract_favs)
            .unwrap_or_default();
        let map = self
            .episodes_list(&details.item, translator_id, &favs)
            .await?;
        let mut all: Vec<Episode> = map.into_values().flatten().collect();
        all.sort_by_key(|episode| (episode.season_id, episode.episode_id));
        Ok(all)
    }

    async fn streams(
        &self,
        details: &MediaDetails,
        translator_id: Option<u64>,
        episode: Option<&Episode>,
    ) -> Result<Vec<StreamQuality>> {
        match episode {
            Some(episode) => {
                let translator_id = resolve_translator_id(details, translator_id)?;
                let post_id = u32::try_from(details.item.id).map_err(|_| {
                    KinoError::Parse("id произведения не помещается в u32".to_string())
                })?;
                let favs = details
                    .raw_html
                    .as_deref()
                    .and_then(extract_favs)
                    .unwrap_or_default();
                self.stream_url(
                    post_id,
                    translator_id,
                    &favs,
                    Some(episode.season_id),
                    Some(episode.episode_id),
                )
                .await
            }
            None => {
                // Выбор другого перевода для фильма не реализован (нужен `action=get_movie`,
                // см. `ARCHITECTURE.md`): если вызывающий просит конкретный перевод, он должен
                // совпасть с тем, что встроен в данные плеера на странице, иначе `Parse`.
                let raw = details.raw_html.as_deref().ok_or_else(|| {
                    KinoError::Parse("нет HTML страницы для потоков фильма".to_string())
                })?;
                let player = extract_player_data(raw)?;
                if let Some(requested) = translator_id {
                    if u64::from(player.translator_id) != requested {
                        return Err(KinoError::Parse(
                            "выбор перевода для фильма не реализован: доступен только перевод \
                             со страницы"
                                .to_string(),
                        ));
                    }
                }
                deobfuscate(&player.streams)
            }
        }
    }
}

/// Перевод для запроса: `requested` (если указан) должен быть среди `details.translators`;
/// `None` — перевод по умолчанию (первый в списке).
fn resolve_translator_id(details: &MediaDetails, requested: Option<u64>) -> Result<u32> {
    let id = match requested {
        Some(id) => {
            details
                .translators
                .iter()
                .find(|translator| translator.id == id)
                .ok_or_else(|| KinoError::Parse("перевод не найден у произведения".to_string()))?
                .id
        }
        None => {
            details
                .translators
                .first()
                .ok_or_else(|| KinoError::Parse("нет переводов у произведения".to_string()))?
                .id
        }
    };
    u32::try_from(id).map_err(|_| KinoError::Parse("id перевода не помещается в u32".to_string()))
}
