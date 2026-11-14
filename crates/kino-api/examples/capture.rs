//! Сохраняет обезличенные страницы HDrezka как фикстуры для тестов.
//!
//! После входа (Keychain) делает ровно по одному GET: главная, каталог фильмов,
//! поиск по слову `the`, одна страница фильма и одна страница сериала. Адреса
//! фильма и сериала берутся из распарсенных карточек (первая `Movie` и первая
//! `Series`); если сериала нет, он пропускается с сообщением. Каждая страница
//! обезличивается [`sanitize_page`] перед записью, а затем проверяется, что
//! сырых адресов почты не осталось.
//!
//! ```text
//! cargo run -p kino-api --example capture -- --out ./captured
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kino_api::{
    catalog_url, contains_email, ensure_logged_in, parse_cards, sanitize_page, search_url,
    ClientConfig, HttpClient, KeychainSecretStore, MemoryMirrorStore, MirrorConfig, MirrorPool,
};
use kino_core::{KinoError, MediaItem, MediaKind};
use url::Url;

const USAGE: &str = "usage: capture --out <каталог> [--mirror URL]";

struct Args {
    out: Option<PathBuf>,
    mirror: Option<Url>,
    help: bool,
}

fn parse_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|err| format!("некорректный url {raw}: {err}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(format!("{raw}: нужен адрес http(s)://хост/"));
    }
    Ok(url)
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut out = None;
    let mut mirror = None;
    let mut help = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--help" | "-h" => help = true,
            "--out" => {
                let dir = args.next().ok_or("--out требует каталог")?;
                out = Some(PathBuf::from(dir));
            }
            "--mirror" => {
                let raw = args.next().ok_or("--mirror требует URL")?;
                mirror = Some(parse_url(&raw)?);
            }
            other => return Err(format!("неизвестный аргумент {other}")),
        }
    }
    Ok(Args { out, mirror, help })
}

fn site_root(mut url: Url) -> Url {
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    url
}

/// Скачивает страницу, обезличивает и пишет в `out/name`. Возвращает исходный
/// HTML для разбора карточек. Если после обезличивания осталась почта — файл
/// не пишется и возвращается ошибка.
async fn capture_page(
    client: &HttpClient,
    url: &Url,
    out: &Path,
    name: &str,
) -> Result<String, String> {
    let raw = client.get(url).await.map_err(|err| err.to_string())?.text();
    let cleaned = sanitize_page(&raw);
    if contains_email(&cleaned) {
        return Err(format!(
            "{name}: не удалось обезличить страницу (найден email)"
        ));
    }
    let path = out.join(name);
    std::fs::write(&path, cleaned.as_bytes())
        .map_err(|err| format!("не удалось записать {}: {err}", path.display()))?;
    println!("сохранено: {}", path.display());
    Ok(raw)
}

fn first_kind(cards: &[MediaItem], kind: MediaKind) -> Option<&MediaItem> {
    cards.iter().find(|item| item.kind == kind)
}

async fn pick_mirror(client: &HttpClient, manual: Option<Url>) -> Result<Url, KinoError> {
    if let Some(url) = manual {
        return Ok(url);
    }
    let pool = MirrorPool::new(client, MemoryMirrorStore::new(), MirrorConfig::new()?)?;
    Ok(pool.resolve().await?.url)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if args.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let Some(out) = args.out else {
        eprintln!("нужен --out <каталог>\n{USAGE}");
        return ExitCode::from(2);
    };

    let client = match HttpClient::new(ClientConfig::default()) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let mirror = match pick_mirror(&client, args.mirror).await {
        Ok(mirror) => mirror,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let store = KeychainSecretStore::new();
    match ensure_logged_in(&client, &mirror, &store).await {
        Ok(_) => {}
        Err(KinoError::AuthRequired) => {
            eprintln!("сначала выполните: cargo run -p kino-api --example login");
            return ExitCode::FAILURE;
        }
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    }

    if let Err(err) = std::fs::create_dir_all(&out) {
        eprintln!("не удалось создать каталог {}: {err}", out.display());
        return ExitCode::FAILURE;
    }

    let root = site_root(mirror);
    let films_url = match catalog_url(&root, Some(MediaKind::Movie), 1) {
        Ok(url) => url,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let search_the = match search_url(&root, "the", 1) {
        Ok(url) => url,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let raw_main = match capture_page(&client, &root, &out, "main.html").await {
        Ok(raw) => raw,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let raw_films = match capture_page(&client, &films_url, &out, "catalog_films.html").await {
        Ok(raw) => raw,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(message) = capture_page(&client, &search_the, &out, "search_the.html").await {
        eprintln!("{message}");
        return ExitCode::FAILURE;
    }

    let main_cards = parse_cards(&raw_main, &root).unwrap_or_default();
    let film_cards = parse_cards(&raw_films, &films_url).unwrap_or_default();

    let movie = first_kind(&film_cards, MediaKind::Movie)
        .or_else(|| first_kind(&main_cards, MediaKind::Movie));
    let series = first_kind(&main_cards, MediaKind::Series);

    let Some(movie) = movie else {
        eprintln!("не найдена карточка фильма в каталоге, страницу фильма пропускаем");
        return ExitCode::FAILURE;
    };
    let movie_name = format!("film_{}.html", movie.id);
    if let Err(message) = capture_page(&client, &movie.page_url, &out, &movie_name).await {
        eprintln!("{message}");
        return ExitCode::FAILURE;
    }

    match series {
        Some(series) => {
            let series_name = format!("series_{}.html", series.id);
            if let Err(message) = capture_page(&client, &series.page_url, &out, &series_name).await
            {
                eprintln!("{message}");
                return ExitCode::FAILURE;
            }
        }
        None => println!("сериал не найден в каталоге, страница сериала пропущена"),
    }

    ExitCode::SUCCESS
}
