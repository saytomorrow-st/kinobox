use std::process::ExitCode;

use kino_api::{ClientConfig, HttpClient, MemoryMirrorStore, MirrorConfig, MirrorPool};
use url::Url;

const USAGE: &str = "usage: mirrors [--known URL]... [--manual URL]";

struct Args {
    known: Vec<Url>,
    manual: Option<Url>,
}

fn parse_url(raw: &str) -> Result<Url, String> {
    Url::parse(raw).map_err(|err| format!("некорректный url {raw}: {err}"))
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut known = Vec::new();
    let mut manual = None;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} требует URL"))?;
        match flag.as_str() {
            "--known" => known.push(parse_url(&value)?),
            "--manual" => manual = Some(parse_url(&value)?),
            _ => return Err(format!("неизвестный аргумент {flag}")),
        }
    }
    Ok(Args { known, manual })
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

    let client = match HttpClient::new(ClientConfig::default()) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("{err} ({err:?})");
            return ExitCode::FAILURE;
        }
    };
    let config = match MirrorConfig::new() {
        Ok(config) => MirrorConfig {
            known: args.known,
            ..config
        },
        Err(err) => {
            eprintln!("{err} ({err:?})");
            return ExitCode::FAILURE;
        }
    };
    let standby = config.standby.clone();
    let pool = match MirrorPool::new(&client, MemoryMirrorStore::new(), config) {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("{err} ({err:?})");
            return ExitCode::FAILURE;
        }
    };

    let mut statuses = pool.health_check().await;
    statuses.push(pool.probe_standby().await);
    for status in &statuses {
        let verdict = if status.alive { "OK  " } else { "DEAD" };
        let latency = status
            .latency
            .map(|latency| format!("{} ms", latency.as_millis()))
            .unwrap_or_else(|| "-".to_string());
        let reason = status.reason.as_deref().unwrap_or("");
        let label = if status.url == standby {
            " (standby)"
        } else {
            ""
        };
        println!("{verdict}  {}{label}  {latency}  {reason}", status.url);
    }

    let result = match args.manual {
        Some(url) => pool.add_manual(url).await,
        None => pool.resolve().await,
    };
    match result {
        Ok(url) => {
            println!("active: {url}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err} ({err:?})");
            ExitCode::FAILURE
        }
    }
}
