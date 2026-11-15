---
id: 017
type: feature
status: review
model: claude-sonnet-5
crate: kino-player
created: 2026-11-15
---

## Задача

Обёртка `libmpv2` в `kino-player`: загрузка потока (`StreamQuality`/`Url`), play/pause/seek/volume,
переключение качества с сохранением позиции, поток событий (позиция, длительность, конец файла,
ошибка, пауза). Без встраивания в окно (это #018) — здесь только управление воспроизведением,
проверяемое headless (`--vo=null` или аналог), без живого GUI-окна.

## Ограничения

- Зависимость `libmpv2` (crates.io, LGPL-2.1) версией `=7.0.1`, `libmpv2-sys` дефолтными
  pregenerated bindings (без фичи `use-bindgen` — она требует `bindgen` и системные заголовки,
  не нужна).
- `cargo check -p kino-player` не требует установленного `libmpv.dylib` (pregenerated bindings,
  линковка идёт только при финальной сборке/тестах). `cargo test -p kino-player` требует
  `brew install mpv` локально и на CI-runner — это установка зависимости для сборки, не запуск
  приложения, разрешена без вопроса.
- Никакого встраивания в NSView/окно здесь — `Mpv`/`RenderContext` в этой задаче не привязаны к
  какому-либо окну. Тесты используют headless-профиль mpv (`vo=null`, `ao=null` или `pcm`), без
  создания окна и без сетевого воспроизведения реального видео (достаточно короткого локального
  тест-файла или `testsrc://`/`av://lavfi` синтетического источника mpv, если библиотека это
  поддерживает — проверить и задокументировать).
- Не трогать `kino-core`: `StreamQuality`/`SubtitleTrack` уже описаны там, `kino-player` только
  потребляет.
- Публичный API `kino-player` — обновить `crates/kino-player/CRATE.md` (создать по образцу
  `kino-ipc/CRATE.md`).

## Готово когда

- `cargo test -p kino-player` зелёный (локально после `brew install mpv`, и так же в CI)
- `cargo clippy --workspace --all-targets -- -D warnings` зелёный
- `cargo check --workspace --target aarch64-apple-darwin` и `--target x86_64-apple-darwin` зелёные
  (линковка не требуется, т.к. `check` не доходит до финальной компоновки бинаря)
- `kino-player/CRATE.md` описывает публичный API и headless-тестовую стратегию

## Заметки

Зависит от #003 (`kino-core::stream`). Блокирует #018 (встраивание рендера в окно).

CI (`.github/workflows/ci.yml`, job `rust`): добавлен шаг `brew install mpv` перед `Test`
(после `Setup Go`), с `HOMEBREW_NO_BUILD_FROM_SOURCE=1` — CI-раннер `macos-latest` (Apple Silicon)
имеет готовую bottle `mpv`, так что шаг не должен компилировать из исходников.

Код написан, личная проверка пройдена: `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo check --workspace` и оба target (`aarch64-apple-darwin`,
`x86_64-apple-darwin`) — все зелёные. `cargo test -p kino-player` **не проверен лично**: на
локальной машине (Intel Mac, macOS 13.7.8) у `mpv` нет x86_64-bottle в homebrew-core (только
`arm64_*` и `*_linux`) — `brew install mpv` неизбежно собирает `llvm` из исходников (несколько
часов), что запрещено. Подтверждение: `-lmpv` — единственная ошибка при `cargo test -p kino-player
--no-run` (линковка, не компиляция). Тест зелёности подтвердит CI (Apple Silicon раннер, есть
arm64-bottle).

Смержено с #018 в одном PR/коммите (оба трогали `crates/kino-player/{lib.rs,Cargo.toml,CRATE.md}`
в разных worktree) — см. `tasks/018-player-window-embedding.md`.
