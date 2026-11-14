//! Поиск, каталог и детали HDrezka из командной строки.
//!
//! Пример запуска:
//! ```text
//! cargo run -p kino-api --example cli -- search "оно 2"
//! cargo run -p kino-api --example cli -- details https://kvk.pub/films/fantasy/85038-odisseya.html
//! cargo run -p kino-api --example cli -- catalog --kind movie --page 2
//! ```

use std::process::ExitCode;

use kino_api::{
    ensure_logged_in, ClientConfig, HdrezkaApi, HttpClient, KeychainSecretStore, MemoryMirrorStore,
    MirrorConfig, MirrorPool,
};
use kino_core::{CatalogQuery, KinoError, MediaItem, MediaKind};
use url::Url;

const USAGE: &str = "usage:\n\
    cli search \"<запрос>\" [--mirror URL]\n\
    cli details <url страницы> [--mirror URL]\n\
    cli catalog [--kind movie|series|cartoon|anime] [--page N] [--mirror URL]";

enum Command {
    Search {
        query: String,
        mirror: Option<Url>,
    },
    Details {
        url: Url,
        mirror: Option<Url>,
    },
    Catalog {
        kind: Option<MediaKind>,
        page: u32,
        mirror: Option<Url>,
    },
    Help,
}

impl Command {
    fn mirror(&self) -> Option<&Url> {
        match self {
            Command::Search { mirror, .. }
            | Command::Details { mirror, .. }
            | Command::Catalog { mirror, .. } => mirror.as_ref(),
            Command::Help => None,
        }
    }
}

fn parse_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|err| format!("некорректный url {raw}: {err}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(format!("{raw}: нужен адрес http(s)://хост/"));
    }
    Ok(url)
}

fn parse_kind(raw: &str) -> Result<MediaKind, String> {
    match raw {
        "movie" => Ok(MediaKind::Movie),
        "series" => Ok(MediaKind::Series),
        "cartoon" => Ok(MediaKind::Cartoon),
        "anime" => Ok(MediaKind::Anime),
        other => Err(format!(
            "неизвестный тип {other}, жду movie|series|cartoon|anime"
        )),
    }
}

fn parse_mirror_flag(args: &mut impl Iterator<Item = String>) -> Result<Option<Url>, String> {
    let mut mirror = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--mirror" => {
                let raw = args.next().ok_or("--mirror требует URL")?;
                mirror = Some(parse_url(&raw)?);
            }
            other => return Err(format!("неизвестный аргумент {other}")),
        }
    }
    Ok(mirror)
}

fn parse_args() -> Result<Command, String> {
    let mut args = std::env::args().skip(1);
    let Some(sub) = args.next() else {
        return Ok(Command::Help);
    };
    match sub.as_str() {
        "--help" | "-h" => Ok(Command::Help),
        "search" => {
            let query = args.next().ok_or("search требует запрос")?;
            let mirror = parse_mirror_flag(&mut args)?;
            Ok(Command::Search { query, mirror })
        }
        "details" => {
            let raw = args.next().ok_or("details требует URL страницы")?;
            let url = parse_url(&raw)?;
            let mirror = parse_mirror_flag(&mut args)?;
            Ok(Command::Details { url, mirror })
        }
        "catalog" => {
            let mut kind = None;
            let mut page = 1;
            let mut mirror = None;
            while let Some(flag) = args.next() {
                match flag.as_str() {
                    "--kind" => {
                        let raw = args.next().ok_or("--kind требует значение")?;
                        kind = Some(parse_kind(&raw)?);
                    }
                    "--page" => {
                        let raw = args.next().ok_or("--page требует число")?;
                        page = raw
                            .parse::<u32>()
                            .map_err(|err| format!("некорректная страница {raw}: {err}"))?;
                        if page == 0 {
                            return Err("страница нумеруется с 1".to_string());
                        }
                    }
                    "--mirror" => {
                        let raw = args.next().ok_or("--mirror требует URL")?;
                        mirror = Some(parse_url(&raw)?);
                    }
                    other => return Err(format!("неизвестный аргумент {other}")),
                }
            }
            Ok(Command::Catalog { kind, page, mirror })
        }
        other => Err(format!("неизвестная команда {other}")),
    }
}

fn kind_label(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "фильм",
        MediaKind::Series => "сериал",
        MediaKind::Cartoon => "мультфильм",
        MediaKind::Anime => "аниме",
    }
}

fn print_items(items: &[MediaItem]) {
    for item in items {
        let year = item
            .year
            .map(|year| year.to_string())
            .unwrap_or_else(|| "-".to_string());
        let rating = item
            .rating
            .map(|rating| rating.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{}  {}  {}  {}  ({rating})",
            item.id,
            year,
            kind_label(item.kind),
            item.title
        );
    }
}

fn print_details(details: &kino_core::MediaDetails) {
    println!("название: {}", details.item.title);
    if let Some(original) = &details.item.original_title {
        println!("оригинал: {original}");
    }
    let year = details
        .item
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| "-".to_string());
    println!("год: {year}");
    let countries = if details.countries.is_empty() {
        "-".to_string()
    } else {
        details.countries.join(", ")
    };
    let genres = if details.genres.is_empty() {
        "-".to_string()
    } else {
        details.genres.join(", ")
    };
    println!("страны: {countries}");
    println!("жанры: {genres}");
    let duration = details
        .duration_minutes
        .map(|minutes| format!("{minutes} мин."))
        .unwrap_or_else(|| "-".to_string());
    println!("длительность: {duration}");
    for translator in &details.translators {
        println!("перевод: {} (id {})", translator.name, translator.id);
    }
    let episodes: usize = details
        .seasons
        .iter()
        .map(|season| season.episodes.len())
        .sum();
    println!("сезонов: {}, серий: {episodes}", details.seasons.len());
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
    let command = match parse_args() {
        Ok(Command::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let client = match HttpClient::new(ClientConfig::default()) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let mirror = match pick_mirror(&client, command.mirror().cloned()).await {
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

    let api = HdrezkaApi::new(client, mirror);
    match command {
        Command::Search { query, .. } => match api.search(&query, 1).await {
            Ok(items) => {
                print_items(&items);
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{err}");
                ExitCode::FAILURE
            }
        },
        Command::Details { url, .. } => match api.details_at(&url).await {
            Ok(details) => {
                print_details(&details);
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{err}");
                ExitCode::FAILURE
            }
        },
        Command::Catalog { kind, page, .. } => {
            match api.catalog(CatalogQuery { kind, page }).await {
                Ok(items) => {
                    print_items(&items);
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Help => ExitCode::SUCCESS,
    }
}
