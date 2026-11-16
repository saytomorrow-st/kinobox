//! Разбор карточек каталога/поиска (`div.b-content__inline_item`) и пагинации.
//!
//! Селекторы карточки НЕ проверены на реальных страницах каталога и поиска.
//! Источники: исходники двух открытых библиотек (`ndenissov/HDRezka`,
//! `kristal374/hdrezka-api`, обе MIT) и просмотр главной kvk.pub 2026-11-14
//! (на главной подтверждён только контейнер карточки с атрибутами `data-id`
//! и `data-url`).

use kino_core::{KinoError, MediaItem, MediaKind, Result};
use scraper::{Html, Selector};
use url::Url;

use super::{
    first_path_segment, is_login_page, kind_from_entity, kind_from_segment, parse_rating,
    parse_year, resolve_url, selector,
};

const CARD: &str = "div.b-content__inline_item";
const CARD_LINK: &str = ".b-content__inline_item-link a";
const CARD_INFO: &str = ".b-content__inline_item-link div";
const CARD_POSTER: &str = ".b-content__inline_item-cover img";
const CARD_RATING: &str = "i.b-category-bestrating";
const CARD_KIND: &str = "i.entity";

/// Каталожная секция для типа произведения.
fn section(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "films",
        MediaKind::Series => "series",
        MediaKind::Cartoon => "cartoons",
        MediaKind::Anime => "animation",
    }
}

/// URL страницы каталога. Без `kind` — главная; `page` нумеруется с 1,
/// при `page == 1` номер в путь не добавляется.
pub fn catalog_url(base: &Url, kind: Option<MediaKind>, page: u32) -> Result<Url> {
    let mut path = String::from("/");
    if let Some(kind) = kind {
        path.push_str(section(kind));
        path.push('/');
    }
    if page > 1 {
        path.push_str("page/");
        path.push_str(&page.to_string());
        path.push('/');
    }
    let mut url = base.clone();
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// Разбирает карточки со страницы каталога или поиска. Относительные ссылки
/// (`data-url`, постеры) разрешаются относительно `base`.
///
/// Страница входа даёт `AuthRequired`. Карточки без `data-id`, `data-url` или
/// названия пропускаются; пустая страница (без карточек) — `Ok(vec![])`.
pub fn parse_cards(html: &str, base: &Url) -> Result<Vec<MediaItem>> {
    if is_login_page(html) {
        return Err(KinoError::AuthRequired);
    }
    let document = Html::parse_document(html);
    let card_sel = selector(CARD)?;
    let link_sel = selector(CARD_LINK)?;
    let info_sel = selector(CARD_INFO)?;
    let poster_sel = selector(CARD_POSTER)?;
    let rating_sel = selector(CARD_RATING)?;
    let kind_sel = selector(CARD_KIND)?;

    let mut items = Vec::new();
    for card in document.select(&card_sel) {
        let Some(id) = card
            .value()
            .attr("data-id")
            .and_then(|value| value.trim().parse::<u64>().ok())
        else {
            continue;
        };
        let Some(raw_url) = card.value().attr("data-url") else {
            continue;
        };
        let Some(page_url) = resolve_url(base, raw_url) else {
            continue;
        };
        let Some(title) = card
            .select(&link_sel)
            .next()
            .map(|link| link.text().collect::<String>())
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
        else {
            continue;
        };
        let year = card
            .select(&info_sel)
            .next()
            .map(|info| info.text().collect::<String>())
            .and_then(|text| parse_year(&text));
        let poster_url = card
            .select(&poster_sel)
            .next()
            .and_then(|img| img.value().attr("src"))
            .and_then(|src| resolve_url(base, src));
        let rating = card
            .select(&rating_sel)
            .next()
            .map(|rating| rating.text().collect::<String>())
            .and_then(|text| parse_rating(&text));
        let kind = card
            .select(&kind_sel)
            .next()
            .map(|entity| entity.text().collect::<String>())
            .as_deref()
            .and_then(kind_from_entity)
            .or_else(|| first_path_segment(&page_url).and_then(kind_from_segment))
            .ok_or_else(|| KinoError::Parse(format!("не удалось определить тип карточки {id}")))?;

        items.push(MediaItem {
            id,
            title,
            original_title: None,
            year,
            kind,
            poster_url,
            page_url,
            rating,
        });
    }
    Ok(items)
}

/// Номер последней страницы из пагинации `div.b-navigation`, если она есть.
/// Берётся максимальный `N` среди ссылок вида `.../page/N/`.
pub fn parse_last_page(html: &str) -> Option<u32> {
    let document = Html::parse_document(html);
    let link_sel = Selector::parse("div.b-navigation a").ok()?;
    let mut last = None;
    for link in document.select(&link_sel) {
        if let Some(page) = link.value().attr("href").and_then(page_from_href) {
            last = Some(last.map_or(page, |current: u32| current.max(page)));
        }
    }
    last
}

/// Число страницы из ссылки `.../page/N/`.
fn page_from_href(href: &str) -> Option<u32> {
    const MARKER: &str = "/page/";
    let mut from = 0;
    let mut found = None;
    while let Some(pos) = href.get(from..)?.find(MARKER) {
        let digits_start = from + pos + MARKER.len();
        let digits: String = href
            .get(digits_start..)?
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            found = digits.parse::<u32>().ok();
        }
        from = digits_start + digits.len();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://kvk.pub/").expect("url")
    }

    #[test]
    fn parses_full_card() {
        let html = r#"<html><body>
            <div class="b-content__inline_item" data-id="85038" data-url="/films/fantasy/85038-odisseya.html">
                <div class="b-content__inline_item-link">
                    <a href="/films/fantasy/85038-odisseya.html">Одиссея</a>
                    <div>2026, Великобритания, США, фэнтези</div>
                </div>
                <a class="b-content__inline_item-cover" href="/films/85038.html"><img src="/posters/85038.jpg"></a>
                <i class="b-category-bestrating">(8.7)</i>
                <i class="entity">Фильм</i>
            </div>
        </body></html>"#;
        let items = parse_cards(html, &base()).expect("карточка");
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.id, 85038);
        assert_eq!(item.title, "Одиссея");
        assert_eq!(item.original_title, None);
        assert_eq!(item.year, Some(2026));
        assert_eq!(item.kind, MediaKind::Movie);
        assert_eq!(
            item.poster_url.as_ref().map(Url::as_str),
            Some("https://kvk.pub/posters/85038.jpg")
        );
        assert_eq!(
            item.page_url.as_str(),
            "https://kvk.pub/films/fantasy/85038-odisseya.html"
        );
        assert_eq!(item.rating, Some(8.7));
    }

    #[test]
    fn missing_optional_fields_become_none() {
        let html = r#"<html><body>
            <div class="b-content__inline_item" data-id="1" data-url="/series/1.html">
                <div class="b-content__inline_item-link">
                    <a href="/series/1.html">Сериал</a>
                    <div>2014 - ...</div>
                </div>
                <i class="entity">Сериал</i>
            </div>
        </body></html>"#;
        let items = parse_cards(html, &base()).expect("карточка");
        let item = &items[0];
        assert_eq!(item.poster_url, None);
        assert_eq!(item.rating, None);
        assert_eq!(item.year, Some(2014));
        assert_eq!(item.kind, MediaKind::Series);
    }

    #[test]
    fn kind_falls_back_to_path_segment() {
        let html = r#"<html><body>
            <div class="b-content__inline_item" data-id="2" data-url="/animation/2.html">
                <div class="b-content__inline_item-link"><a href="/animation/2.html">Аниме</a></div>
            </div>
        </body></html>"#;
        let items = parse_cards(html, &base()).expect("карточка");
        assert_eq!(items[0].kind, MediaKind::Anime);
    }

    #[test]
    fn skips_garbage_and_returns_empty_for_no_cards() {
        let junk = r#"<html><body><div class="b-content__inline_item">без id</div></body></html>"#;
        assert_eq!(parse_cards(junk, &base()).expect("пусто"), vec![]);
        let empty = "<html><body><h1>Ничего</h1></body></html>";
        assert_eq!(parse_cards(empty, &base()).expect("пусто"), vec![]);
    }

    #[test]
    fn login_page_is_auth_required() {
        let login = r#"<html><body><form id="loginForm"></form></body></html>"#;
        assert_eq!(parse_cards(login, &base()), Err(KinoError::AuthRequired));
    }

    #[test]
    fn last_page_is_max_from_navigation() {
        let html = r#"<div class="b-navigation">
            <a href="/films/page/1/">1</a>
            <a href="/films/page/2/">2</a>
            <a href="/films/page/5/">5</a>
        </div>"#;
        assert_eq!(parse_last_page(html), Some(5));
        assert_eq!(parse_last_page("<html>без навигации</html>"), None);
    }

    #[test]
    fn catalog_url_paths() {
        let cases = [
            (None, 1, "https://kvk.pub/"),
            (None, 2, "https://kvk.pub/page/2/"),
            (Some(MediaKind::Movie), 1, "https://kvk.pub/films/"),
            (Some(MediaKind::Movie), 3, "https://kvk.pub/films/page/3/"),
            (Some(MediaKind::Series), 1, "https://kvk.pub/series/"),
            (
                Some(MediaKind::Cartoon),
                2,
                "https://kvk.pub/cartoons/page/2/",
            ),
            (
                Some(MediaKind::Anime),
                4,
                "https://kvk.pub/animation/page/4/",
            ),
        ];
        for (kind, page, expected) in cases {
            assert_eq!(catalog_url(&base(), kind, page).unwrap().as_str(), expected);
        }
    }
}
