//! Управляемое состояние плеера: `WindowedPlayer` живёт, пока открыто окно `player`.

use std::fmt::Display;
use std::sync::{Arc, Mutex};

use kino_ipc::{IpcError, IpcErrorKind, StreamQualityDto};
use kino_player::WindowedPlayer;

/// Владеющий плеер (`Player` + рендер-контекст), пока открыто окно `player`.
///
/// `WindowedPlayer` реализует только `Send`, не `Sync` (его рендер-контекст держит
/// `NSView`/`NSOpenGLContext`, методы которых разрешено вызывать только с главного потока).
/// `Mutex` сериализует доступ: `Mutex<T>` реализует `Send + Sync`, если `T: Send`, — это ровно
/// то, что требует `tauri::State`/`Manager::manage`.
///
/// `qualities` — список качеств последнего `player_load`, отдельный `Mutex`, не завязан на
/// жизненный цикл `WindowedPlayer`: нужен окну `player-controls` (своя `JS`-среда, без доступа к
/// стору главного окна) для построения переключателя качества.
#[derive(Default)]
pub struct PlayerState(
    Arc<Mutex<Option<WindowedPlayer>>>,
    Arc<Mutex<Vec<StreamQualityDto>>>,
);

impl PlayerState {
    /// Разделяемый указатель на плеер. Нужен `windows.rs` (очистка состояния из обработчика
    /// события окна) и потоку прогресса (чтение событий mpv) — оба не могут взять `State`.
    pub fn shared(&self) -> Arc<Mutex<Option<WindowedPlayer>>> {
        Arc::clone(&self.0)
    }

    /// Запоминает полный список качеств текущего запуска (вызывается из `player_load`).
    pub fn set_qualities(&self, qualities: Vec<StreamQualityDto>) {
        *self.1.lock().unwrap() = qualities;
    }

    /// Список качеств, сохранённый последним `player_load`.
    pub fn qualities(&self) -> Vec<StreamQualityDto> {
        self.1.lock().unwrap().clone()
    }
}

/// Ошибка плеера для UI: всегда `internal`, текст без секретов.
///
/// Единственная точка, через которую проходят все ошибки mpv/плеера, — поэтому логируем
/// здесь: сбой запуска воспроизведения (баг заказчика «не удалось запустить
/// воспроизведение») оставляет в логе строку ошибки mpv.
pub fn player_error(err: impl Display) -> IpcError {
    log::error!("плеер: {err}");
    IpcError::new(IpcErrorKind::Internal, format!("Плеер: {err}"))
}
