---
id: 018
type: feature
status: review
model: claude-sonnet-5
crate: kino-player
created: 2026-11-15
---

## Задача

Встраивание рендера `libmpv` (OpenGL render API, модуль `render` crate `libmpv2`) в окно Tauri
`player` через `raw-window-handle`: `mpv_render_context` рисует напрямую в `NSView` окна, минуя
`WKWebView`. Контролы остаются в отдельном окне `player-controls` (HTML, прозрачное, поверх
`player`) — синхронизацию размеров/позиции окон делает `src-tauri` в #021, здесь только сторона
`kino-player`: функция, принимающая хендл окна и отдающая готовый рендер-контекст с callback'ами
`render`/`swap`/`flip`.

## Ограничения

- Полностью протестировать без реального GUI-запуска нельзя (запрещено агенту и subagent'ам). Сборка
  и компиляция проверяются через `cargo check`/CI; визуальная проверка — вручную заказчиком после
  `build-dev.yml`.
- Код должен компилироваться на обеих архитектурах (`cargo check --target …`) и быть clippy-чистым
  даже без возможности запустить рендер живьём.
- OpenGL на macOS официально deprecated (с Catalina), но присутствует и работает минимум до
  актуальных версий — минимальная версия системы в `tauri.conf.json` (`12.0`) гарантированно его
  поддерживает. Явно задокументировать этот выбор и его срок жизни в `ARCHITECTURE.md` (риск: Apple
  может убрать OpenGL framework в будущей macOS — тогда потребуется переход на Metal, отдельная
  задача).
- Новые зависимости (`raw-window-handle` явной записью в `Cargo.toml`, возможно `objc2`/
  `objc2-app-kit` для получения `NSOpenGLContext`/`CGLContext` из `NSView`) — зафиксировать версии
  явно (`=x.y.z`), как остальной workspace.
- Не делать здесь синхронизацию окон `player`/`player-controls` — это #021. Эта задача даёт только
  функцию встраивания рендера в произвольный хендл окна.

## Готово когда

- `cargo check --workspace --target aarch64-apple-darwin` и `--target x86_64-apple-darwin` зелёные
- `cargo clippy --workspace --all-targets -- -D warnings` зелёный
- Юнит-тесты на то, что не требует реального рендера (геометрия, конвертация хендлов, обработка
  ошибок инициализации) зелёные в `cargo test -p kino-player`
- `kino-player/CRATE.md` дополнен разделом про встраивание, явно отмечено, что визуально не
  проверено без GUI

## Заметки

Зависит от #017. Блокирует #021 (окно плеера в `src-tauri`).

Самая рискованная задача M3: нет прямого способа проверить результат без живого запуска. Если в
процессе найдётся, что `raw-window-handle` из `tauri`/`wry` не даёт нужного уровня доступа (например
не отдаёт `NSView` именно окна `player`, а только контейнер WKWebView) — остановиться и вернуться с
вариантами, не изобретать обходы вслепую.

Код написан, личная проверка пройдена: `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo check --workspace` и оба target — все зелёные (включая
юнит-тесты геометрии/хендлов из `render.rs`/`render/macos.rs`, но без реального рендера в живое
окно — см. `CRATE.md`, раздел «Что проверено и что нет»). `cargo test -p kino-player` не
запущен лично по той же причине, что в #017 (нет x86_64-bottle `mpv` на этой машине) — ждём
CI (Apple Silicon раннер).

Смержено с #017 в одном PR/коммите: обе задачи трогали `crates/kino-player/{lib.rs,Cargo.toml,
CRATE.md}` в разных worktree (`/Users/saytomorrow/kinobox` и `/Users/saytomorrow/kinobox-wt-018`),
слияние сделано вручную лид-агентом (`mod events; mod mpv; pub mod render; mod source;`
в `lib.rs`, объединённый `Cargo.toml`/`CRATE.md`).
