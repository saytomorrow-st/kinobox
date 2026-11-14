//! Окна создаёт и закрывает Rust. Настройки берутся из `tauri.conf.json`.

use kino_ipc::{IpcError, IpcErrorKind};
use tauri::{AppHandle, Manager, WebviewWindowBuilder};

pub const LOGIN: &str = "login";
pub const MAIN: &str = "main";

fn internal(err: tauri::Error) -> IpcError {
    IpcError::new(
        IpcErrorKind::Internal,
        format!("Не удалось открыть окно: {err}"),
    )
}

/// Показывает окно, а если его нет, создаёт по описанию из конфигурации.
pub fn open(app: &AppHandle, label: &str) -> Result<(), IpcError> {
    let window = match app.get_webview_window(label) {
        Some(window) => window,
        None => {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|config| config.label == label)
                .ok_or_else(|| {
                    IpcError::new(IpcErrorKind::Internal, "Окно не описано в конфигурации")
                })?;
            WebviewWindowBuilder::from_config(app, config)
                .map_err(internal)?
                .build()
                .map_err(internal)?
        }
    };
    window.show().map_err(internal)?;
    window.set_focus().map_err(internal)
}

pub fn close(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        // Ошибка закрытия не меняет исход: окно уже уходит или приложение завершается.
        let _ = window.close();
    }
}

/// Главное окно появляется раньше, чем закрывается окно входа: окон не бывает ноль.
pub fn enter_main(app: &AppHandle) -> Result<(), IpcError> {
    open(app, MAIN)?;
    close(app, LOGIN);
    Ok(())
}

pub fn enter_login(app: &AppHandle) -> Result<(), IpcError> {
    open(app, LOGIN)?;
    close(app, MAIN);
    Ok(())
}
