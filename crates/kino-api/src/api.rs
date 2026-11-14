//! Высокоуровневая обёртка над HTTP-клиентом: поиск, каталог и детали.

use kino_core::{CatalogQuery, MediaDetails, MediaItem, Result};
use url::Url;

use crate::client::HttpClient;
use crate::parse::{catalog_url, parse_cards, parse_details, search_url};

/// Обёртка над зеркалом HDrezka. Хранит HTTP-клиент и корень зеркала.
///
/// Реализация трейта [`kino_core::KinoProvider`] появится в задаче #013 вместе
/// с методами `episodes` и `streams`.
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

    use wiremock::matchers::{method, path};
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
}
