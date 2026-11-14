//! Разбор страницы деталей произведения.
//!
//! Проверено на реальной обезличенной странице фильма
//! `tests/fixtures/film_85038.html` (kvk.pub, 2026-11-14). Сезоны/серии для
//! сериалов на реальной странице не проверялись — только синтетический
//! фрагмент в тестах.

use kino_core::{KinoError, MediaDetails, MediaItem, MediaKind, Result, Season, Translator};
use scraper::{ElementRef, Html};
use url::Url;

use super::{
    first_path_segment, is_login_page, kind_from_segment, parse_rating, parse_year, resolve_url,
    selector,
};

const POST_ID: &str = "input#post_id";
const TITLE: &str = ".b-post__title h1";
const ORIG_TITLE: &str = ".b-post__origtitle";
const DESCRIPTION: &str = ".b-post__description_text";
const POSTER: &str = "img[itemprop=\"image\"]";
const RATING_NUM: &str = ".b-post__rating span.num";
const INFO_ROW: &str = ".b-post__info tr";
const TRANSLATORS: &str = "#translators-list li[data-translator_id]";
const SEASON: &str = "li.b-simple_season__item[data-tab_id]";
const EPISODE: &str = "li.b-simple_episode__item[data-season_id][data-episode_id]";

/// Разбирает страницу деталей. Относительные ссылки разрешаются относительно
/// `page_url`. Отсутствие необязательных полей таблицы даёт `None`/пусто, а не
/// ошибку; `Parse` возвращается только если нет `post_id` или названия.
/// Страница входа даёт `AuthRequired`.
pub fn parse_details(html: &str, page_url: &Url) -> Result<MediaDetails> {
    if is_login_page(html) {
        return Err(KinoError::AuthRequired);
    }
    let document = Html::parse_document(html);

    let id = document
        .select(&selector(POST_ID)?)
        .next()
        .and_then(|input| input.value().attr("value"))
        .and_then(|value| value.trim().parse::<u64>().ok())
        .ok_or_else(|| KinoError::Parse("на странице нет post_id".to_string()))?;

    let title = first_text(&document, TITLE)?
        .ok_or_else(|| KinoError::Parse("на странице нет названия".to_string()))?;

    let original_title = first_text(&document, ORIG_TITLE)?;
    let canonical = og_url(&document, page_url);
    let kind = kind_from_page(&document, &canonical)?;
    let poster_url = document
        .select(&selector(POSTER)?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .and_then(|src| resolve_url(&canonical, src));
    let rating = document
        .select(&selector(RATING_NUM)?)
        .next()
        .map(|span| span.text().collect::<String>())
        .and_then(|text| parse_rating(&text));
    let description = first_text(&document, DESCRIPTION)?;

    let rows = info_rows(&document)?;
    let year = info_field(&rows, &["Год", "Дата выхода"]).and_then(|text| parse_year(&text));
    let genres = list_field(&rows, "Жанр")?;
    let countries = list_field(&rows, "Страна")?;
    let duration_minutes = info_field(&rows, &["Время"])
        .as_deref()
        .and_then(parse_duration);
    let translators = parse_translators(&document, html, &rows)?;
    let seasons = parse_seasons(&document)?;

    let item = MediaItem {
        id,
        title,
        original_title,
        year,
        kind,
        poster_url,
        page_url: canonical,
        rating,
    };

    Ok(MediaDetails {
        item,
        description,
        genres,
        countries,
        duration_minutes,
        translators,
        seasons,
        raw_html: Some(html.to_string()),
    })
}

/// Текст первого подходящего элемента, без пробелов по краям; пустой текст — `None`.
fn first_text(document: &Html, sel: &str) -> Result<Option<String>> {
    Ok(document
        .select(&selector(sel)?)
        .next()
        .map(|el| el.text().collect::<String>())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty()))
}

/// Канонический адрес страницы из `meta[property="og:url"]`, иначе переданный `page_url`.
fn og_url(document: &Html, page_url: &Url) -> Url {
    let Ok(sel) = selector("meta[property=\"og:url\"]") else {
        return page_url.clone();
    };
    document
        .select(&sel)
        .next()
        .and_then(|meta| meta.value().attr("content"))
        .and_then(|content| resolve_url(page_url, content))
        .unwrap_or_else(|| page_url.clone())
}

fn kind_from_page(document: &Html, canonical: &Url) -> Result<MediaKind> {
    if let Some(segment) = first_path_segment(canonical) {
        if let Some(kind) = kind_from_segment(segment) {
            return Ok(kind);
        }
    }
    if let Some(og_type) = document
        .select(&selector("meta[property=\"og:type\"]")?)
        .next()
        .and_then(|meta| meta.value().attr("content"))
    {
        match og_type {
            "video.movie" => return Ok(MediaKind::Movie),
            "video.series" => return Ok(MediaKind::Series),
            _ => {}
        }
    }
    Err(KinoError::Parse(
        "не удалось определить тип произведения".to_string(),
    ))
}

/// Строки таблицы `.b-post__info`: (заголовок `h2`, ячейка значения).
fn info_rows<'a>(document: &'a Html) -> Result<Vec<(String, ElementRef<'a>)>> {
    let row_sel = selector(INFO_ROW)?;
    let h2_sel = selector("td h2")?;
    let td_sel = selector("td")?;
    let mut rows = Vec::new();
    for row in document.select(&row_sel) {
        let label = row
            .select(&h2_sel)
            .next()
            .map(|h2| h2.text().collect::<String>())
            .map(|text| text.trim().to_string())
            .unwrap_or_default();
        let cells: Vec<ElementRef<'a>> = row.select(&td_sel).collect();
        let Some(value) = cells.into_iter().nth(1) else {
            continue;
        };
        rows.push((label, value));
    }
    Ok(rows)
}

/// Текст поля таблицы по одному из заголовков.
fn info_field(rows: &[(String, ElementRef<'_>)], names: &[&str]) -> Option<String> {
    rows.iter()
        .find(|(label, _)| names.iter().any(|name| label == name))
        .map(|(_, value)| value.text().collect::<String>())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// Список значений поля таблицы (текст ссылок внутри ячейки).
fn list_field(rows: &[(String, ElementRef<'_>)], name: &str) -> Result<Vec<String>> {
    let Some((_, value)) = rows.iter().find(|(label, _)| label == name) else {
        return Ok(Vec::new());
    };
    let link_sel = selector("a")?;
    Ok(value
        .select(&link_sel)
        .map(|link| link.text().collect::<String>())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect())
}

/// Длительность в минутах из текста вида `172 мин.`.
fn parse_duration(text: &str) -> Option<u32> {
    let digits: String = text
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn parse_translators(
    document: &Html,
    html: &str,
    rows: &[(String, ElementRef<'_>)],
) -> Result<Vec<Translator>> {
    let list_sel = selector(TRANSLATORS)?;
    let translators: Vec<Translator> = document
        .select(&list_sel)
        .filter_map(|li| {
            let id = li
                .value()
                .attr("data-translator_id")?
                .trim()
                .parse::<u64>()
                .ok()?;
            let name = li.text().collect::<String>();
            let name = name.trim().to_string();
            if name.is_empty() {
                return None;
            }
            Some(Translator { id, name })
        })
        .collect();
    if !translators.is_empty() {
        return Ok(translators);
    }
    // Единственный перевод без списка: id из initCDN(Series|Movies)Events.
    let Some(id) = single_translator_id(html) else {
        return Ok(Vec::new());
    };
    let name = info_field(rows, &["В переводе"]).unwrap_or_default();
    Ok(vec![Translator { id, name }])
}

/// Второй числовой аргумент `initCDNMoviesEvents(id, translator_id, ...)` или
/// `initCDNSeriesEvents(...)`, если списка переводов на странице нет.
fn single_translator_id(html: &str) -> Option<u64> {
    ["initCDNMoviesEvents(", "initCDNSeriesEvents("]
        .into_iter()
        .find_map(|marker| translator_id_after(html, marker))
}

fn translator_id_after(html: &str, marker: &str) -> Option<u64> {
    let pos = html.find(marker)?;
    let mut numbers = html[pos + marker.len()..]
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok());
    numbers.next()?;
    numbers.next()
}

fn parse_seasons(document: &Html) -> Result<Vec<Season>> {
    let season_sel = selector(SEASON)?;
    let episode_sel = selector(EPISODE)?;
    let mut seasons: Vec<Season> = document
        .select(&season_sel)
        .filter_map(|li| {
            let number = li.value().attr("data-tab_id")?.trim().parse::<u16>().ok()?;
            Some(Season {
                number,
                episodes: Vec::new(),
            })
        })
        .collect();
    for episode in document.select(&episode_sel) {
        let Some(season_id) = episode
            .value()
            .attr("data-season_id")
            .and_then(|v| v.trim().parse::<u16>().ok())
        else {
            continue;
        };
        let Some(episode_id) = episode
            .value()
            .attr("data-episode_id")
            .and_then(|v| v.trim().parse::<u16>().ok())
        else {
            continue;
        };
        if let Some(season) = seasons.iter_mut().find(|season| season.number == season_id) {
            season.episodes.push(episode_id);
        }
    }
    for season in &mut seasons {
        season.episodes.sort_unstable();
        season.episodes.dedup();
    }
    Ok(seasons)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url() -> Url {
        Url::parse("https://kvk.pub/films/fantasy/85038-odisseya-2026-latest.html").expect("url")
    }

    #[test]
    fn seasons_and_episodes_from_synthetic_fragment() {
        let html = r#"<html><body>
            <input id="post_id" value="1">
            <div class="b-post__title"><h1>Сериал</h1></div>
            <ul>
              <li class="b-simple_season__item" data-tab_id="1">Сезон 1</li>
              <li class="b-simple_season__item" data-tab_id="2">Сезон 2</li>
            </ul>
            <ul>
              <li class="b-simple_episode__item" data-season_id="1" data-episode_id="1"></li>
              <li class="b-simple_episode__item" data-season_id="1" data-episode_id="2"></li>
              <li class="b-simple_episode__item" data-season_id="2" data-episode_id="1"></li>
            </ul>
        </body></html>"#;
        let details = parse_details(html, &url()).expect("детали");
        assert_eq!(details.item.kind, MediaKind::Movie);
        assert_eq!(
            details.seasons,
            vec![
                Season {
                    number: 1,
                    episodes: vec![1, 2]
                },
                Season {
                    number: 2,
                    episodes: vec![1]
                }
            ]
        );
    }

    #[test]
    fn missing_post_id_is_parse_error() {
        let html = r#"<html><body><div class="b-post__title"><h1>Без id</h1></div></body></html>"#;
        assert!(matches!(
            parse_details(html, &url()),
            Err(KinoError::Parse(_))
        ));
    }

    #[test]
    fn login_page_is_auth_required() {
        let login = r#"<html><body><form id="loginForm"></form></body></html>"#;
        assert_eq!(parse_details(login, &url()), Err(KinoError::AuthRequired));
    }

    #[test]
    fn single_translator_fallback_reads_init_events() {
        let html = r#"<html><body><input id="post_id" value="3"><div class="b-post__title"><h1>Фильм</h1></div>
            <script>sof.tv.initCDNMoviesEvents(3, 45, 0, 0, 0, 'kvk.pub', true, false, {});</script>
        </body></html>"#;
        let details = parse_details(html, &url()).expect("детали");
        assert_eq!(
            details.translators,
            vec![Translator {
                id: 45,
                name: String::new()
            }]
        );
    }
}
