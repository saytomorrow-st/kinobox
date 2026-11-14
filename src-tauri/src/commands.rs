//! Тонкие обёртки над `AppService`. Логики здесь нет, кроме управления окнами.

use kino_ipc::{BootState, IpcError, MirrorDto, UserDto};
use tauri::{AppHandle, State};

use crate::state::Service;
use crate::windows;

/// Окно входа создано скрытым и показывается только здесь, если вход не удался тихо.
#[tauri::command]
#[specta::specta]
pub async fn boot(app: AppHandle, service: State<'_, Service>) -> Result<BootState, IpcError> {
    let state = service.boot().await;
    let shown = match state {
        BootState::SignedIn { .. } => windows::enter_main(&app),
        BootState::NeedsLogin { .. } | BootState::Unavailable { .. } => {
            windows::open(&app, windows::LOGIN)
        }
    };
    if let Err(err) = shown {
        // Окно входа скрыто: без него пользователь не увидит даже ошибку.
        let _ = windows::open(&app, windows::LOGIN);
        return Err(err);
    }
    Ok(state)
}

#[tauri::command]
#[specta::specta]
pub async fn login(
    app: AppHandle,
    service: State<'_, Service>,
    email: String,
    password: String,
) -> Result<UserDto, IpcError> {
    let user = service.login(email, password).await?;
    windows::enter_main(&app)?;
    Ok(user)
}

#[tauri::command]
#[specta::specta]
pub async fn logout(app: AppHandle, service: State<'_, Service>) -> Result<(), IpcError> {
    service.logout().await?;
    windows::enter_login(&app)
}

#[tauri::command]
#[specta::specta]
pub fn session(service: State<'_, Service>) -> Option<UserDto> {
    service.session()
}

#[tauri::command]
#[specta::specta]
pub async fn mirrors(service: State<'_, Service>) -> Result<Vec<MirrorDto>, IpcError> {
    Ok(service.mirrors().await)
}

#[tauri::command]
#[specta::specta]
pub async fn check_mirrors(service: State<'_, Service>) -> Result<Vec<MirrorDto>, IpcError> {
    Ok(service.check_mirrors().await)
}

#[tauri::command]
#[specta::specta]
pub async fn add_mirror(service: State<'_, Service>, url: String) -> Result<MirrorDto, IpcError> {
    service.add_mirror(url).await
}

#[tauri::command]
#[specta::specta]
pub async fn select_mirror(
    service: State<'_, Service>,
    url: String,
) -> Result<MirrorDto, IpcError> {
    service.select_mirror(url).await
}

#[tauri::command]
#[specta::specta]
pub fn quit(app: AppHandle) {
    app.exit(0);
}
