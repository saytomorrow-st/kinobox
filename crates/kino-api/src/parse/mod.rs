//! Парсинг HTML-страниц HDrezka: карточки каталога/поиска и страница деталей.
//!
//! Честные ограничения (см. `crates/kino-api/CRATE.md`, раздел «Парсинг и API»):
//! парсер деталей проверен на реальной обезличенной странице фильма
//! `tests/fixtures/film_85038.html`; селекторы каталога и поиска на реальных
//! страницах не проверялись (источники: исходники двух открытых библиотек и
//! просмотр главной kvk.pub 2026-11-14).

mod catalog;
mod details;
mod search;

pub use catalog::{catalog_url, parse_cards, parse_last_page};
pub use details::parse_details;
pub use search::search_url;

use kino_core::{KinoError, MediaKind, Result};
use scraper::Selector;
use url::Url;

/// Маркер формы входа, по которому страница считается заглушкой входа.
const LOGIN_FORM_MARKER: &str = "id=\"loginForm\"";

/// Страница входа (есть форма `id="loginForm"`).
pub(crate) fn is_login_page(html: &str) -> bool {
    html.contains(LOGIN_FORM_MARKER)
}

/// Компилирует CSS-селектор; ошибка синтаксиса — `KinoError::Parse`.
pub(crate) fn selector(sel: &str) -> Result<Selector> {
    Selector::parse(sel)
        .map_err(|err| KinoError::Parse(format!("некорректный CSS-селектор {sel}: {err}")))
}

/// Разрешает относительную ссылку против `base`; невалидная — `None`.
pub(crate) fn resolve_url(base: &Url, raw: &str) -> Option<Url> {
    base.join(raw).ok()
}

/// Первый непустой сегмент пути.
pub(crate) fn first_path_segment(url: &Url) -> Option<&str> {
    url.path_segments()?.find(|segment| !segment.is_empty())
}

/// Тип произведения по первому сегменту пути: `films`/`series`/`cartoons`/`animation`.
pub(crate) fn kind_from_segment(segment: &str) -> Option<MediaKind> {
    match segment {
        "films" => Some(MediaKind::Movie),
        "series" => Some(MediaKind::Series),
        "cartoons" => Some(MediaKind::Cartoon),
        "animation" => Some(MediaKind::Anime),
        _ => None,
    }
}

/// Тип произведения по тексту метки `i.entity`.
pub(crate) fn kind_from_entity(text: &str) -> Option<MediaKind> {
    match text.trim() {
        "Фильм" => Some(MediaKind::Movie),
        "Сериал" => Some(MediaKind::Series),
        "Мультфильм" => Some(MediaKind::Cartoon),
        "Аниме" => Some(MediaKind::Anime),
        _ => None,
    }
}

/// Первое ровно 4-значное число в тексте (год). Границы числа не должны быть
/// цифрами, поэтому `20261` годом не считается.
pub(crate) fn parse_year(text: &str) -> Option<u16> {
    let bytes = text.as_bytes();
    for i in 0..bytes.len().saturating_sub(3) {
        let window = &bytes[i..i + 4];
        if !window.iter().all(u8::is_ascii_digit) {
            continue;
        }
        let left_ok = i == 0 || !bytes[i - 1].is_ascii_digit();
        let right_ok = i + 4 >= bytes.len() || !bytes[i + 4].is_ascii_digit();
        if left_ok && right_ok {
            return std::str::from_utf8(window).ok()?.parse().ok();
        }
    }
    None
}

/// Рейтинг из текста вида `(8.7)`, `8.7` или `8,7`.
pub(crate) fn parse_rating(text: &str) -> Option<f32> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | ','))
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.replace(',', ".").parse().ok()
}
