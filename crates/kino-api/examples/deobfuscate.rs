use std::process::ExitCode;

use base64::Engine as _;

/// Мусорные токены, зеркальные библиотечным (см. `src/deobfuscate.rs`).
/// Здесь нужны только чтобы собрать демо-строку «в обратную сторону».
const JUNK: &[&str] = &[
    "$$!!@$$@^!@#$$@",
    "@@@@@!##!^^^",
    "####^!!##!@@",
    "^^^!@##!!##",
    "$$#!!@#!@##",
];

fn main() -> ExitCode {
    match std::env::args().nth(1) {
        Some(raw) => print(kino_api::deobfuscate(&raw)),
        None => run_demo(),
    }
}

fn run_demo() -> ExitCode {
    let plain = [
        "[360p]https://a.example.com/s/1/zrkms.mp4:hls:manifest.m3u8 or https://b.example.com/s/1/zrkms.mp4:hls:manifest.m3u8 or https://a.example.com/s/1/zrkms.mp4 or https://b.example.com/s/1/zrkms.mp4",
        "[480p]https://a.example.com/s/2/zrkms.mp4:hls:manifest.m3u8 or https://b.example.com/s/2/zrkms.mp4:hls:manifest.m3u8 or https://a.example.com/s/2/zrkms.mp4 or https://b.example.com/s/2/zrkms.mp4",
        "[<span class=\"pjs-prem-quality\">1080p Ultra<img src=\"https://cdn.example.com/prem-icon.svg\" alt=\"\"></span>]https://a.example.com/s/5/zrkms.mp4:hls:manifest.m3u8 or https://b.example.com/s/5/zrkms.mp4:hls:manifest.m3u8 or https://a.example.com/s/5/zrkms.mp4 or https://b.example.com/s/5/zrkms.mp4",
    ]
    .join(",");

    // Собираем обфусцированную строку из «чистого» списка качеств.
    let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&plain);
    let mut obf = b64;
    obf.insert_str(0, JUNK[0]);
    obf.insert_str(0, "//_//");
    // Середину считаем один раз: второй insert не должен попасть внутрь уже
    // вставленного разделителя `//_//`.
    let mid = obf.len() / 2;
    obf.insert_str(mid, "//_//");
    obf.insert_str(mid, JUNK[1]);

    println!("=== демо: новый формат (без #h) ===");
    let plain_code = print(kino_api::deobfuscate(&plain));

    println!();
    println!("=== демо: обфусцированный (#h) ===");
    println!("#h{obf}");
    println!();
    let obf_code = print(kino_api::deobfuscate(&format!("#h{obf}")));

    if plain_code == ExitCode::SUCCESS && obf_code == ExitCode::SUCCESS {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print(result: kino_core::Result<Vec<kino_core::StreamQuality>>) -> ExitCode {
    match result {
        Ok(qualities) => {
            for q in &qualities {
                let premium = if q.premium { " premium" } else { "" };
                let mp4_host = q.url.host_str().unwrap_or("-");
                let hls_host = q.hls.as_ref().and_then(|u| u.host_str()).unwrap_or("-");
                println!(
                    "{}{}  mp4={}  hls={}  fallbacks={}",
                    q.label,
                    premium,
                    mp4_host,
                    hls_host,
                    q.fallbacks.len()
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
