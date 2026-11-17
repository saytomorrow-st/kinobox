//! Оболочка Tauri: окна, регистрация команд `kino-ipc`, привязки TypeScript.

mod commands;
mod player_state;
mod state;
mod windows;

use specta_typescript::Typescript;
use tauri::{Manager, Wry};
use tauri_plugin_log::{Target, TargetKind};
use tauri_specta::{collect_commands, Builder, ErrorHandlingMode};

const BINDINGS_HEADER: &str =
    "// @generated tauri-specta (cargo test -p kinobox-app export_bindings). Не править руками.\n/* eslint-disable */\n";

fn bindings_builder() -> Builder<Wry> {
    Builder::<Wry>::new()
        .error_handling(ErrorHandlingMode::Result)
        .disable_serde_phases()
        .commands(collect_commands![
            commands::boot,
            commands::login,
            commands::logout,
            commands::session,
            commands::mirrors,
            commands::check_mirrors,
            commands::add_mirror,
            commands::select_mirror,
            commands::quit,
            commands::catalog,
            commands::search,
            commands::details,
            commands::episodes,
            commands::streams,
            commands::player_load,
            commands::player_play,
            commands::player_pause,
            commands::player_seek,
            commands::player_set_quality,
            commands::player_set_volume,
            commands::player_stop,
            commands::player_qualities,
        ])
}

fn export_bindings_to(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    bindings_builder().export(Typescript::default().header(BINDINGS_HEADER), path)?;
    Ok(())
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let service = state::create_service()?;
    let builder = bindings_builder();

    #[cfg(debug_assertions)]
    if let Err(err) = export_bindings_to(&bindings_path()) {
        eprintln!("Привязки TypeScript не записаны: {err}");
    }

    install_panic_hook();

    // Двойная запись: в stdout (Console.app при живом запуске) и в файл
    // `~/Library/Logs/app.kinobox.desktop/` (LogDir). Общий уровень — Info
    // (и в debug, и в release): сторонние крейты (`selectors`/`html5ever`/
    // `scraper` внутри kino-api, `tao`/`wry` внутри Tauri и т.п.) на Debug
    // заваливают лог тысячами строк вида "Matching complex selector ..." и
    // хоронят в них собственные сообщения приложения (баг, найден
    // 17.11.2026 по реальному `KinoBox.log` заказчика — файл был на 100%
    // состоящим из `selectors::matching`). Debug оставлен только для своих
    // крейтов через `level_for`, по имени пакета (`CARGO_PKG_NAME` с `-`
    // заменённым на `_`, как его видит `log`/`fern`).
    let own_crates_level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .level(log::LevelFilter::Info)
                .level_for("kino_core", own_crates_level)
                .level_for("kino_api", own_crates_level)
                .level_for("kino_store", own_crates_level)
                .level_for("kino_ipc", own_crates_level)
                .level_for("kino_player", own_crates_level)
                .level_for("kinobox_app", own_crates_level)
                .build(),
        )
        .setup(|app| {
            // Логгер подключён плагином раньше этого хука, поэтому сообщение о
            // пути к файлу попадает и в сам файл, и в stdout/Console.app. По этому
            // пути заказчик присылает лог при диагностике.
            let log_dir = app.path().app_log_dir()?;
            let file = log_dir.join("KinoBox.log");
            log::info!("лог-файл: {}", file.display());
            Ok(())
        })
        .manage(service)
        .manage(player_state::PlayerState::default())
        .invoke_handler(builder.invoke_handler())
        .run(tauri::generate_context!())?;
    Ok(())
}

/// Паника в Tauri-команде иначе просто завершает процесс без следа. Хук логирует
/// её через `log::error!`, чтобы сообщение попало и в Console.app, и в файл, до
/// того как процесс упадёт.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `log` может быть ещё не инициализирован (паника до подключения плагина) —
        // тогда сообщение уходит только на stderr и в Console.app.
        log::error!("паника: {info}");
        default_hook(info);
    }));
}

fn bindings_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/bindings.ts")
}

#[cfg(test)]
mod tests {
    /// Пишет `ui/src/bindings.ts` без запуска GUI, Keychain и сети.
    #[test]
    fn export_bindings() {
        super::export_bindings_to(&super::bindings_path()).expect("экспорт привязок");
    }
}
