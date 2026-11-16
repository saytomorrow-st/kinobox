use std::process::ExitCode;

use kino_api::{ClientConfig, HttpClient};
use url::Url;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let Some(raw) = std::env::args().nth(1) else {
        eprintln!("usage: fetch <url>");
        return ExitCode::from(2);
    };
    let url = match Url::parse(&raw) {
        Ok(url) => url,
        Err(err) => {
            eprintln!("некорректный url: {err}");
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
    match client.get(&url).await {
        Ok(response) => {
            let preview: String = response.text().chars().take(200).collect();
            println!("status:   {}", response.status);
            println!("attempts: {}", response.attempts);
            println!("body[..200]: {preview}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err} ({err:?})");
            ExitCode::FAILURE
        }
    }
}
