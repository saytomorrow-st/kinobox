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
    let plain = "[360p]https://cdn.example.com/360.mp4,[720p]https://cdn.example.com/720.mp4,[1080p]https://cdn.example.com/1080.mp4";
    // Собираем обфусцированную строку из «чистого» списка качеств.
    let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(plain);
    let mut obf = b64;
    obf.insert_str(11, JUNK[0]);
    obf.insert_str(29, "//_//");
    obf.insert_str(47, JUNK[1]);
    obf.insert_str(60, "//_//");
    obf.insert_str(90, JUNK[2]);

    println!("демо-строка:");
    println!("#h{obf}");
    println!();

    print(kino_api::deobfuscate(&format!("#h{obf}")))
}

fn print(result: kino_core::Result<Vec<kino_core::StreamQuality>>) -> ExitCode {
    match result {
        Ok(qualities) => {
            for q in &qualities {
                println!("{}  {}", q.label, q.url);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
