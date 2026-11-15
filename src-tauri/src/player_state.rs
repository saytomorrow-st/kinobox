//! Управляемое состояние плеера: `WindowedPlayer` живёт, пока открыто окно `player`.

use std::fmt::Display;
use std::sync::{Arc, Mutex};

use kino_ipc::{IpcError, IpcErrorKind};
use kino_player::WindowedPlayer;

/// Владеющий плеер (`Player` + рендер-контекст), пока открыто окно `player`.
///
/// `WindowedPlayer` реализует только `Send`, не `Sync` (его рендер-контекст держит
/// `NSView`/`NSOpenGLContext`, методы которых разрешено вызывать только с главного потока).
/// `Mutex` сериализует доступ: `Mutex<T>` реализует `Send + Sync`, если `T: Send`, — это ровно
/// то, что требует `tauri::State`/`Manager::manage`.
#[derive(Default)]
pub struct PlayerState(Arc<Mutex<Option<WindowedPlayer>>>);

impl PlayerState {
    /// Разделяемый указатель на плеер. Нужен `windows.rs` (очистка состояния из обработчика
    /// события окна) и потоку прогресса (чтение событий mpv) — оба не могут взять `State`.
    pub fn shared(&self) -> Arc<Mutex<Option<WindowedPlayer>>> {
        Arc::clone(&self.0)
    }
}

/// Ошибка плеера для UI: всегда `internal`, текст без секретов.
pub fn player_error(err: impl Display) -> IpcError {
    IpcError::new(IpcErrorKind::Internal, format!("Плеер: {err}"))
}
