//! Оболочка Tauri: окна, регистрация команд `kino-ipc`, привязки TypeScript.

mod commands;
mod state;
mod windows;

use specta_typescript::Typescript;
use tauri::Wry;
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

    tauri::Builder::default()
        .manage(service)
        .invoke_handler(builder.invoke_handler())
        .run(tauri::generate_context!())?;
    Ok(())
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
