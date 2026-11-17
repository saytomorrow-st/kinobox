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

/// Для логов: URL зеркала без query/fragment, чтобы не засветить лишнее.
fn redact_url_for_log(raw: &str) -> &str {
    raw.split(['?', '#']).next().unwrap_or(raw)
}

/// Окно входа создано скрытым и показывается только здесь, если вход не удался тихо.
#[tauri::command]
#[specta::specta]
pub async fn boot(app: AppHandle, service: State<'_, Service>) -> Result<BootState, IpcError> {
    log::debug!("cmd boot");
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
        log::error!("cmd boot: {err}");
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
    // Пароль в лог не попадает никогда — только email для контекста.
    log::debug!("cmd login: email={}", email.trim());
    let user = match service.login(email, password).await {
        Ok(user) => user,
        Err(err) => {
            log::warn!("cmd login: отказ: {err}");
            return Err(err);
        }
    };
    windows::enter_main(&app)?;
    log::info!("cmd login: ok, user_id={}", user.user_id);
    Ok(user)
}

#[tauri::command]
#[specta::specta]
pub async fn logout(app: AppHandle, service: State<'_, Service>) -> Result<(), IpcError> {
    log::debug!("cmd logout");
    service.logout().await?;
    windows::enter_login(&app)
}

#[tauri::command]
#[specta::specta]
pub fn session(service: State<'_, Service>) -> Option<UserDto> {
    let session = service.session();
    log::debug!(
        "cmd session: {}",
        session
            .as_ref()
            .map(|u| format!("user_id={}", u.user_id))
            .unwrap_or_else(|| "нет сессии".to_string())
    );
    session
}

#[tauri::command]
#[specta::specta]
pub async fn mirrors(service: State<'_, Service>) -> Result<Vec<MirrorDto>, IpcError> {
    log::debug!("cmd mirrors");
    Ok(service.mirrors().await)
}

#[tauri::command]
#[specta::specta]
pub async fn check_mirrors(service: State<'_, Service>) -> Result<Vec<MirrorDto>, IpcError> {
    log::debug!("cmd check_mirrors");
    Ok(service.check_mirrors().await)
}

#[tauri::command]
#[specta::specta]
pub async fn add_mirror(service: State<'_, Service>, url: String) -> Result<MirrorDto, IpcError> {
    // URL зеркала может содержать только scheme/host/port (parse_mirror_url это
    // проверяет), но в лог кладём без query/fragment на всякий случай.
    log::info!("cmd add_mirror: {}", redact_url_for_log(&url));
    service.add_mirror(url).await
}

#[tauri::command]
#[specta::specta]
pub async fn select_mirror(
    service: State<'_, Service>,
    url: String,
) -> Result<MirrorDto, IpcError> {
    log::info!("cmd select_mirror: {}", redact_url_for_log(&url));
    service.select_mirror(url).await
}

#[tauri::command]
#[specta::specta]
pub fn quit(app: AppHandle) {
    log::info!("cmd quit");
    app.exit(0);
}

#[tauri::command]
#[specta::specta]
pub async fn catalog(
    service: State<'_, Service>,
    kind: Option<MediaKindDto>,
    page: u32,
) -> Result<Vec<MediaItemDto>, IpcError> {
    log::debug!("cmd catalog: kind={kind:?}, page={page}");
    let items = service.catalog(kind, page).await?;
    log::debug!("cmd catalog: ok, {} элементов", items.len());
    Ok(items)
}

#[tauri::command]
#[specta::specta]
pub async fn search(
    service: State<'_, Service>,
    query: String,
) -> Result<Vec<MediaItemDto>, IpcError> {
    log::debug!("cmd search: query={query:?}");
    service.search(query).await
}

#[tauri::command]
#[specta::specta]
pub async fn details(
    service: State<'_, Service>,
    item: MediaItemDto,
) -> Result<MediaDetailsDto, IpcError> {
    log::debug!("cmd details: id={}", item.id);
    service.details(item).await
}

#[tauri::command]
#[specta::specta]
pub async fn episodes(
    service: State<'_, Service>,
    details: MediaDetailsDto,
) -> Result<Vec<EpisodeDto>, IpcError> {
    log::debug!("cmd episodes: id={}", details.item.id);
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
    log::debug!(
        "cmd streams: id={}, translator_id={:?}, episode={:?}",
        item.id,
        translator_id.as_ref().map(|t| t.0),
        episode.as_ref().map(|e| (e.season_id, e.episode_id)),
    );
    service
        .streams(item, translator_id.map(|id| id.0), episode)
        .await
}

/// Открывает окно плеера, привязывает рендер и загружает качество как новый файл (полная
/// перезагрузка, позиция не сохраняется — это старт просмотра). Повторный вызов с уже открытым
/// плеером только переключает файл, не пересоздавая `WindowedPlayer`. `qualities` — полный список
/// доступных качеств (из `streams`), запоминается для `player_qualities`: окно `player-controls`
/// не может прочитать стор главного окна (отдельная JS-среда), поэтому список приходит и
/// хранится здесь, а не передаётся напрямую.
#[tauri::command]
#[specta::specta]
pub async fn player_load(
    app: AppHandle,
    player: State<'_, PlayerState>,
    quality: StreamQualityDto,
    qualities: Vec<StreamQualityDto>,
) -> Result<(), IpcError> {
    log::info!("cmd player_load: качество={}", quality.label);
    let quality_target = quality.clone().try_into_stream_quality()?;
    let shared = player.shared();
    let window = windows::open_player(&app, Arc::clone(&shared))?;

    if shared.lock().unwrap().is_none() {
        let attached = attach_player(&window, Arc::clone(&shared)).await?;
        attached
            .player()
            .observe_playback_properties()
            .map_err(player_error)?;
        *shared.lock().unwrap() = Some(attached);
        start_progress_thread(app.clone(), Arc::clone(&shared));
    }

    player.set_qualities(qualities);

    let guard = shared.lock().unwrap();
    let windowed = guard
        .as_ref()
        .ok_or_else(|| player_error("плеер не создан"))?;
    let result = windowed.player().load_quality(&quality_target);
    result.map_err(player_error)
}

/// Список качеств, сохранённый последним `player_load` — читает окно `player-controls`, у
/// которого нет доступа к стору главного окна (отдельная JS-среда).
#[tauri::command]
#[specta::specta]
pub fn player_qualities(player: State<'_, PlayerState>) -> Result<Vec<StreamQualityDto>, IpcError> {
    Ok(player.qualities())
}

#[tauri::command]
#[specta::specta]
pub fn player_play(player: State<'_, PlayerState>) -> Result<(), IpcError> {
    log::debug!("cmd player_play");
    with_player(&player, |p| p.play().map_err(player_error))
}

#[tauri::command]
#[specta::specta]
pub fn player_pause(player: State<'_, PlayerState>) -> Result<(), IpcError> {
    log::debug!("cmd player_pause");
    with_player(&player, |p| p.pause().map_err(player_error))
}

#[tauri::command]
#[specta::specta]
pub fn player_seek(player: State<'_, PlayerState>, seconds: f64) -> Result<(), IpcError> {
    log::debug!("cmd player_seek: {seconds}");
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
    log::info!("cmd player_set_quality: {}", quality.label);
    let quality = quality.try_into_stream_quality()?;
    with_player(&player, |p| {
        p.switch_quality(&quality).map_err(player_error)
    })
}

#[tauri::command]
#[specta::specta]
pub fn player_set_volume(player: State<'_, PlayerState>, volume: f64) -> Result<(), IpcError> {
    log::debug!("cmd player_set_volume: {volume}");
    with_player(&player, |p| p.set_volume(volume).map_err(player_error))
}

/// Останавливает воспроизведение: закрывает окно плеера, а очистка `WindowedPlayer` происходит
/// в обработчике события окна на главном потоке.
#[tauri::command]
#[specta::specta]
pub fn player_stop(app: AppHandle) -> Result<(), IpcError> {
    log::info!("cmd player_stop");
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

/// Создаёт `WindowedPlayer`, привязанный к `NSView` окна `player`, на главном потоке,
/// и подключает redraw-цикл (#027): `set_update_callback` заводит обратный вызов
/// libmpv «кадр готов», который маршалит `render` обратно на главный поток.
async fn attach_player(
    window: &Window,
    shared: Arc<Mutex<Option<WindowedPlayer>>>,
) -> Result<WindowedPlayer, IpcError> {
    let handle = window.clone();
    log::info!("плеер: attach рендера к окну");
    let mut attached = on_main(window, move || {
        WindowRenderTarget::new(&handle)
            .and_then(WindowedPlayer::attach)
            .map_err(player_error)
    })
    .await?;

    // Callback вызывается из потока libmpv (НЕ главного). Он владеет клонами `Window` и
    // `shared`, поэтому `'static`; сам `render`/`view_size` трогают AppKit/GL и уходят на
    // главный поток внутри `schedule_render` через `run_on_main_thread`.
    let cb_window = window.clone();
    let cb_shared = Arc::clone(&shared);
    attached
        .render_context_mut()
        .set_update_callback(move || schedule_render(&cb_window, &cb_shared));

    Ok(attached)
}

/// Планирует отрисовку одного кадра на главном потоке. Вызывается из update-callback'а
/// libmpv (фоновый поток) — сам рендер переносится в `run_on_main_thread`, как `attach` и
/// `view_size` выше: AppKit/GL-вызовы разрешены только с главного потока.
fn schedule_render(window: &Window, shared: &Arc<Mutex<Option<WindowedPlayer>>>) {
    let window = window.clone();
    let shared = Arc::clone(shared);
    if window
        .run_on_main_thread(move || {
            let guard = shared.lock().unwrap();
            let Some(windowed) = guard.as_ref() else {
                return;
            };
            let (w, h) = windowed.render_context().view_size();
            let scale = window.scale_factor().unwrap_or(1.0);
            let (pw, ph) = ((w * scale) as i32, (h * scale) as i32);
            if pw > 0 && ph > 0 {
                if let Err(err) = windowed.render_context().render(pw, ph) {
                    log::error!("плеер: render({pw}x{ph}) не удался: {err}");
                }
            }
        })
        .is_err()
    {
        // Главный поток уже не отвечает (окно закрывается) — кадр пропускаем без шума.
        log::debug!("плеер: пропущен редрав — главный поток недоступен");
    }
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
