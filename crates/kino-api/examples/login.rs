use std::io::{BufRead, Write};
use std::process::ExitCode;

use kino_api::{
    ensure_logged_in, login_and_remember, ClientConfig, Credentials, HttpClient,
    KeychainSecretStore, MemoryMirrorStore, MirrorConfig, MirrorPool, MirrorState, Password,
    SecretStore,
};
use kino_core::KinoError;
use url::Url;

const USAGE: &str = "usage: login [--mirror URL] [--email EMAIL] [--logout]\n\
    пароль запрашивается скрыто и в аргументы или переменные окружения не принимается";

struct Args {
    mirror: Option<Url>,
    email: Option<String>,
    logout: bool,
    help: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        mirror: None,
        email: None,
        logout: false,
        help: false,
    };
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--help" | "-h" => parsed.help = true,
            "--logout" => parsed.logout = true,
            "--mirror" => {
                let raw = args.next().ok_or("--mirror требует URL")?;
                let url =
                    Url::parse(&raw).map_err(|err| format!("некорректный url {raw}: {err}"))?;
                if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                    return Err(format!("{raw}: нужен адрес http(s)://хост/"));
                }
                parsed.mirror = Some(url);
            }
            "--email" => {
                let email = args.next().ok_or("--email требует значение")?;
                if email.trim().is_empty() {
                    return Err("--email не может быть пустым".to_string());
                }
                parsed.email = Some(email);
            }
            _ => return Err(format!("неизвестный аргумент {flag}")),
        }
    }
    Ok(parsed)
}

fn describe(err: &KinoError) -> String {
    match err {
        KinoError::Challenge => {
            "сайт требует защитную проверку (Anubis), вход невозможен на этом зеркале".to_string()
        }
        KinoError::AllMirrorsUnavailable => {
            "ни одно зеркало не отвечает или все требуют защитную проверку".to_string()
        }
        other => other.to_string(),
    }
}

fn read_email() -> Result<String, String> {
    print!("email: ");
    std::io::stdout().flush().map_err(|err| err.to_string())?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|err| format!("не удалось прочитать email: {err}"))?;
    let email = line.trim().to_string();
    if email.is_empty() {
        return Err("email не введён".to_string());
    }
    Ok(email)
}

fn read_credentials(email: Option<String>) -> Result<Credentials, String> {
    let email = match email {
        Some(email) => email,
        None => read_email()?,
    };
    let password = rpassword::prompt_password("password: ")
        .map_err(|err| format!("не удалось прочитать пароль: {err}"))?;
    if password.is_empty() {
        return Err("пароль не введён".to_string());
    }
    Ok(Credentials {
        email,
        password: Password::new(password),
    })
}

async fn pick_mirror(client: &HttpClient, manual: Option<Url>) -> Result<Url, KinoError> {
    if let Some(url) = manual {
        println!("mirror: {url}");
        return Ok(url);
    }
    let pool = MirrorPool::new(client, MemoryMirrorStore::new(), MirrorConfig::new()?)?;
    let mirror = pool.resolve().await?;
    let mode = match mirror.state {
        MirrorState::Open => "open",
        _ => "login required",
    };
    println!("mirror: {} ({mode})", mirror.url);
    Ok(mirror.url)
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

    let store = KeychainSecretStore::new();
    if args.logout {
        return match store.clear().await {
            Ok(()) => {
                println!("credentials removed");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{}", describe(&err));
                ExitCode::FAILURE
            }
        };
    }

    let client = match HttpClient::new(ClientConfig::default()) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("{}", describe(&err));
            return ExitCode::FAILURE;
        }
    };
    let mirror = match pick_mirror(&client, args.mirror).await {
        Ok(mirror) => mirror,
        Err(err) => {
            eprintln!("{}", describe(&err));
            return ExitCode::FAILURE;
        }
    };

    let stored = match store.load().await {
        Ok(stored) => stored,
        Err(err) => {
            eprintln!("{}", describe(&err));
            return ExitCode::FAILURE;
        }
    };
    let result = if args.email.is_none() && stored.is_some() {
        ensure_logged_in(&client, &mirror, &store).await
    } else {
        let credentials = match read_credentials(args.email) {
            Ok(credentials) => credentials,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::FAILURE;
            }
        };
        login_and_remember(&client, &mirror, &store, &credentials).await
    };

    match result {
        Ok(session) => {
            println!("logged in as {} (id {})", session.username, session.user_id);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{}", describe(&err));
            ExitCode::FAILURE
        }
    }
}
