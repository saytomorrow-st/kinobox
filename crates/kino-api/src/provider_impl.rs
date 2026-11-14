//! Реализация трейта [`kino_core::KinoProvider`] для [`HdrezkaApi`].
//!
//! `search`/`catalog`/`details` делегируют одноимённым inherent-методам
//! [`HdrezkaApi`]. `episodes` и `streams` используют данные плеера со страницы
//! (`cdn::extract_player_data`) и AJAX `/ajax/get_cdn_series/`.

use kino_core::{
    CatalogQuery, Episode, KinoError, KinoProvider, MediaDetails, MediaItem, Result, StreamQuality,
};

use crate::api::HdrezkaApi;
use crate::cdn::extract_player_data;
use crate::deobfuscate::deobfuscate;

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

    async fn episodes(&self, details: &MediaDetails) -> Result<Vec<Episode>> {
        if details.seasons.is_empty() {
            return Ok(Vec::new());
        }
        let translator_id = first_translator_id(details)?;
        let map = self.episodes_list(&details.item, translator_id).await?;
        let mut all: Vec<Episode> = map.into_values().flatten().collect();
        all.sort_by_key(|episode| (episode.season_id, episode.episode_id));
        Ok(all)
    }

    async fn streams(
        &self,
        details: &MediaDetails,
        episode: Option<&Episode>,
    ) -> Result<Vec<StreamQuality>> {
        match episode {
            Some(episode) => {
                let translator_id = first_translator_id(details)?;
                let post_id = u32::try_from(details.item.id).map_err(|_| {
                    KinoError::Parse("id произведения не помещается в u32".to_string())
                })?;
                self.stream_url(
                    post_id,
                    translator_id,
                    Some(episode.season_id),
                    Some(episode.episode_id),
                )
                .await
            }
            None => {
                let raw = details.raw_html.as_deref().ok_or_else(|| {
                    KinoError::Parse("нет HTML страницы для потоков фильма".to_string())
                })?;
                let player = extract_player_data(raw)?;
                deobfuscate(&player.streams)
            }
        }
    }
}

/// Первый перевод списка (перевод по умолчанию) как `u32`.
fn first_translator_id(details: &MediaDetails) -> Result<u32> {
    let translator = details
        .translators
        .first()
        .ok_or_else(|| KinoError::Parse("нет переводов у произведения".to_string()))?;
    u32::try_from(translator.id)
        .map_err(|_| KinoError::Parse("id перевода не помещается в u32".to_string()))
}
