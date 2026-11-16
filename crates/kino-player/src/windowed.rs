//! `WindowedPlayer`: владеющая пара `Player` + `PlayerRenderContext`, живущие вместе в одной
//! структуре — нужна `src-tauri` (#021), которая держит и то, и другое в одном `tauri::State`
//! на всё время, пока открыто окно `player`.
//!
//! `PlayerRenderContext<'a>` заимствует `&'a libmpv2::Mpv` из `Player` (см. `render.rs`). Чтобы
//! не протаскивать параметр времени жизни наружу в `src-tauri` (там это было бы либо
//! самоссылающейся структурой, либо жизнь `Player` должна была бы быть `'static`, что невозможно
//! для значения, созданного в рантайме), этот модуль инкапсулирует единственный во всём крейте
//! `unsafe`-блок, продлевающий заимствование до `'static`, и гарантирует его корректность двумя
//! инвариантами:
//!
//! 1. `player` лежит в `Box` — адрес на куче не меняется, даже когда сам `WindowedPlayer`
//!    перемещают (переносится только указатель `Box`, не данные за ним), поэтому продлённая
//!    ссылка остаётся валидной независимо от того, где лежит `WindowedPlayer`.
//! 2. `render_context` объявлен раньше `player`: Rust дропает поля структур в порядке
//!    объявления (это гарантия языка, не деталь реализации), значит при разрушении
//!    `WindowedPlayer` сначала гарантированно уходит `render_context` (отпуская заимствование),
//!    и только потом — `player`. Наружу нет методов, которые могли бы вынуть/заменить `player`
//!    при живом `render_context` (только `&Player`/`&PlayerRenderContext` геттеры).
//!
//! ## `WindowedPlayer` и `Send`
//!
//! `PlayerRenderContext` на macOS хранит `Retained<NSView>`/`Retained<NSOpenGLContext>`
//! (`render::macos::MacosRenderTarget`) — `objc2` намеренно не реализует для них `Send`/`Sync`,
//! потому что вызывать методы AppKit (`makeCurrentContext`, `flushBuffer`, `frame()` и т.д.)
//! разрешено только с главного потока приложения. Но `tauri::State`/`Manager::manage` требуют
//! `T: Send + Sync + 'static` — то есть `WindowedPlayer` должен быть `Send`, чтобы `src-tauri`
//! мог положить его в `Mutex<Option<WindowedPlayer>>` и управляемое состояние (сам `Mutex<T>`
//! becomes `Send + Sync`, если `T: Send`; `T: Sync` не требуется).
//!
//! Этот крейт реализует для `WindowedPlayer` только `Send`, не `Sync` (см. `unsafe impl` ниже).
//! Сам факт перемещения владения указателем `Retained<NSView>` между потоками безопасен для
//! Cocoa: небезопасно не владение, а вызов AppKit-методов не с главного потока. Rust не может
//! выразить «этот тип можно трогать только с одного конкретного потока в рантайме» через
//! `Send`/`Sync` — `Send` здесь **не** гарантирует потокобезопасность вызовов, а только то, что
//! владение можно передать. Поэтому соблюдение инварианта «методы, трогающие `render_context`
//! (конструктор `attach`, `render`, `view_size`, `set_update_callback`), вызываются только с
//! главного потока» остаётся обязанностью вызывающей стороны (`src-tauri`, #021) — не крейта.
//! Методы `Player` (`play`/`pause`/`seek_to`/`volume`/`load_quality`/...) трогают только
//! `libmpv2::Mpv`, который сам `Send + Sync` (`libmpv2` явно реализует это), и безопасны с
//! любого потока — поэтому `player()`/управление воспроизведением можно вызывать из async-команд
//! `tauri` напрямую, без `run_on_main_thread`.

use crate::mpv::Player;
use crate::render::{PlayerRenderContext, RenderError, WindowRenderTarget};

pub struct WindowedPlayer {
    render_context: PlayerRenderContext<'static>,
    player: Box<Player>,
}

// SAFETY: см. раздел «`WindowedPlayer` и `Send`» в комментарии модуля. Владение AppKit-объектами
// внутри `render_context` можно безопасно передавать между потоками — опасен не факт обладания
// указателем, а вызов AppKit-методов не с главного потока. `WindowedPlayer` не реализует `Sync`
// (нет `unsafe impl Sync`), поэтому делить `&WindowedPlayer` между потоками одновременно всё
// ещё запрещено типами — только `Mutex<WindowedPlayer>`, который сериализует доступ.
unsafe impl Send for WindowedPlayer {}

impl WindowedPlayer {
    /// Создаёт `Player::windowed()` и сразу привязывает его к окну через `target`. При ошибке
    /// привязки (`RenderError`) `Player` тоже уничтожается — вызывающая сторона получает либо
    /// полностью готовую пару, либо ничего.
    pub fn attach(target: WindowRenderTarget) -> Result<Self, RenderError> {
        let player = Box::new(Player::windowed()?);
        // SAFETY: см. обоснование инвариантов в комментарии модуля. `player` только что создан
        // и ещё не возвращён вызывающей стороне — паники/раннего возврата между этой строкой и
        // записью в поля структуры ниже нет, так что либо вся пара строится, либо `player`
        // (ещё обычный локальный `Box`) дропается сам по себе без живого `render_context`.
        let mpv_handle: &'static libmpv2::Mpv =
            unsafe { &*(player.mpv_handle() as *const libmpv2::Mpv) };
        let render_context = PlayerRenderContext::attach(mpv_handle, target)?;
        Ok(Self {
            render_context,
            player,
        })
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn render_context(&self) -> &PlayerRenderContext<'static> {
        &self.render_context
    }

    /// Нужен только для `set_update_callback` (единственный метод `PlayerRenderContext`,
    /// принимающий `&mut self`) — `render`/`view_size` доступны через `render_context()`.
    pub fn render_context_mut(&mut self) -> &mut PlayerRenderContext<'static> {
        &mut self.render_context
    }
}

#[cfg(test)]
mod tests {
    use super::WindowedPlayer;

    /// Компилируется только если `WindowedPlayer: Send` — именно это нужно `tauri::State`
    /// (см. раздел «`WindowedPlayer` и `Send`» в комментарии модуля). Не вызывается, важен сам
    /// факт успешной компиляции.
    #[allow(dead_code)]
    fn assert_send() {
        fn needs_send<T: Send>() {}
        needs_send::<WindowedPlayer>();
    }
}
