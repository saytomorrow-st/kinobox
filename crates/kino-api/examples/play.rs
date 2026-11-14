//! Резолв потоков HDrezka из командной строки.
//!
//! Пример запуска:
//! ```text
//! cargo run -p kino-api --example play -- https://kvk.pub/films/fantasy/85038-odisseya.html
//! cargo run -p kino-api --example play -- <url-сериала> --season 1 --episode 2 --quality 720p
//! ```
//!
//! Для сериала без `--season`/`--episode` печатается список серий и процесс
//! завершается с кодом 2. Ссылки потоков выводятся целиком — это рабочая
//! команда для воспроизведения, а не обезличенный пример.

use std::process::ExitCode;

use kino_api::{
    ensure_logged_in, ClientConfig, HdrezkaApi, HttpClient, KeychainSecretStore, MemoryMirrorStore,
    MirrorConfig, MirrorPool,
};
use kino_core::{KinoError, KinoProvider, StreamQuality};
use url::Url;

const USAGE: &str = "usage: play <url страницы> [--translator ID] [--quality 1080p|720p|…]\n\
                     [--season N --episode M] [--mirror URL]";

struct Args {
    url: Url,
    translator: Option<u64>,
    quality: Option<u32>,
    season: Option<u32>,
    episode: Option<u32>,
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

fn parse_number(flag: &str, raw: &str) -> Result<u32, String> {
    raw.parse::<u32>()
        .map_err(|err| format!("{flag}: некорректное число {raw}: {err}"))
}

fn parse_quality(raw: &str) -> Result<u32, String> {
    let digits: String = raw.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Err(format!("качество {raw} без числа (жду 1080p, 720p, …)"));
    }
    digits
        .parse()
        .map_err(|err| format!("качество {raw}: некорректное число: {err}"))
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let raw_url = args.next().ok_or("нужен url страницы")?;
    if matches!(raw_url.as_str(), "--help" | "-h") {
        return Ok(Args {
            url: Url::parse("https://example.com/").expect("валидный url"),
            translator: None,
            quality: None,
            season: None,
            episode: None,
            mirror: None,
            help: true,
        });
    }
    let url = parse_url(&raw_url)?;

    let mut translator = None;
    let mut quality = None;
    let mut season = None;
    let mut episode = None;
    let mut mirror = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--translator" => {
                let raw = args.next().ok_or("--translator требует значение")?;
                translator = Some(
                    raw.parse::<u64>()
                        .map_err(|err| format!("--translator: {raw}: {err}"))?,
                );
            }
            "--quality" => {
                let raw = args.next().ok_or("--quality требует значение")?;
                quality = Some(parse_quality(&raw)?);
            }
            "--season" => {
                let raw = args.next().ok_or("--season требует значение")?;
                season = Some(parse_number("--season", &raw)?);
            }
            "--episode" => {
                let raw = args.next().ok_or("--episode требует значение")?;
                episode = Some(parse_number("--episode", &raw)?);
            }
            "--mirror" => {
                let raw = args.next().ok_or("--mirror требует URL")?;
                mirror = Some(parse_url(&raw)?);
            }
            other => return Err(format!("неизвестный аргумент {other}")),
        }
    }

    Ok(Args {
        url,
        translator,
        quality,
        season,
        episode,
        mirror,
        help: false,
    })
}

async fn pick_mirror(client: &HttpClient, manual: Option<Url>) -> Result<Url, KinoError> {
    if let Some(url) = manual {
        return Ok(url);
    }
    let pool = MirrorPool::new(client, MemoryMirrorStore::new(), MirrorConfig::new()?)?;
    Ok(pool.resolve().await?.url)
}

/// Число в начале метки качества (`720p` → 720, `1080p Ultra` → 1080, иначе 0).
fn label_number(label: &str) -> u32 {
    label
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

/// Индекс выбранного качества: ближайшее к `requested` по числу в метке, либо
/// самое высокое, если `requested` — `None`.
fn select_quality(qualities: &[StreamQuality], requested: Option<u32>) -> Option<usize> {
    match requested {
        Some(target) => qualities
            .iter()
            .enumerate()
            .min_by_key(|(_, quality)| label_number(&quality.label).abs_diff(target))
            .map(|(index, _)| index),
        None => qualities
            .iter()
            .enumerate()
            .max_by_key(|(_, quality)| label_number(&quality.label))
            .map(|(index, _)| index),
    }
}

fn print_streams(qualities: &[StreamQuality], requested: Option<u32>) -> ExitCode {
    if qualities.is_empty() {
        eprintln!("потоков нет");
        return ExitCode::FAILURE;
    }
    for quality in qualities {
        let premium = if quality.premium { " premium" } else { "" };
        let hls = quality.hls.as_ref().map(|url| url.as_str()).unwrap_or("-");
        println!(
            "{}{}  HLS: {}  MP4: {}  fallbacks: {}",
            quality.label,
            premium,
            hls,
            quality.url,
            quality.fallbacks.len()
        );
    }
    if let Some(index) = select_quality(qualities, requested) {
        let selected = &qualities[index];
        println!();
        println!("выбрано: {}", selected.label);
        println!("  MP4: {}", selected.url);
        if let Some(hls) = &selected.hls {
            println!("  HLS: {hls}");
        }
        for fallback in &selected.fallbacks {
            println!("  резерв: {fallback}");
        }
    }
    ExitCode::SUCCESS
}

fn describe(err: &KinoError) -> String {
    match err {
        KinoError::AuthRequired => {
            "сначала выполните: cargo run -p kino-api --example login".to_string()
        }
        other => other.to_string(),
    }
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

    let client = match HttpClient::new(ClientConfig::default()) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("{}", describe(&err));
            return ExitCode::FAILURE;
        }
    };
    let mirror = match pick_mirror(&client, args.mirror.clone()).await {
        Ok(mirror) => mirror,
        Err(err) => {
            eprintln!("{}", describe(&err));
            return ExitCode::FAILURE;
        }
    };
    let store = KeychainSecretStore::new();
    match ensure_logged_in(&client, &mirror, &store).await {
        Ok(_) => {}
        Err(err) => {
            eprintln!("{}", describe(&err));
            return ExitCode::FAILURE;
        }
    }

    let api = HdrezkaApi::new(client, mirror);
    let mut details = match api.details_at(&args.url).await {
        Ok(details) => details,
        Err(err) => {
            eprintln!("{}", describe(&err));
            return ExitCode::FAILURE;
        }
    };

    // `--translator` выдвигает выбранный перевод на первое место: провайдер
    // берёт перевод по умолчанию именно из начала списка.
    if let Some(id) = args.translator {
        match details
            .translators
            .iter()
            .position(|translator| translator.id == id)
        {
            Some(position) => details.translators.swap(0, position),
            None => {
                eprintln!("перевод {id} не найден");
                return ExitCode::from(2);
            }
        }
    }

    if details.seasons.is_empty() {
        // Фильм: потоки лежат в данных плеера на странице.
        return match api.streams(&details, None).await {
            Ok(qualities) => print_streams(&qualities, args.quality),
            Err(err) => {
                eprintln!("{}", describe(&err));
                ExitCode::FAILURE
            }
        };
    }

    // Сериал: печатаем список серий, поток только по явной паре season/episode.
    let episodes = match api.episodes(&details).await {
        Ok(episodes) => episodes,
        Err(err) => {
            eprintln!("{}", describe(&err));
            return ExitCode::FAILURE;
        }
    };
    for episode in &episodes {
        println!(
            "сезон {} серия {}: {}",
            episode.season_id, episode.episode_id, episode.title
        );
    }

    let (season, episode) = match (args.season, args.episode) {
        (Some(season), Some(episode)) => (season, episode),
        _ => {
            eprintln!("укажите --season N и --episode M, чтобы получить потоки");
            return ExitCode::from(2);
        }
    };
    let selected = match episodes
        .iter()
        .find(|ep| ep.season_id == season && ep.episode_id == episode)
    {
        Some(episode) => episode,
        None => {
            eprintln!("серия {season}.{episode} не найдена");
            return ExitCode::from(2);
        }
    };

    match api.streams(&details, Some(selected)).await {
        Ok(qualities) => print_streams(&qualities, args.quality),
        Err(err) => {
            eprintln!("{}", describe(&err));
            ExitCode::FAILURE
        }
    }
}
