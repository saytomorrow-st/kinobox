//! Тонкие обёртки над `AppService`. Логики здесь нет, кроме управления окнами и плеером.

use std::sync::{Arc, Mutex};

use kino_ipc::{
    BootState, EpisodeDto, IpcError, MediaDetailsDto, MediaItemDto, MediaKindDto, MirrorDto,
    PlayerProgressDto, StreamQualityDto, TranslatorId, UserDto,
};
use kino_player::{PlayerEvent, WindowRenderTarget, WindowedPlayer};
use tauri::{AppHandle, Emitter, State, Window};

use crate::player_state::{player_error, PlayerState};
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

#[tauri::command]
#[specta::specta]
pub async fn catalog(
    service: State<'_, Service>,
    kind: Option<MediaKindDto>,
    page: u32,
) -> Result<Vec<MediaItemDto>, IpcError> {
    service.catalog(kind, page).await
}

#[tauri::command]
#[specta::specta]
pub async fn search(
    service: State<'_, Service>,
    query: String,
) -> Result<Vec<MediaItemDto>, IpcError> {
    service.search(query).await
}

#[tauri::command]
#[specta::specta]
pub async fn details(
    service: State<'_, Service>,
    item: MediaItemDto,
) -> Result<MediaDetailsDto, IpcError> {
    service.details(item).await
}

#[tauri::command]
#[specta::specta]
pub async fn episodes(
    service: State<'_, Service>,
    details: MediaDetailsDto,
) -> Result<Vec<EpisodeDto>, IpcError> {
    service.episodes(details).await
}

/// Потоки уже после деобфускации. `translator_id = None` — перевод по умолчанию; `episode`
/// задаётся только для сериала. Детали перезапрашиваются внутри сервиса (`raw_html` по IPC
/// не идёт).
#[tauri::command]
#[specta::specta]
pub async fn streams(
    service: State<'_, Service>,
    item: MediaItemDto,
    translator_id: Option<TranslatorId>,
    episode: Option<EpisodeDto>,
) -> Result<Vec<StreamQualityDto>, IpcError> {
    service
        .streams(item, translator_id.map(|id| id.0), episode)
        .await
}

/// Открывает окно плеера, привязывает рендер и загружает качество как новый файл (полная
/// перезагрузка, позиция не сохраняется — это старт просмотра). Повторный вызов с уже открытым
/// плеером только переключает файл, не пересоздавая `WindowedPlayer`.
#[tauri::command]
#[specta::specta]
pub async fn player_load(
    app: AppHandle,
    player: State<'_, PlayerState>,
    quality: StreamQualityDto,
) -> Result<(), IpcError> {
    let quality = quality.try_into_stream_quality()?;
    let shared = player.shared();
    let window = windows::open_player(&app, Arc::clone(&shared))?;

    if shared.lock().unwrap().is_none() {
        let attached = attach_player(&window).await?;
        attached
            .player()
            .observe_playback_properties()
            .map_err(player_error)?;
        *shared.lock().unwrap() = Some(attached);
        start_progress_thread(app.clone(), Arc::clone(&shared));
    }

    let guard = shared.lock().unwrap();
    let windowed = guard
        .as_ref()
        .ok_or_else(|| player_error("плеер не создан"))?;
    windowed
        .player()
        .load_quality(&quality)
        .map_err(player_error)
}

#[tauri::command]
#[specta::specta]
pub fn player_play(player: State<'_, PlayerState>) -> Result<(), IpcError> {
    with_player(&player, |p| p.play().map_err(player_error))
}

#[tauri::command]
#[specta::specta]
pub fn player_pause(player: State<'_, PlayerState>) -> Result<(), IpcError> {
    with_player(&player, |p| p.pause().map_err(player_error))
}

#[tauri::command]
#[specta::specta]
pub fn player_seek(player: State<'_, PlayerState>, seconds: f64) -> Result<(), IpcError> {
    with_player(&player, |p| p.seek_to(seconds).map_err(player_error))
}

/// Переключает качество текущего файла с сохранением позиции (`switch_quality`): вызывается,
/// только когда плеер уже играет то же произведение, поэтому перезагрузка не нужна.
#[tauri::command]
#[specta::specta]
pub fn player_set_quality(
    player: State<'_, PlayerState>,
    quality: StreamQualityDto,
) -> Result<(), IpcError> {
    let quality = quality.try_into_stream_quality()?;
    with_player(&player, |p| {
        p.switch_quality(&quality).map_err(player_error)
    })
}

#[tauri::command]
#[specta::specta]
pub fn player_set_volume(player: State<'_, PlayerState>, volume: f64) -> Result<(), IpcError> {
    with_player(&player, |p| p.set_volume(volume).map_err(player_error))
}

/// Останавливает воспроизведение: закрывает окно плеера, а очистка `WindowedPlayer` происходит
/// в обработчике события окна на главном потоке.
#[tauri::command]
#[specta::specta]
pub fn player_stop(app: AppHandle) -> Result<(), IpcError> {
    windows::close_player(&app);
    Ok(())
}

/// Выполняет `f` на главном потоке и возвращает её результат через `oneshot`. Нужен только для
/// операций рендер-контекста (`attach`, `render`, `view_size`, `set_update_callback`), которые
/// трогают AppKit — методы `Player` (`play`/`pause`/`seek_to`/`volume`/`load_quality`/…)
/// потокобезопасны и вызываются напрямую, без этой обёртки.
async fn on_main<T, F>(window: &Window, f: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, IpcError> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    window
        .run_on_main_thread(move || {
            let _ = tx.send(f());
        })
        .map_err(player_error)?;
    rx.await
        .map_err(|_| player_error("главный поток не ответил"))?
}

/// Создаёт `WindowedPlayer`, привязанный к `NSView` окна `player`, на главном потоке.
async fn attach_player(window: &Window) -> Result<WindowedPlayer, IpcError> {
    let handle = window.clone();
    on_main(window, move || {
        WindowRenderTarget::new(&handle)
            .and_then(WindowedPlayer::attach)
            .map_err(player_error)
    })
    .await
}

fn with_player<R>(
    player: &PlayerState,
    f: impl FnOnce(&kino_player::Player) -> Result<R, IpcError>,
) -> Result<R, IpcError> {
    let guard = player.shared();
    let guard = guard.lock().unwrap();
    let windowed = guard
        .as_ref()
        .ok_or_else(|| player_error("плеер не создан"))?;
    f(windowed.player())
}

/// Фоновый поток прогресса: опрашивает события mpv и эмиттит снимок в `player-controls`
/// событием `player-progress`. Сам завершается, когда `shared` становится `None` (закрытие окна).
///
/// Мьютекс держится на время `next_event` (максимум ~0.1 с): за это время команды
/// `player_play`/`player_pause` могут кратко заблокироваться — осознанная цена за то, что
/// `WindowedPlayer` (самоссылающаяся структура) нельзя поделить на `Arc`-части.
fn start_progress_thread(app: AppHandle, shared: Arc<Mutex<Option<WindowedPlayer>>>) {
    std::thread::spawn(move || {
        let mut progress = PlayerProgressDto {
            position: None,
            duration: None,
            paused: false,
        };
        loop {
            let event = {
                let guard = shared.lock().unwrap();
                let Some(windowed) = guard.as_ref() else {
                    break;
                };
                windowed.player().next_event(0.1)
            };
            let Some(event) = event else {
                continue;
            };
            let changed = match event {
                PlayerEvent::Position(position) => {
                    progress.position = Some(position);
                    true
                }
                PlayerEvent::Duration(duration) => {
                    progress.duration = Some(duration);
                    true
                }
                PlayerEvent::Paused(paused) => {
                    progress.paused = paused;
                    true
                }
                _ => false,
            };
            if changed {
                let _ = app.emit_to(windows::PLAYER_CONTROLS, "player-progress", progress);
            }
        }
    });
}
