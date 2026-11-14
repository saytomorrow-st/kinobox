//! Формирование URL поиска. Сам разбор результатов переиспользует
//! [`crate::parse::parse_cards`], отдельный парсер поиска не нужен.
//!
//! Формат запроса взят из `ARCHITECTURE.md`; на живой странице не проверялся.

use kino_core::Result;
use url::Url;

/// URL поиска: `{зеркало}/search/?do=search&subaction=search&q=<запрос>[&page=N]`.
/// `q` percent-кодируется; `&page=` добавляется только при `page > 1`.
pub fn search_url(base: &Url, query: &str, page: u32) -> Result<Url> {
    let mut url = base.clone();
    url.set_path("/search/");
    url.set_query(None);
    url.set_fragment(None);
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("do", "search");
        pairs.append_pair("subaction", "search");
        pairs.append_pair("q", query);
        if page > 1 {
            pairs.append_pair("page", &page.to_string());
        }
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://kvk.pub/").expect("url")
    }

    #[test]
    fn encodes_cyrillic_and_spaces() {
        let url = search_url(&base(), "оно 2", 1).expect("url");
        assert_eq!(
            url.as_str(),
            "https://kvk.pub/search/?do=search&subaction=search&q=%D0%BE%D0%BD%D0%BE+2"
        );
    }

    #[test]
    fn page_one_has_no_page_param() {
        let url = search_url(&base(), "the", 1).expect("url");
        assert!(!url.as_str().contains("page="), "{}", url);
        assert_eq!(
            url.as_str(),
            "https://kvk.pub/search/?do=search&subaction=search&q=the"
        );
    }

    #[test]
    fn page_two_appends_page_param() {
        let url = search_url(&base(), "the", 2).expect("url");
        assert_eq!(
            url.as_str(),
            "https://kvk.pub/search/?do=search&subaction=search&q=the&page=2"
        );
    }
}
