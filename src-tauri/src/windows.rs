//! Окна создаёт и закрывает Rust. Настройки берутся из `tauri.conf.json`.

use std::sync::{Arc, Mutex};

use kino_ipc::{IpcError, IpcErrorKind};
use kino_player::WindowedPlayer;
use tauri::utils::config::WindowConfig;
use tauri::{AppHandle, Manager, WebviewWindowBuilder, WindowEvent};

pub const LOGIN: &str = "login";
pub const MAIN: &str = "main";
pub const PLAYER: &str = "player";
pub const PLAYER_CONTROLS: &str = "player-controls";

fn internal(err: tauri::Error) -> IpcError {
    IpcError::new(
        IpcErrorKind::Internal,
        format!("Не удалось открыть окно: {err}"),
    )
}

fn find_window_config<'a>(app: &'a AppHandle, label: &str) -> Result<&'a WindowConfig, IpcError> {
    app.config()
        .app
        .windows
        .iter()
        .find(|config| config.label == label)
        .ok_or_else(|| IpcError::new(IpcErrorKind::Internal, "Окно не описано в конфигурации"))
}

/// Показывает окно, а если его нет, создаёт по описанию из конфигурации.
pub fn open(app: &AppHandle, label: &str) -> Result<(), IpcError> {
    let window = match app.get_webview_window(label) {
        Some(window) => window,
        None => {
            let config = find_window_config(app, label)?;
            log::info!("окно {label}: создание");
            WebviewWindowBuilder::from_config(app, config)
                .map_err(internal)?
                .build()
                .map_err(internal)?
        }
    };
    log::debug!("окно {label}: show");
    window.show().map_err(internal)?;
    log::debug!("окно {label}: focus");
    window.set_focus().map_err(internal)
}

pub fn close(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        // Ошибка закрытия не меняет исход: окно уже уходит или приложение завершается.
        log::debug!("окно {label}: close");
        let _ = window.close();
    }
}

/// Главное окно появляется раньше, чем закрывается окно входа: окон не бывает ноль.
pub fn enter_main(app: &AppHandle) -> Result<(), IpcError> {
    log::debug!("переход в main");
    open(app, MAIN)?;
    close(app, LOGIN);
    Ok(())
}

pub fn enter_login(app: &AppHandle) -> Result<(), IpcError> {
    log::debug!("переход в login");
    open(app, LOGIN)?;
    close(app, MAIN);
    Ok(())
}

/// Создаёт (если нужно) и показывает окно `player` (без веб-контента) и прозрачное окно
/// `player-controls` поверх него. Возвращает окно плеера — `commands.rs` привязывает к нему
/// рендер `libmpv`.
///
/// Очистка привязана к событиям окна здесь же: при `CloseRequested`/`Destroyed` контролы
/// закрываются, а `WindowedPlayer` в `shared` сбрасывается в `None` (на главном потоке, где
/// доставляются события окна) — воспроизведение останавливается, утечки mpv/рендера нет.
pub fn open_player(
    app: &AppHandle,
    shared: Arc<Mutex<Option<WindowedPlayer>>>,
) -> Result<tauri::Window, IpcError> {
    let window = match app.get_window(PLAYER) {
        Some(window) => window,
        None => {
            let config = find_window_config(app, PLAYER)?;
            // `player` — окно без веб-контента, поэтому не `WebviewWindowBuilder`, а голый
            // `WindowBuilder` (unstable API, включён в `Cargo.toml`).
            log::info!("окно {PLAYER}: создание (без веб-контента)");
            let window = tauri::window::WindowBuilder::from_config(app, config)
                .map_err(internal)?
                .build()
                .map_err(internal)?;

            let handle = app.clone();
            let shared = Arc::clone(&shared);
            window.on_window_event(move |event| {
                match event {
                    WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => {
                        log::debug!("окно {PLAYER}: CloseRequested/Destroyed — очистка");
                        close_player_controls(&handle);
                        *shared.lock().unwrap() = None;
                    }
                    // #031: `player` — безрамочное окно без веб-контента; `player-controls` —
                    // его дочерний оверлей (`parent: "player"`), и именно с ним пользователь
                    // взаимодействует (кнопки/seek/громкость). Когда фокус уходит на `main` и
                    // возвращается кликом по плееру, оверлей мог остаться «за» `player`/не стать
                    // key-окном повторно — плеер выглядел «мёртвым». На каждый возврат фокуса
                    // явно поднимаем `player-controls` вперёд, чтобы оверлей снова стал
                    // кликабельным. `main` при этом не трогаем: пользователь сам решил вернуться
                    // к плееру, переключаться между окнами по-прежнему можно.
                    WindowEvent::Focused(true) => {
                        if let Some(controls) = handle.get_webview_window(PLAYER_CONTROLS) {
                            log::debug!("окно {PLAYER}: Focused — поднимаю {PLAYER_CONTROLS}");
                            if let Err(err) = controls.set_focus() {
                                log::warn!("окно {PLAYER_CONTROLS}: focus не удался: {err}");
                            }
                        }
                    }
                    _ => {}
                }
            });
            window
        }
    };

    log::debug!("окно {PLAYER}: show");
    window.show().map_err(internal)?;
    // Без явного `set_focus()` окно `player` (голое, без веб-контента) появляется, но не
    // становится key-окном — клики по нему не доходят (#027, баг #4). `open()` и
    // `open_player_controls()` делают то же самое, здесь вызов был пропущен.
    log::debug!("окно {PLAYER}: focus");
    window.set_focus().map_err(internal)?;
    open_player_controls(app)?;
    Ok(window)
}

fn open_player_controls(app: &AppHandle) -> Result<(), IpcError> {
    let controls = match app.get_webview_window(PLAYER_CONTROLS) {
        Some(controls) => controls,
        None => {
            let config = find_window_config(app, PLAYER_CONTROLS)?;
            log::info!("окно {PLAYER_CONTROLS}: создание (parent={PLAYER})");
            WebviewWindowBuilder::from_config(app, config)
                .map_err(internal)?
                .build()
                .map_err(internal)?
        }
    };
    log::debug!("окно {PLAYER_CONTROLS}: show");
    controls.show().map_err(internal)?;
    log::debug!("окно {PLAYER_CONTROLS}: focus");
    controls.set_focus().map_err(internal)
}

pub fn close_player(app: &AppHandle) {
    if let Some(window) = app.get_window(PLAYER) {
        // Закрытие окна плеера тянет за собой очистку состояния через `on_window_event`.
        log::debug!("окно {PLAYER}: close");
        let _ = window.close();
    }
}

fn close_player_controls(app: &AppHandle) {
    if let Some(controls) = app.get_webview_window(PLAYER_CONTROLS) {
        log::debug!("окно {PLAYER_CONTROLS}: close");
        let _ = controls.close();
    }
}
