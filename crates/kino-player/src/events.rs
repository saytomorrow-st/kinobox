//! Поток событий воспроизведения: позиция, длительность, конец файла, пауза, ошибка.

use libmpv2::events::{Event as MpvEvent, PropertyData};
use libmpv2::mpv_end_file_reason;

use crate::mpv::Player;

/// Причина завершения файла (`libmpv2::EndFileReason`, числовые константы `mpv_end_file_reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// Достигнут конец файла.
    Eof,
    /// Ошибка во время воспроизведения.
    Error,
    /// Остановлено командой `quit` или завершением плеера.
    Quit,
    /// Остановлено внешним действием (например сменой плейлиста).
    Stop,
    /// Файл оказался плейлистом и был заменён его содержимым.
    Redirect,
    /// Код причины не из известного набора (новая версия mpv).
    Unknown(u32),
}

impl EndReason {
    fn from_raw(raw: libmpv2::EndFileReason) -> Self {
        match raw {
            mpv_end_file_reason::Eof => Self::Eof,
            mpv_end_file_reason::Error => Self::Error,
            mpv_end_file_reason::Quit => Self::Quit,
            mpv_end_file_reason::Stop => Self::Stop,
            mpv_end_file_reason::Redirect => Self::Redirect,
            other => Self::Unknown(other),
        }
    }
}

/// Событие воспроизведения, отданное наружу из `kino-player`.
///
/// Это проекция `libmpv2::events::Event`: наружу выходят только события, нужные плееру
/// (позиция, длительность, пауза, конец файла, ошибка). Остальные события mpv (`Seek`,
/// `FileLoaded`, `VideoReconfig` и т.д.) приходят как [`PlayerEvent::Other`], чтобы вызывающий
/// мог решить, обрабатывать ли их, без необходимости менять сигнатуру при добавлении новых.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    /// Текущая позиция воспроизведения, секунды (свойство `time-pos`).
    Position(f64),
    /// Длительность текущего файла, секунды (свойство `duration`).
    Duration(f64),
    /// Пауза включена или выключена (свойство `pause`).
    Paused(bool),
    /// Файл закончился; `None`, если причина не была распознана как ошибка.
    EndOfFile(EndReason),
    /// mpv сообщил об ошибке через `Deprecated`/некорректное событие.
    Error(String),
    /// Событие, не входящее в перечисленные выше (`Seek`, `FileLoaded`, `Shutdown` и т.д.).
    Other,
}

/// Идентификаторы `observe_property`: различают события в `wait_event` без сравнения строк.
pub(crate) const REPLY_TIME_POS: u64 = 1;
pub(crate) const REPLY_DURATION: u64 = 2;
pub(crate) const REPLY_PAUSE: u64 = 3;

impl Player {
    /// Подписывает плеер на изменения позиции, длительности и паузы. Нужно вызвать один раз
    /// после создания, прежде чем `next_event` начнёт сообщать о них.
    pub fn observe_playback_properties(&self) -> libmpv2::Result<()> {
        self.mpv()
            .observe_property("time-pos", libmpv2::Format::Double, REPLY_TIME_POS)?;
        self.mpv()
            .observe_property("duration", libmpv2::Format::Double, REPLY_DURATION)?;
        self.mpv()
            .observe_property("pause", libmpv2::Format::Flag, REPLY_PAUSE)?;
        Ok(())
    }

    /// Ждёт следующее событие максимум `timeout` секунд; `0.0` — опрос без ожидания.
    /// `None` — за это время событий не было.
    pub fn next_event(&self, timeout: f64) -> Option<PlayerEvent> {
        match self.mpv().wait_event(timeout) {
            None => None,
            Some(Ok(event)) => Some(map_event(event)),
            Some(Err(err)) => Some(PlayerEvent::Error(err.to_string())),
        }
    }
}

fn map_event(event: MpvEvent<'_>) -> PlayerEvent {
    match event {
        MpvEvent::EndFile(reason) => PlayerEvent::EndOfFile(EndReason::from_raw(reason)),
        MpvEvent::PropertyChange {
            reply_userdata,
            change,
            ..
        } => map_property_change(reply_userdata, change),
        _ => PlayerEvent::Other,
    }
}

fn map_property_change(reply_userdata: u64, change: PropertyData<'_>) -> PlayerEvent {
    match (reply_userdata, change) {
        (REPLY_TIME_POS, PropertyData::Double(position)) => PlayerEvent::Position(position),
        (REPLY_DURATION, PropertyData::Double(duration)) => PlayerEvent::Duration(duration),
        (REPLY_PAUSE, PropertyData::Flag(paused)) => PlayerEvent::Paused(paused),
        _ => PlayerEvent::Other,
    }
}
