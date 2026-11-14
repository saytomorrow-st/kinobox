//! Тесты парсинга деталей на реальной обезличенной странице фильма.

use kino_api::parse_details;
use kino_core::{KinoError, MediaKind};
use url::Url;

const FILM: &str = include_str!("fixtures/film_85038.html");

fn page_url() -> Url {
    Url::parse("https://kvk.pub/films/fantasy/85038-odisseya-2026-latest.html").expect("url")
}

#[test]
fn parses_real_film_fixture() {
    let details = parse_details(FILM, &page_url()).expect("детали должны разобраться");

    let item = &details.item;
    assert_eq!(item.id, 85038);
    assert_eq!(item.title, "Одиссея");
    assert_eq!(item.original_title.as_deref(), Some("The Odyssey"));
    assert_eq!(item.year, Some(2026));
    assert_eq!(item.kind, MediaKind::Movie);
    assert!(item.poster_url.is_some());
    assert_eq!(
        item.page_url.as_str(),
        "https://kvk.pub/films/fantasy/85038-odisseya-2026-latest.html"
    );
    assert_eq!(item.rating, Some(7.15));

    assert!(details
        .description
        .as_deref()
        .is_some_and(|d| !d.trim().is_empty()));

    assert_eq!(details.countries, ["Великобритания", "США"]);
    assert_eq!(
        details.genres,
        ["Фэнтези", "Приключения", "Боевики", "Зарубежные"]
    );
    assert_eq!(details.duration_minutes, Some(172));

    let translators: Vec<(u64, &str)> = details
        .translators
        .iter()
        .map(|t| (t.id, t.name.as_str()))
        .collect();
    assert_eq!(
        translators,
        [
            (111, "HDrezka Studio"),
            (672, "Дубляж"),
            (668, "Дубляж (TVOË)"),
            (59, "Многоголосый закадровый"),
            (565, "TVShows"),
            (32, "яскъер (Jaskier)"),
            (269, "Яроцкий"),
            (238, "Оригинал (+субтитры)"),
        ]
    );

    assert!(details.seasons.is_empty());
}

#[test]
fn login_page_is_auth_required() {
    let login = r#"<html><body><form id="loginForm"></form></body></html>"#;
    assert_eq!(
        parse_details(login, &page_url()),
        Err(KinoError::AuthRequired)
    );
}
