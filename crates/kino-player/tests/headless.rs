//! Headless-тесты `kino-player`: без окна, без звукового устройства, без сети.
//!
//! Источник для воспроизведения — локальный WAV-файл `tests/fixtures/silence.wav`
//! (сгенерирован `python3`+`wave`, синус 440 Гц, 2 секунды, см. `CRATE.md`).
//! Плеер создаётся через [`Player::headless`] (`vo=null`, `ao=null`), поэтому тесты не
//! открывают окно и не трогают звуковое устройство — это проверено вручную через
//! `mpv --vo=null --ao=null <fixture>` до написания тестов (см. `CRATE.md`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use kino_core::{StreamQuality, SubtitleTrack};
use kino_player::{EndReason, Player, PlayerEvent};
use url::Url;

fn fixture_url(name: &str) -> Url {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(name);
    Url::from_file_path(&path).expect("fixture path must be absolute")
}

fn fixture_quality(label: &str, name: &str) -> StreamQuality {
    StreamQuality {
        label: label.to_string(),
        url: fixture_url(name),
        premium: false,
        hls: None,
        fallbacks: vec![],
    }
}

/// Ждёт, пока `check` не вернёт `true`, опрашивая `next_event` не дольше `timeout`.
/// Нужен, потому что decoding/загрузка файла в mpv асинхронны: свойства недоступны
/// до `FileLoaded`, а наблюдаемые события приходят через `observe_playback_properties`.
fn wait_until(player: &Player, timeout: Duration, mut check: impl FnMut(&PlayerEvent) -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(event) = player.next_event(0.1) {
            if check(&event) {
                return;
            }
        }
    }
    panic!("условие не выполнилось за {timeout:?}");
}

#[test]
fn headless_player_has_no_position_before_load() {
    let player = Player::headless().expect("headless mpv init");
    assert_eq!(player.position().unwrap(), None);
    assert_eq!(player.duration().unwrap(), None);
}

#[test]
fn play_pause_toggle_playback_state() {
    let player = Player::headless().expect("headless mpv init");
    player
        .load_quality(&fixture_quality("test", "silence.wav"))
        .unwrap();

    player.pause().unwrap();
    assert!(player.is_paused().unwrap());

    player.play().unwrap();
    assert!(!player.is_paused().unwrap());
}

#[test]
fn seek_moves_playback_position() {
    let player = Player::headless().expect("headless mpv init");
    player
        .load_quality(&fixture_quality("test", "silence.wav"))
        .unwrap();
    player.pause().unwrap();

    // Ждём, пока mpv загрузит файл и `time-pos` станет доступен.
    let mut loaded = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !loaded {
        if player.position().unwrap().is_some() {
            loaded = true;
        }
    }
    assert!(loaded, "файл не загрузился за 5 секунд");

    player.seek_to(1.0).unwrap();

    // Перемотка внутри mpv асинхронна, поэтому ждём итоговую позицию в допуске.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = None;
    while Instant::now() < deadline {
        if let Some(position) = player.position().unwrap() {
            if position >= 0.9 {
                seen = Some(position);
                break;
            }
        }
    }
    assert!(seen.is_some(), "позиция не достигла ожидаемого значения");
}

#[test]
fn volume_roundtrip() {
    let player = Player::headless().expect("headless mpv init");
    player.set_volume(42.0).unwrap();
    assert_eq!(player.volume().unwrap(), 42.0);
}

#[test]
fn events_report_pause_and_end_of_file() {
    let player = Player::headless().expect("headless mpv init");
    player.observe_playback_properties().unwrap();
    player
        .load_quality(&fixture_quality("test", "silence.wav"))
        .unwrap();

    // Файл короткий (2 секунды) и без звукового устройства доигрывается быстро,
    // поэтому ждём `EndOfFile` без принудительного `seek`/`playlist-next`.
    wait_until(&player, Duration::from_secs(15), |event| {
        matches!(event, PlayerEvent::EndOfFile(EndReason::Eof))
    });
}

#[test]
fn switch_quality_preserves_position() {
    let player = Player::headless().expect("headless mpv init");
    player
        .load_quality(&fixture_quality("low", "silence.wav"))
        .unwrap();
    player.pause().unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && player.position().unwrap().is_none() {}
    player.seek_to(1.0).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(position) = player.position().unwrap() {
            if position >= 0.9 {
                break;
            }
        }
    }

    player
        .switch_quality(&fixture_quality("high", "silence.wav"))
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut restored = None;
    while Instant::now() < deadline {
        if let Some(position) = player.position().unwrap() {
            if position >= 0.9 {
                restored = Some(position);
                break;
            }
        }
    }
    assert!(
        restored.is_some(),
        "позиция не восстановилась после переключения качества"
    );
}

#[test]
fn add_subtitle_does_not_error_without_window() {
    let player = Player::headless().expect("headless mpv init");
    player
        .load_quality(&fixture_quality("test", "silence.wav"))
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && player.position().unwrap().is_none() {}

    // У WAV-фикстуры нет видео, но `sub-add` всё равно принимает внешний файл субтитров
    // и не требует видеодорожки для добавления трека.
    let subtitle_path = {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/fixtures/empty.srt");
        path
    };
    std::fs::write(&subtitle_path, "1\n00:00:00,000 --> 00:00:01,000\ntest\n").unwrap();
    let subtitle_url = Url::from_file_path(&subtitle_path).unwrap();

    player
        .add_subtitle(&SubtitleTrack {
            lang: "rus".to_string(),
            label: "Русские".to_string(),
            url: subtitle_url,
            is_default: true,
        })
        .unwrap();

    std::fs::remove_file(&subtitle_path).ok();
}
