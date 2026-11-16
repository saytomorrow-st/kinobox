//! Высокоуровневая обёртка над HTTP-клиентом: поиск, каталог и детали.

use std::collections::HashMap;

use kino_core::{CatalogQuery, Episode, KinoError, MediaDetails, MediaItem, Result, StreamQuality};
use url::Url;

use crate::cdn::parse_episodes_html;
use crate::client::HttpClient;
use crate::deobfuscate::deobfuscate;
use crate::parse::{catalog_url, parse_cards, parse_details, search_url};

/// Путь главного AJAX-эндпоинта HDrezka (`get_movie`/`get_stream`/`get_episodes`).
const CDN_SERIES_PATH: &str = "/ajax/get_cdn_series/";

/// Обёртка над зеркалом HDrezka. Хранит HTTP-клиент и корень зеркала.
///
/// Реализация трейта [`kino_core::KinoProvider`] лежит в `provider_impl` (#013).
#[derive(Clone)]
pub struct HdrezkaApi {
    client: HttpClient,
    base: Url,
}

impl std::fmt::Debug for HdrezkaApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HdrezkaApi")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl HdrezkaApi {
    /// Создаёт обёртку; `mirror` приводится к корню (`/`).
    pub fn new(client: HttpClient, mirror: Url) -> Self {
        Self {
            client,
            base: site_root(mirror),
        }
    }

    /// Корень зеркала, относительно которого строятся адреса каталога и поиска.
    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Поиск по строке как есть. Пустой запрос — `Ok(vec![])` без обращения к сети.
    pub async fn search(&self, query: &str, page: u32) -> Result<Vec<MediaItem>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let url = search_url(&self.base, query, page)?;
        let html = self.client.get(&url).await?.text();
        parse_cards(&html, &url)
    }

    /// Страница каталога; `page` нумеруется с 1.
    pub async fn catalog(&self, query: CatalogQuery) -> Result<Vec<MediaItem>> {
        let url = catalog_url(&self.base, query.kind, query.page)?;
        let html = self.client.get(&url).await?.text();
        parse_cards(&html, &url)
    }

    /// Подробная карточка по элементу каталога.
    pub async fn details(&self, item: &MediaItem) -> Result<MediaDetails> {
        self.details_at(&item.page_url).await
    }

    /// Подробная карточка по адресу страницы.
    pub async fn details_at(&self, page_url: &Url) -> Result<MediaDetails> {
        let html = self.client.get(page_url).await?.text();
        parse_details(&html, page_url)
    }

    /// Серии всех сезонов для перевода `translator_id` через
    /// `POST /ajax/get_cdn_series/` (`action=get_episodes`).
    ///
    /// Ответ `{"success":true,"episodes":{сезон: "<ul>…</ul>"}}`; при
    /// `success:false` или иной структуре — `KinoError::Parse`.
    pub async fn episodes_list(
        &self,
        item: &MediaItem,
        translator_id: u32,
    ) -> Result<HashMap<u32, Vec<Episode>>> {
        let url = self.cdn_series_url()?;
        let id = item.id.to_string();
        let translator = translator_id.to_string();
        let form = [
            ("id", id.as_str()),
            ("translator_id", translator.as_str()),
            ("action", "get_episodes"),
        ];
        let reply = self.client.post_form(&url, &form).await?;
        let value = parse_cdn_json(&reply.text(), "get_episodes")?;

        let episodes = value
            .get("episodes")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| KinoError::Parse("get_episodes: нет поля episodes".to_string()))?;

        let mut map = HashMap::new();
        for (season, html) in episodes {
            let season_id: u32 = season.parse().map_err(|_| {
                KinoError::Parse(format!("get_episodes: нечисловой сезон «{season}»"))
            })?;
            let html = html.as_str().ok_or_else(|| {
                KinoError::Parse(format!("get_episodes: сезон {season_id} — не строка"))
            })?;
            map.insert(season_id, parse_episodes_html(html, season_id)?);
        }
        Ok(map)
    }

    /// Потоки для перевода `translator_id` (и, для сериала, `season`/`episode`)
    /// через `POST /ajax/get_cdn_series/` (`action=get_stream`).
    ///
    /// Ответ `{"success":true,"url":"#h…"}` либо
    /// `{"success":false,"message":"…"}`. `url` проходит через деобфускацию.
    /// Поля `season`/`episode` добавляются в форму, только если заданы.
    pub async fn stream_url(
        &self,
        post_id: u32,
        translator_id: u32,
        season: Option<u32>,
        episode: Option<u32>,
    ) -> Result<Vec<StreamQuality>> {
        let url = self.cdn_series_url()?;
        let id = post_id.to_string();
        let translator = translator_id.to_string();
        let season_owned = season.map(|value| value.to_string());
        let episode_owned = episode.map(|value| value.to_string());

        let mut form: Vec<(&str, &str)> = vec![
            ("id", id.as_str()),
            ("translator_id", translator.as_str()),
            ("action", "get_stream"),
        ];
        if let Some(season) = season_owned.as_deref() {
            form.push(("season", season));
        }
        if let Some(episode) = episode_owned.as_deref() {
            form.push(("episode", episode));
        }

        let reply = self.client.post_form(&url, &form).await?;
        let value = parse_cdn_json(&reply.text(), "get_stream")?;
        let raw = value
            .get("url")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| KinoError::Parse("get_stream: нет поля url".to_string()))?;
        deobfuscate(raw)
    }

    fn cdn_series_url(&self) -> Result<Url> {
        self.base
            .join(CDN_SERIES_PATH)
            .map_err(|err| KinoError::Parse(format!("адрес get_cdn_series: {err}")))
    }
}

/// Разбирает JSON ответа `get_cdn_series` и проверяет `success:true`. При
/// `success:false` (или отсутствии булева `success`) — `Parse` с текстом `message`.
fn parse_cdn_json(body: &str, action: &str) -> Result<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|err| KinoError::Parse(format!("{action}: не JSON: {err}")))?;
    if value.get("success").and_then(serde_json::Value::as_bool) != Some(true) {
        let message = value
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("причина не указана");
        return Err(KinoError::Parse(format!("{action}: {message}")));
    }
    Ok(value)
}

fn site_root(mirror: Url) -> Url {
    let mut base = mirror;
    base.set_path("/");
    base.set_query(None);
    base.set_fragment(None);
    base
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wiremock::matchers::{body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::client::ClientConfig;

    fn client() -> HttpClient {
        HttpClient::new(ClientConfig {
            timeout: Duration::from_secs(5),
            max_retries: 0,
            base_delay: Duration::from_millis(1),
            user_agent: "kinobox-test".to_string(),
        })
        .expect("клиент")
    }

    fn card(id: u64, data_url: &str) -> String {
        format!(
            "<div class=\"b-content__inline_item\" data-id=\"{id}\" data-url=\"{data_url}\">\
             <div class=\"b-content__inline_item-link\"><a href=\"{data_url}\">Фильм {id}</a>\
             <div>2024, США, боевик</div></div>\
             <i class=\"entity\">Фильм</i></div>"
        )
    }

    async fn server() -> MockServer {
        MockServer::start().await
    }

    #[tokio::test]
    async fn empty_search_skips_network() {
        let server = server().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        assert_eq!(
            api.search("   ", 1).await.expect("пусто"),
            Vec::<MediaItem>::new()
        );
    }

    #[tokio::test]
    async fn search_sends_expected_path_and_query() {
        let server = server().await;
        Mock::given(method("GET"))
            .and(path("/search/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(card(7, "/films/7.html")))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        let items = api.search("the", 1).await.expect("поиск");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 7);
    }

    #[tokio::test]
    async fn catalog_sends_expected_path() {
        let server = server().await;
        Mock::given(method("GET"))
            .and(path("/films/page/2/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(card(9, "/films/9.html")))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        let query = CatalogQuery {
            kind: Some(kino_core::MediaKind::Movie),
            page: 2,
        };
        let items = api.catalog(query).await.expect("каталог");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 9);
    }

    #[tokio::test]
    async fn details_fetches_item_page() {
        let server = server().await;
        let body = r#"<input id="post_id" value="42">
            <div class="b-post__title"><h1>Фильм 42</h1></div>"#;
        Mock::given(method("GET"))
            .and(path("/films/42.html"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        let item = MediaItem {
            id: 42,
            title: "Фильм 42".to_string(),
            original_title: None,
            year: None,
            kind: kino_core::MediaKind::Movie,
            poster_url: None,
            page_url: Url::parse(&format!("{}/films/42.html", server.uri())).expect("url"),
            rating: None,
        };
        let details = api.details(&item).await.expect("детали");
        assert_eq!(details.item.id, 42);
    }

    #[tokio::test]
    async fn not_found_propagates() {
        let server = server().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        assert_eq!(
            api.search("x", 1).await,
            Err(kino_core::KinoError::NotFound)
        );
    }

    #[tokio::test]
    async fn login_page_is_auth_required() {
        let server = server().await;
        let login = r#"<html><body><form id="loginForm"></form></body></html>"#;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(login))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        assert_eq!(
            api.catalog(CatalogQuery {
                kind: None,
                page: 1
            })
            .await,
            Err(kino_core::KinoError::AuthRequired)
        );
    }

    fn sample_item(id: u64) -> MediaItem {
        MediaItem {
            id,
            title: "Сериал".to_string(),
            original_title: None,
            year: None,
            kind: kino_core::MediaKind::Series,
            poster_url: None,
            page_url: Url::parse(&format!("https://kvk.pub/series/{id}.html")).expect("url"),
            rating: None,
        }
    }

    #[tokio::test]
    async fn episodes_list_parses_seasons_and_episodes() {
        let server = server().await;
        let body = r#"{"success":true,"episodes":{"1":"<ul><li>1 серия</li><li>2 серия</li></ul>","2":"<ul><li>1 серия</li></ul>"}}"#;
        Mock::given(method("POST"))
            .and(path("/ajax/get_cdn_series/"))
            .and(body_string("id=7&translator_id=111&action=get_episodes"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        let map = api
            .episodes_list(&sample_item(7), 111)
            .await
            .expect("серии");
        assert_eq!(map.len(), 2);
        assert_eq!(map[&1].len(), 2);
        assert_eq!(map[&1][0].title, "1 серия");
        assert_eq!(map[&1][1].title, "2 серия");
        assert_eq!(map[&2].len(), 1);
        assert_eq!(map[&2][0].title, "1 серия");
    }

    #[tokio::test]
    async fn episodes_list_success_false_is_parse_error() {
        let server = server().await;
        let body = r#"{"success":false,"message":"нет доступа"}"#;
        Mock::given(method("POST"))
            .and(path("/ajax/get_cdn_series/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        let err = api
            .episodes_list(&sample_item(7), 111)
            .await
            .expect_err("ошибка");
        assert!(matches!(err, kino_core::KinoError::Parse(_)), "{err:?}");
        assert!(err.to_string().contains("нет доступа"), "{err}");
    }

    #[tokio::test]
    async fn stream_url_deobfuscates_obfuscated_url() {
        let server = server().await;
        let plain = "[720p]https://cdn.example.com/720.mp4";
        let obfuscated = format!(
            "#h{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, plain)
        );
        let body = format!(r#"{{"success":true,"url":"{obfuscated}"}}"#);
        Mock::given(method("POST"))
            .and(path("/ajax/get_cdn_series/"))
            .and(body_string(
                "id=85038&translator_id=111&action=get_stream&season=1&episode=2",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        let qualities = api
            .stream_url(85038, 111, Some(1), Some(2))
            .await
            .expect("потоки");
        assert_eq!(qualities.len(), 1);
        assert_eq!(qualities[0].label, "720p");
        assert_eq!(qualities[0].url.as_str(), "https://cdn.example.com/720.mp4");
    }

    #[tokio::test]
    async fn stream_url_success_false_is_parse_error() {
        let server = server().await;
        let body = r#"{"success":false,"message":"нужен премиум"}"#;
        Mock::given(method("POST"))
            .and(path("/ajax/get_cdn_series/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        let err = api
            .stream_url(85038, 111, None, None)
            .await
            .expect_err("ошибка");
        assert!(matches!(err, kino_core::KinoError::Parse(_)), "{err:?}");
        assert!(err.to_string().contains("нужен премиум"), "{err}");
    }

    #[tokio::test]
    async fn stream_url_omits_season_and_episode_when_none() {
        let server = server().await;
        let body = r#"{"success":true,"url":"[360p]https://cdn.example.com/360.mp4"}"#;
        Mock::given(method("POST"))
            .and(path("/ajax/get_cdn_series/"))
            .and(body_string("id=85038&translator_id=111&action=get_stream"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let api = HdrezkaApi::new(client(), Url::parse(&server.uri()).expect("url"));
        let qualities = api
            .stream_url(85038, 111, None, None)
            .await
            .expect("потоки");
        assert_eq!(qualities.len(), 1);
        assert_eq!(qualities[0].label, "360p");
    }
}
