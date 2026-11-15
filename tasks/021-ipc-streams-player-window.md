---
id: 021
type: feature
status: open
model: claude-sonnet-5
crate: kino-ipc
created: 2026-11-15
---

## Задача

Команды потоков (`streams(details, translator, episode?)` → `Vec<StreamQualityDto>`) в `kino-ipc`,
и в `src-tauri`: окна `player`+`player-controls` (создание/закрытие/синхронизация геометрии),
команды управления плеером (`playerLoad/Play/Pause/Seek/SetQuality/SetVolume/Stop`) как тонкие
обёртки над `kino-player` (из #017/#018), событие прогресса из `kino-player` наружу в `player-controls`
через `tauri::Emitter`.

## Ограничения

- `kino-player` не знает про Tauri (как и `kino-ipc` не знает про Tauri напрямую, см. правило
  зависимостей в `ARCHITECTURE.md`) — интеграция с окнами и событиями живёт в `src-tauri`, не в
  `kino-ipc`/`kino-player`.
- Окно `player` без веб-контента (см. #018); окно `player-controls` — обычный Tauri-webview,
  прозрачный, поверх `player`, с кликабельной зоной только там, где реально есть контролы
  (остальная область пропускает клики на `player`/видео, см. архитектурное решение в
  `ARCHITECTURE.md`).
- Capabilities (`src-tauri/capabilities/default.json`) — добавить новые команды и окна `player`/
  `player-controls` в список, не расширять права сверх необходимого (без `fs`/`shell`/`http`/
  `opener`).
- Закрытие `player`/`player-controls` останавливает воспроизведение и освобождает `kino-player`
  (нет утечки потоков/хендлов mpv между просмотрами).
- Не запускать `cargo tauri dev`/`.app`/`cargo run -p kinobox-app` — визуальная/живая проверка через
  CI-логи и вручную заказчиком.

## Готово когда

- `cargo check --workspace --target aarch64-apple-darwin`/`x86_64-apple-darwin` зелёные
- `cargo test -p kino-ipc` и `cargo clippy --workspace --all-targets -- -D warnings` зелёные
- `cargo test -p kinobox-app export_bindings` детерминирован, `ui/src/bindings.ts` обновлён и
  закоммичен
- `kino-ipc/CRATE.md` и `src-tauri/CRATE.md` описывают новые команды, окна и события

## Заметки

Зависит от #018 (рендер в окне), #020 (DTO каталога/деталей для получения `translator`/`episode`).
Самая сложная интеграционная задача M3 — после неё технически возможно «войти → выбрать фильм →
получить поток и команды управления», но без UI плеера (#024) посмотреть кино пока не на чём.
