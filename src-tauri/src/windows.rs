//! Окна создаёт и закрывает Rust. Настройки берутся из `tauri.conf.json`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use kino_ipc::{IpcError, IpcErrorKind};
use kino_player::WindowedPlayer;
use tauri::utils::config::WindowConfig;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindowBuilder, WindowEvent};

/// #035: смещение `player` относительно `main`, снятое в момент открытия плеера.
/// `None` = плеер закрыт → синхронизация выключена.
static PLAYER_SYNC_OFFSET: Mutex<Option<(i32, i32)>> = Mutex::new(None);
/// #035: подписка на `Moved`/`Resized` окна `main` ставится один раз
/// (окно `main` живёт дольше `player`, при повторном открытии плеера
/// обработчик не должен навешиваться второй раз).
static PLAYER_SYNC_ATTACHED: AtomicBool = AtomicBool::new(false);

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
                        // #035: плеер закрыт — синхронизацию позиции выключаем,
                        // обработчик `main` видит `None` и больше ничего не двигает.
                        *PLAYER_SYNC_OFFSET.lock().unwrap() = None;
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
    attach_player_sync(app, &window)?;
    open_player_controls(app)?;
    Ok(window)
}

/// #035: окно `player` визуально следует за `main` без AppKit-связи (`parent`/
/// `addChildWindow` отклонены в #031 — они синхронизируют minimize/close).
///
/// Схема: при открытии плеера запоминаем смещение `player` относительно `main`
/// (в физических пикселях); подписка на `Moved`/`Resized` окна `main` при каждом
/// событии ставит `player` в `main.position + offset`. Размер `player` не
/// меняется — плеер «ездит» вместе с главным окном, оставаясь независимым окном.
///
/// Порог срабатывания: `PLAYER_SYNC_OFFSET == None` (плеер закрыт) или `main`
/// в fullscreen — синхронизация не применяется (безопасный no-op), поэтому
/// fullscreen-переходы не уводят плеер и не ломают геометрию. Ошибка применения
/// логируется `warn!`, не паникует.
fn attach_player_sync(app: &AppHandle, player: &tauri::Window) -> Result<(), IpcError> {
    let main = match app.get_webview_window(MAIN) {
        Some(main) => main,
        None => {
            // Плеер открывается только из `main` — но если окно вдруг недоступно,
            // просто остаёмся без синхронизации, это не ошибка открытия плеера.
            log::debug!("окно {PLAYER}: {MAIN} недоступен — синхронизация позиции выключена");
            return Ok(());
        }
    };

    let main_pos = main.outer_position().map_err(internal)?;
    let player_pos = player.outer_position().map_err(internal)?;
    let offset = (player_pos.x - main_pos.x, player_pos.y - main_pos.y);
    *PLAYER_SYNC_OFFSET.lock().unwrap() = Some(offset);
    log::debug!("окно {PLAYER}: синхронизация позиции, смещение {offset:?}");

    if PLAYER_SYNC_ATTACHED.swap(true, Ordering::SeqCst) {
        // Подписка на `main` уже стоит с прошлого открытия — она сама прочитает
        // новое смещение, повторно навешивать обработчик не нужно.
        return Ok(());
    }

    let handle = app.clone();
    main.on_window_event(move |event| match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            let offset = match *PLAYER_SYNC_OFFSET.lock().unwrap() {
                Some(offset) => offset,
                // Плеер закрыт — обработчик остаётся, но ничего не двигает.
                None => return,
            };
            let main = match handle.get_webview_window(MAIN) {
                Some(main) => main,
                None => return,
            };
            let player = match handle.get_window(PLAYER) {
                Some(player) => player,
                None => return,
            };
            match main.is_fullscreen() {
                Ok(true) => {
                    log::trace!("окно {PLAYER}: {MAIN} в fullscreen — пропускаю синхронизацию");
                    return;
                }
                Err(err) => {
                    log::warn!("окно {PLAYER}: не удалось проверить fullscreen {MAIN}: {err}");
                    return;
                }
                Ok(false) => {}
            }
            if let Ok(main_pos) = main.outer_position() {
                let target = PhysicalPosition::new(main_pos.x + offset.0, main_pos.y + offset.1);
                if let Err(err) = player.set_position(target) {
                    log::warn!("окно {PLAYER}: синхронизация позиции не удалась: {err}");
                }
            }
        }
        _ => {}
    });
    Ok(())
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
