//! Разбор курируемых коллекций (`b-collections__newest`) и построение URL страниц коллекции.
//!
//! Селекторы карточек коллекций НЕ проверены на реальной странице. Источник:
//! эталонная Python-библиотека `kristal374/hdrezka-api` (MIT,
//! `HDrezka/movie_collections.py`) — контейнер `b-content__collections_item`
//! внутри блока `b-collections__newest` на главной. Точные имена полей/классов
//! карточки — предположение по исходникам, помечены как «не проверено».

use kino_core::{Collection, KinoError, Result};
use scraper::Html;
use url::Url;

use super::{is_login_page, resolve_url, selector};

const NEWEST_BLOCK: &str = "div.b-collections__newest";
const COLLECTION_ITEM: &str = ".b-content__collections_item";
const COLLECTION_LINK: &str = "a.b-content__collections_item-link";
const COLLECTION_POSTER: &str = ".b-content__collections_item-cover img";
const COLLECTION_COUNT: &str = ".b-content__collections_item-count";

/// Список курируемых коллекций с главной страницы.
///
/// Карточки берутся из блока `div.b-collections__newest`; относительные ссылки
/// (обложка, URL коллекции) разрешаются относительно `base`. Страница входа даёт
/// `AuthRequired`. Карточки без `id`/`url`/заголовка пропускаются; пустая страница
/// (без блока) — `Ok(vec![])`.
pub fn parse_collections(html: &str, base: &Url) -> Result<Vec<Collection>> {
    if is_login_page(html) {
        return Err(KinoError::AuthRequired);
    }
    let document = Html::parse_document(html);
    let block_sel = selector(NEWEST_BLOCK)?;
    let item_sel = selector(COLLECTION_ITEM)?;
    let link_sel = selector(COLLECTION_LINK)?;
    let poster_sel = selector(COLLECTION_POSTER)?;
    let count_sel = selector(COLLECTION_COUNT)?;

    let mut collections = Vec::new();
    let Some(block) = document.select(&block_sel).next() else {
        return Ok(collections);
    };

    for card in block.select(&item_sel) {
        let Some(link) = card.select(&link_sel).next() else {
            continue;
        };
        let Some(raw_url) = link.value().attr("href") else {
            continue;
        };
        let Some(url) = resolve_url(base, raw_url) else {
            continue;
        };
        let Some(id) = collection_id_from_url(&url) else {
            continue;
        };
        let title = link.text().collect::<String>();
        let title = title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let poster_url = card
            .select(&poster_sel)
            .next()
            .and_then(|img| img.value().attr("src"))
            .and_then(|src| resolve_url(base, src));
        let items_count = card
            .select(&count_sel)
            .next()
            .map(|count| count.text().collect::<String>())
            .and_then(|text| parse_items_count(&text));

        collections.push(Collection {
            id,
            title,
            url,
            poster_url,
            items_count,
        });
    }
    Ok(collections)
}

/// Извлекает числовой ID коллекции из её URL (`{base}/collections/<id>-<slug>/`
/// или `{base}/collections/<id>.html`). Берётся первый path-сегмент после
/// `collections`, из которого читается ведущее число до первого нецифрового
/// символа (`-`, `.` и т. п.).
fn collection_id_from_url(url: &Url) -> Option<u64> {
    let mut segments = url.path_segments()?;
    let mut prev: Option<&str> = None;
    for segment in segments.by_ref() {
        if prev == Some("collections") {
            let digits: String = segment.chars().take_while(|c| c.is_ascii_digit()).collect();
            return digits.parse().ok();
        }
        prev = Some(segment);
    }
    None
}

/// Число произведений в коллекции из текста счётчика (первое целое число).
fn parse_items_count(text: &str) -> Option<u32> {
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            return std::str::from_utf8(&bytes[i..j]).ok()?.parse().ok();
        }
    }
    None
}

/// URL страницы коллекции с пагинацией: `{collection.url}/page/N/` при `N > 1`,
/// без суффикса при `page == 1` (как у каталога). Строится от полного URL коллекции
/// (со slug), а не реконструируется из ID.
pub fn collection_url(collection: &Collection, page: u32) -> Result<Url> {
    let mut url = collection.url.clone();
    url.set_query(None);
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/').to_string();
    let mut new_path = path;
    if page > 1 {
        new_path.push_str("/page/");
        new_path.push_str(&page.to_string());
        new_path.push('/');
    } else {
        new_path.push('/');
    }
    url.set_path(&new_path);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://kvk.pub/").expect("url")
    }

    #[test]
    fn parses_collection_card() {
        let html = r#"<html><body>
            <div class="b-collections__newest">
                <div class="b-content__collections_item">
                    <a class="b-content__collections_item-link" href="/collections/5-oskar-2026/">Оскар 2026</a>
                    <a class="b-content__collections_item-cover" href="/collections/5-oskar-2026/"><img src="/collections/5.jpg"></a>
                    <span class="b-content__collections_item-count">24 фильма</span>
                </div>
            </div>
        </body></html>"#;
        let collections = parse_collections(html, &base()).expect("коллекции");
        assert_eq!(collections.len(), 1);
        let collection = &collections[0];
        assert_eq!(collection.id, 5);
        assert_eq!(collection.title, "Оскар 2026");
        assert_eq!(
            collection.url.as_str(),
            "https://kvk.pub/collections/5-oskar-2026/"
        );
        assert_eq!(
            collection.poster_url.as_ref().map(Url::as_str),
            Some("https://kvk.pub/collections/5.jpg")
        );
        assert_eq!(collection.items_count, Some(24));
    }

    #[test]
    fn missing_optional_fields_become_none() {
        let html = r#"<html><body>
            <div class="b-collections__newest">
                <div class="b-content__collections_item">
                    <a class="b-content__collections_item-link" href="/collections/7.html">Без обложки</a>
                </div>
            </div>
        </body></html>"#;
        let collections = parse_collections(html, &base()).expect("коллекции");
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].id, 7);
        assert_eq!(collections[0].poster_url, None);
        assert_eq!(collections[0].items_count, None);
    }

    #[test]
    fn no_block_or_no_cards_gives_empty() {
        let no_block = "<html><body><h1>Главная</h1></body></html>";
        assert_eq!(parse_collections(no_block, &base()).unwrap(), vec![]);
        let no_cards = r#"<html><body><div class="b-collections__newest"></div></body></html>"#;
        assert_eq!(parse_collections(no_cards, &base()).unwrap(), vec![]);
    }

    #[test]
    fn login_page_is_auth_required() {
        let login = r#"<html><body><form id="loginForm"></form></body></html>"#;
        assert_eq!(
            parse_collections(login, &base()),
            Err(KinoError::AuthRequired)
        );
    }

    #[test]
    fn collection_url_adds_page_suffix_only_above_one() {
        let collection = Collection {
            id: 5,
            title: "Оскар 2026".to_string(),
            url: Url::parse("https://kvk.pub/collections/5-oskar-2026/").unwrap(),
            poster_url: None,
            items_count: None,
        };
        assert_eq!(
            collection_url(&collection, 1).unwrap().as_str(),
            "https://kvk.pub/collections/5-oskar-2026/"
        );
        assert_eq!(
            collection_url(&collection, 2).unwrap().as_str(),
            "https://kvk.pub/collections/5-oskar-2026/page/2/"
        );
    }
}
