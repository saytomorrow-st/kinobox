//! DTO для UI. Пароля здесь нет ни в одном типе.

use std::time::Duration;

use kino_api::{MirrorState, MirrorStatus, ResolvedMirror};
use kino_core::{
    CatalogSort, Episode, MediaDetails, MediaItem, MediaKind, Season, StreamQuality, Translator,
    UserSession,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use specta_typescript::Number;
use url::Url;

use crate::error::IpcError;

const NEEDS_LOGIN_REASON: &str = "требуется вход";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    #[specta(type = Number)]
    pub user_id: u64,
    pub username: String,
}

impl From<&UserSession> for UserDto {
    fn from(session: &UserSession) -> Self {
        Self {
            user_id: session.user_id,
            username: session.username.clone(),
        }
    }
}

/// Состояние при запуске: что показать пользователю первым.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum BootState {
    /// Нужен экран входа; `email` — сохранённый адрес, если сайт отклонил вход.
    NeedsLogin {
        email: Option<String>,
    },
    SignedIn {
        user: UserDto,
    },
    /// Зеркала или сеть недоступны. Сохранённые данные не тронуты.
    Unavailable {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MirrorStateDto {
    Unknown,
    Open,
    NeedsLogin,
    Challenge,
    Dead,
}

impl From<MirrorState> for MirrorStateDto {
    fn from(state: MirrorState) -> Self {
        match state {
            MirrorState::Open => Self::Open,
            MirrorState::NeedsLogin => Self::NeedsLogin,
            MirrorState::Challenge => Self::Challenge,
            MirrorState::Dead => Self::Dead,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MirrorDto {
    pub url: String,
    pub state: MirrorStateDto,
    pub reason: Option<String>,
    #[specta(type = Option<Number>)]
    pub latency_ms: Option<u64>,
    pub active: bool,
    /// Запись получена пробой standby-адреса, а не из списка известных зеркал.
    pub standby: bool,
}

impl MirrorDto {
    pub(crate) fn unknown(url: &Url, active: bool) -> Self {
        Self {
            url: url.to_string(),
            state: MirrorStateDto::Unknown,
            reason: None,
            latency_ms: None,
            active,
            standby: false,
        }
    }

    pub(crate) fn from_status(status: &MirrorStatus, active: Option<&Url>, standby: bool) -> Self {
        Self {
            url: status.url.to_string(),
            state: status.state.into(),
            reason: status.reason.clone(),
            latency_ms: status.latency.map(millis),
            active: active.is_some_and(|active| same_mirror(active, &status.url)),
            standby,
        }
    }

    /// Зеркало, ставшее активным после `add_manual`.
    pub(crate) fn from_resolved(mirror: &ResolvedMirror) -> Self {
        Self {
            url: mirror.url.to_string(),
            state: mirror.state.into(),
            reason: (mirror.state == MirrorState::NeedsLogin)
                .then(|| NEEDS_LOGIN_REASON.to_string()),
            latency_ms: None,
            active: true,
            standby: false,
        }
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Тип произведения в каталоге HDrezka.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MediaKindDto {
    Movie,
    Series,
    Cartoon,
    Anime,
}

impl From<MediaKind> for MediaKindDto {
    fn from(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Movie => Self::Movie,
            MediaKind::Series => Self::Series,
            MediaKind::Cartoon => Self::Cartoon,
            MediaKind::Anime => Self::Anime,
        }
    }
}

impl From<MediaKindDto> for MediaKind {
    fn from(kind: MediaKindDto) -> Self {
        match kind {
            MediaKindDto::Movie => Self::Movie,
            MediaKindDto::Series => Self::Series,
            MediaKindDto::Cartoon => Self::Cartoon,
            MediaKindDto::Anime => Self::Anime,
        }
    }
}

/// Сортировка/фильтр каталога (соответствует `filter=` HDrezka).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CatalogSortDto {
    Last,
    Popular,
    Soon,
    Watching,
}

impl From<CatalogSort> for CatalogSortDto {
    fn from(sort: CatalogSort) -> Self {
        match sort {
            CatalogSort::Last => Self::Last,
            CatalogSort::Popular => Self::Popular,
            CatalogSort::Soon => Self::Soon,
            CatalogSort::Watching => Self::Watching,
        }
    }
}

impl From<CatalogSortDto> for CatalogSort {
    fn from(sort: CatalogSortDto) -> Self {
        match sort {
            CatalogSortDto::Last => Self::Last,
            CatalogSortDto::Popular => Self::Popular,
            CatalogSortDto::Soon => Self::Soon,
            CatalogSortDto::Watching => Self::Watching,
        }
    }
}

/// Карточка произведения в каталоге, поиске или избранном.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaItemDto {
    #[specta(type = Number)]
    pub id: u64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<u16>,
    pub kind: MediaKindDto,
    pub poster_url: Option<String>,
    pub page_url: String,
    pub rating: Option<f32>,
}

impl From<&MediaItem> for MediaItemDto {
    fn from(item: &MediaItem) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            original_title: item.original_title.clone(),
            year: item.year,
            kind: item.kind.into(),
            poster_url: item.poster_url.as_ref().map(ToString::to_string),
            page_url: item.page_url.to_string(),
            rating: item.rating,
        }
    }
}

impl MediaItemDto {
    /// Обратно в `MediaItem`: `kino-ipc` получает только то, что сам отдал, поэтому
    /// `page_url` считается корректным адресом.
    pub(crate) fn try_into_media_item(&self) -> Result<MediaItem, IpcError> {
        let page_url = Url::parse(&self.page_url)
            .map_err(|_| IpcError::invalid_input("Некорректный адрес страницы произведения"))?;
        let poster_url = match &self.poster_url {
            Some(raw) => Some(
                Url::parse(raw)
                    .map_err(|_| IpcError::invalid_input("Некорректный адрес постера"))?,
            ),
            None => None,
        };
        Ok(MediaItem {
            id: self.id,
            title: self.title.clone(),
            original_title: self.original_title.clone(),
            year: self.year,
            kind: self.kind.into(),
            poster_url,
            page_url,
            rating: self.rating,
        })
    }
}

/// Перевод (озвучка), доступный для произведения.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TranslatorDto {
    #[specta(type = Number)]
    pub id: u64,
    pub name: String,
}

impl From<&Translator> for TranslatorDto {
    fn from(translator: &Translator) -> Self {
        Self {
            id: translator.id,
            name: translator.name.clone(),
        }
    }
}

/// Идентификатор перевода HDrezka для команд, принимающих `Option`-идентификатор.
/// Как и `TranslatorDto::id`, экспортируется в TypeScript как `number`: ID укладываются в
/// JS-число (до 2^53-1), потерь точности нет. Обёртка нужна, потому что specta запрещает
/// голый `u64`/`i64` в сигнатурах команд (см. «Specta и u64» в `CRATE.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct TranslatorId(#[specta(type = Number)] pub u64);

/// Сезон сериала: номер и список номеров серий.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SeasonDto {
    pub number: u16,
    pub episodes: Vec<u16>,
}

impl From<&Season> for SeasonDto {
    fn from(season: &Season) -> Self {
        Self {
            number: season.number,
            episodes: season.episodes.clone(),
        }
    }
}

/// Подробная карточка произведения. `rawHtml` ядра сюда не попадает: поле отсутствует
/// как таковое (см. `media_details_dto_has_no_raw_html_field`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaDetailsDto {
    pub item: MediaItemDto,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub duration_minutes: Option<u32>,
    pub translators: Vec<TranslatorDto>,
    pub seasons: Vec<SeasonDto>,
}

impl From<&MediaDetails> for MediaDetailsDto {
    fn from(details: &MediaDetails) -> Self {
        Self {
            item: MediaItemDto::from(&details.item),
            description: details.description.clone(),
            genres: details.genres.clone(),
            countries: details.countries.clone(),
            duration_minutes: details.duration_minutes,
            translators: details
                .translators
                .iter()
                .map(TranslatorDto::from)
                .collect(),
            seasons: details.seasons.iter().map(SeasonDto::from).collect(),
        }
    }
}

impl MediaDetailsDto {
    /// Обратно в `MediaDetails` для вызова `KinoProvider::episodes`. `raw_html` остаётся
    /// `None`: DTO его не несёт, а `episodes` (в отличие от `streams` для фильма) в нём
    /// не нуждается.
    pub(crate) fn try_into_media_details(&self) -> Result<MediaDetails, IpcError> {
        Ok(MediaDetails {
            item: self.item.try_into_media_item()?,
            description: self.description.clone(),
            genres: self.genres.clone(),
            countries: self.countries.clone(),
            duration_minutes: self.duration_minutes,
            translators: self
                .translators
                .iter()
                .map(|translator| Translator {
                    id: translator.id,
                    name: translator.name.clone(),
                })
                .collect(),
            seasons: self
                .seasons
                .iter()
                .map(|season| Season {
                    number: season.number,
                    episodes: season.episodes.clone(),
                })
                .collect(),
            raw_html: None,
        })
    }
}

/// Одна серия сериала.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeDto {
    pub season_id: u32,
    pub episode_id: u32,
    pub title: String,
}

impl From<&Episode> for EpisodeDto {
    fn from(episode: &Episode) -> Self {
        Self {
            season_id: episode.season_id,
            episode_id: episode.episode_id,
            title: episode.title.clone(),
        }
    }
}

impl From<&EpisodeDto> for Episode {
    fn from(episode: &EpisodeDto) -> Self {
        Self {
            season_id: episode.season_id,
            episode_id: episode.episode_id,
            title: episode.title.clone(),
        }
    }
}

/// Один вариант качества потока после деобфускации.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StreamQualityDto {
    pub label: String,
    pub url: String,
    pub premium: bool,
    pub hls: Option<String>,
    pub fallbacks: Vec<String>,
}

impl From<&StreamQuality> for StreamQualityDto {
    fn from(quality: &StreamQuality) -> Self {
        Self {
            label: quality.label.clone(),
            url: quality.url.to_string(),
            premium: quality.premium,
            hls: quality.hls.as_ref().map(ToString::to_string),
            fallbacks: quality.fallbacks.iter().map(ToString::to_string).collect(),
        }
    }
}

impl StreamQualityDto {
    /// Обратно в `StreamQuality`: `kino-ipc` получает только то, что сам отдал, поэтому адреса
    /// считаются корректными. Используется `src-tauri` (#021) для загрузки потока в `kino-player`.
    pub fn try_into_stream_quality(&self) -> Result<StreamQuality, IpcError> {
        let url = Url::parse(&self.url)
            .map_err(|_| IpcError::invalid_input("Некорректный адрес потока"))?;
        let hls = match &self.hls {
            Some(raw) => Some(
                Url::parse(raw)
                    .map_err(|_| IpcError::invalid_input("Некорректный HLS-манифест"))?,
            ),
            None => None,
        };
        let fallbacks = self
            .fallbacks
            .iter()
            .map(|raw| {
                Url::parse(raw)
                    .map_err(|_| IpcError::invalid_input("Некорректный резервный адрес потока"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(StreamQuality {
            label: self.label.clone(),
            url,
            premium: self.premium,
            hls,
            fallbacks,
        })
    }
}

/// Снимок прогресса воспроизведения, который `src-tauri` эмиттит в окно `player-controls`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayerProgressDto {
    pub position: Option<f64>,
    pub duration: Option<f64>,
    pub paused: bool,
}

/// Сообщение об асинхронной ошибке mpv (после принятого `loadfile`), которое `src-tauri`
/// эмиттит в окно `player-controls` событием `player-error`. `message` — человекочитаемый
/// русский текст для UI (без технических деталей mpv и без утечки URL/токенов потока).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayerErrorDto {
    pub message: String,
}

/// Те же зеркало: схема, хост и порт; путь и запрос не важны.
pub(crate) fn same_mirror(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::json;

    use super::*;
    use crate::error::IpcErrorKind;

    fn roundtrip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let text = serde_json::to_string(value).expect("json");
        let back: T = serde_json::from_str(&text).expect("разбор");
        assert_eq!(*value, back);
    }

    fn user() -> UserDto {
        UserDto {
            user_id: 42,
            username: "kino".to_string(),
        }
    }

    #[test]
    fn boot_state_json_is_tagged_camel_case() {
        let cases = [
            (
                BootState::NeedsLogin { email: None },
                json!({"state": "needsLogin", "email": null}),
            ),
            (
                BootState::NeedsLogin {
                    email: Some("a@b.c".to_string()),
                },
                json!({"state": "needsLogin", "email": "a@b.c"}),
            ),
            (
                BootState::SignedIn { user: user() },
                json!({"state": "signedIn", "user": {"userId": 42, "username": "kino"}}),
            ),
            (
                BootState::Unavailable {
                    message: "нет сети".to_string(),
                },
                json!({"state": "unavailable", "message": "нет сети"}),
            ),
        ];
        for (state, expected) in cases {
            assert_eq!(serde_json::to_value(&state).expect("json"), expected);
            roundtrip(&state);
        }
    }

    #[test]
    fn mirror_json_is_camel_case() {
        let mirror = MirrorDto {
            url: "https://kvk.pub/".to_string(),
            state: MirrorStateDto::NeedsLogin,
            reason: None,
            latency_ms: Some(120),
            active: true,
            standby: false,
        };
        assert_eq!(
            serde_json::to_value(&mirror).expect("json"),
            json!({
                "url": "https://kvk.pub/",
                "state": "needsLogin",
                "reason": null,
                "latencyMs": 120,
                "active": true,
                "standby": false,
            })
        );
        roundtrip(&mirror);
    }

    #[test]
    fn mirror_states_serialize_as_camel_case_words() {
        let all = [
            (MirrorStateDto::Unknown, "unknown"),
            (MirrorStateDto::Open, "open"),
            (MirrorStateDto::NeedsLogin, "needsLogin"),
            (MirrorStateDto::Challenge, "challenge"),
            (MirrorStateDto::Dead, "dead"),
        ];
        for (state, word) in all {
            assert_eq!(serde_json::to_value(state).expect("json"), json!(word));
        }
    }

    #[test]
    fn same_mirror_ignores_path_and_default_port() {
        let parse = |raw: &str| Url::parse(raw).expect("url");
        assert!(same_mirror(
            &parse("https://kvk.pub/"),
            &parse("https://kvk.pub:443/films/")
        ));
        assert!(!same_mirror(
            &parse("https://kvk.pub/"),
            &parse("http://kvk.pub/")
        ));
        assert!(!same_mirror(
            &parse("https://kvk.pub/"),
            &parse("https://rezka.fi/")
        ));
    }

    fn media_item() -> MediaItem {
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

    fn media_details() -> MediaDetails {
        MediaDetails {
            item: media_item(),
            description: Some("Описание".to_string()),
            genres: vec!["фантастика".to_string()],
            countries: vec!["США".to_string()],
            duration_minutes: Some(169),
            translators: vec![Translator {
                id: 1,
                name: "Дублированный".to_string(),
            }],
            seasons: vec![Season {
                number: 1,
                episodes: vec![1, 2],
            }],
            raw_html: Some("<html>секретная страница</html>".to_string()),
        }
    }

    #[test]
    fn media_kind_dto_round_trips_through_core_and_json() {
        for kind in [
            MediaKind::Movie,
            MediaKind::Series,
            MediaKind::Cartoon,
            MediaKind::Anime,
        ] {
            let dto = MediaKindDto::from(kind);
            assert_eq!(MediaKind::from(dto), kind);
            roundtrip(&dto);
        }
        assert_eq!(
            serde_json::to_value(MediaKindDto::Cartoon).unwrap(),
            json!("cartoon")
        );
    }

    #[test]
    fn media_item_dto_is_camel_case_and_round_trips_both_ways() {
        let item = media_item();
        let dto = MediaItemDto::from(&item);
        let value = serde_json::to_value(&dto).unwrap();
        // `rating` сравнивается отдельно: `f32 8.7` как `f64` в JSON не точно `8.7`.
        assert_eq!(value["rating"].as_f64().unwrap(), f64::from(8.7_f32));
        assert_eq!(
            value,
            json!({
                "id": 1234,
                "title": "Интерстеллар",
                "originalTitle": "Interstellar",
                "year": 2014,
                "kind": "movie",
                "posterUrl": "https://hdrezka.ag/poster/1.jpg",
                "pageUrl": "https://hdrezka.ag/films/1234-interstellar.html",
                "rating": f64::from(8.7_f32),
            })
        );
        roundtrip(&dto);
        assert_eq!(dto.try_into_media_item().unwrap(), item);
    }

    #[test]
    fn media_item_dto_rejects_bad_urls_on_the_way_back() {
        let mut dto = MediaItemDto::from(&media_item());
        dto.page_url = "не url".to_string();
        let err = dto.try_into_media_item().unwrap_err();
        assert_eq!(err.kind, IpcErrorKind::InvalidInput);

        let mut dto = MediaItemDto::from(&media_item());
        dto.poster_url = Some("не url".to_string());
        let err = dto.try_into_media_item().unwrap_err();
        assert_eq!(err.kind, IpcErrorKind::InvalidInput);
    }

    #[test]
    fn media_details_dto_round_trips_and_drops_raw_html() {
        let details = media_details();
        let dto = MediaDetailsDto::from(&details);
        let json = serde_json::to_string(&dto).unwrap();
        assert!(!json.contains("rawHtml"));
        assert!(!json.contains("raw_html"));
        assert!(!json.contains("секретная страница"));
        roundtrip(&dto);

        let back = dto.try_into_media_details().unwrap();
        assert_eq!(back.item, details.item);
        assert_eq!(back.description, details.description);
        assert_eq!(back.genres, details.genres);
        assert_eq!(back.countries, details.countries);
        assert_eq!(back.duration_minutes, details.duration_minutes);
        assert_eq!(back.translators, details.translators);
        assert_eq!(back.seasons, details.seasons);
        assert!(back.raw_html.is_none());
    }

    /// `MediaDetailsDto` физически не содержит `raw_html` ни под каким именем: сериализация
    /// в `serde_json::Value` даёт объект с ровно этими ключами, других полей нет в принципе
    /// (не только скрыты сериализацией, как в `kino_core::MediaDetails`).
    #[test]
    fn media_details_dto_has_no_raw_html_field_at_all() {
        let dto = MediaDetailsDto::from(&media_details());
        let value = serde_json::to_value(&dto).unwrap();
        let object = value.as_object().unwrap();
        let expected: std::collections::HashSet<&str> = [
            "item",
            "description",
            "genres",
            "countries",
            "durationMinutes",
            "translators",
            "seasons",
        ]
        .into_iter()
        .collect();
        let actual: std::collections::HashSet<&str> = object.keys().map(String::as_str).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn translator_and_season_and_episode_dto_are_camel_case() {
        let translator = TranslatorDto::from(&Translator {
            id: 1,
            name: "Дублированный".to_string(),
        });
        assert_eq!(
            serde_json::to_value(&translator).unwrap(),
            json!({"id": 1, "name": "Дублированный"})
        );
        roundtrip(&translator);

        let season = SeasonDto::from(&Season {
            number: 1,
            episodes: vec![1, 2, 3],
        });
        assert_eq!(
            serde_json::to_value(&season).unwrap(),
            json!({"number": 1, "episodes": [1, 2, 3]})
        );
        roundtrip(&season);

        let episode = EpisodeDto::from(&Episode {
            season_id: 1,
            episode_id: 2,
            title: "2 серия".to_string(),
        });
        assert_eq!(
            serde_json::to_value(&episode).unwrap(),
            json!({"seasonId": 1, "episodeId": 2, "title": "2 серия"})
        );
        roundtrip(&episode);
    }

    #[test]
    fn episode_dto_round_trips_back_to_core() {
        let episode = Episode {
            season_id: 1,
            episode_id: 2,
            title: "2 серия".to_string(),
        };
        let dto = EpisodeDto::from(&episode);
        assert_eq!(Episode::from(&dto), episode);
    }

    #[test]
    fn stream_quality_dto_is_camel_case_and_round_trips_both_ways() {
        let quality = StreamQuality {
            label: "720p".to_string(),
            url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
            premium: false,
            hls: Some(Url::parse("https://cdn.example.com/720.m3u8").unwrap()),
            fallbacks: vec![Url::parse("https://cdn.example.com/720-b.mp4").unwrap()],
        };
        let dto = StreamQualityDto::from(&quality);
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({
                "label": "720p",
                "url": "https://cdn.example.com/720.mp4",
                "premium": false,
                "hls": "https://cdn.example.com/720.m3u8",
                "fallbacks": ["https://cdn.example.com/720-b.mp4"],
            })
        );
        roundtrip(&dto);
        assert_eq!(dto.try_into_stream_quality().unwrap(), quality);
    }

    #[test]
    fn stream_quality_dto_rejects_bad_urls_on_the_way_back() {
        let mut dto = StreamQualityDto::from(&StreamQuality {
            label: "720p".to_string(),
            url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
            premium: false,
            hls: None,
            fallbacks: vec![],
        });
        dto.url = "не url".to_string();
        let err = dto.try_into_stream_quality().unwrap_err();
        assert_eq!(err.kind, IpcErrorKind::InvalidInput);

        let mut dto = StreamQualityDto::from(&StreamQuality {
            label: "720p".to_string(),
            url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
            premium: false,
            hls: None,
            fallbacks: vec![],
        });
        dto.hls = Some("не url".to_string());
        let err = dto.try_into_stream_quality().unwrap_err();
        assert_eq!(err.kind, IpcErrorKind::InvalidInput);

        let mut dto = StreamQualityDto::from(&StreamQuality {
            label: "720p".to_string(),
            url: Url::parse("https://cdn.example.com/720.mp4").unwrap(),
            premium: false,
            hls: None,
            fallbacks: vec![],
        });
        dto.fallbacks = vec!["не url".to_string()];
        let err = dto.try_into_stream_quality().unwrap_err();
        assert_eq!(err.kind, IpcErrorKind::InvalidInput);
    }

    #[test]
    fn player_progress_dto_is_camel_case() {
        let progress = PlayerProgressDto {
            position: Some(12.5),
            duration: Some(90.0),
            paused: true,
        };
        assert_eq!(
            serde_json::to_value(progress).unwrap(),
            json!({"position": 12.5, "duration": 90.0, "paused": true})
        );
        roundtrip(&progress);

        let empty = PlayerProgressDto {
            position: None,
            duration: None,
            paused: false,
        };
        assert_eq!(
            serde_json::to_value(empty).unwrap(),
            json!({"position": null, "duration": null, "paused": false})
        );
    }

    #[test]
    fn player_error_dto_is_camel_case() {
        let error = PlayerErrorDto {
            message: "Не удалось воспроизвести поток.".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            json!({"message": "Не удалось воспроизвести поток."})
        );
        roundtrip(&error);
    }
}
