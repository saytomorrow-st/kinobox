---
id: 015
type: feature
status: review
model: claude-sonnet-5-5
crate: src-tauri
created: 2026-11-14
---

## Задача

Оболочка Tauri v2 (`src-tauri/`, член workspace) и каркас UI (`ui/`: Vite, React 18, TypeScript, Zustand, Tailwind). Привязка команд `kino-ipc` к TypeScript, окно входа открывается первым и единственным до входа, главное окно появляется только после успешного входа.

## Ограничения

- `pnpm` 9, Node 22+, Universal binary (aarch64 + x86_64) обязателен для релиза.
- Сгенерированные TS-типы не править руками.
- Изменение границ крейтов (новый член workspace `src-tauri`) записать в `ARCHITECTURE.md` в том же PR.

## Готово когда

- `pnpm install && cargo tauri dev` открывает окно
- `cargo check --workspace` и обе архитектуры зелёные

## Заметки

Зависит от #014.

2026-11-15: оболочка и каркас UI готовы.
- `src-tauri` (пакет `kinobox-app`, бинарь `KinoBox`): окна `login`/`main` управляются Rust
  (`windows.rs`), окон не бывает нуль. Команды — тонкие обёртки над `AppService` (`commands.rs`).
  `macOSPrivateApi` включён (фича `tauri`, поле конфига) для vibrancy окна входа.
- `ui`: Vite + React 18 + TS + Zustand + Tailwind, один бандл на оба окна по `label`. `LoginScreen`
  простой, но рабочий (форма, ошибка, Unavailable+Retry); блок зеркал — задача #016. `MainScreen` —
  приветствие и «Выйти».
- `kino-ipc`: DTO получили `specta::Type`; `user_id`/`latency_ms` экспортируются в TS как `number`
  (`#[specta(type = Number)]`), сериализация serde не изменилась.
- `ARCHITECTURE.md`/`README.md`/`src-tauri/CRATE.md` обновлены (крейты, правило зависимостей, окна,
  macOSPrivateApi, раздел «Сборка»).
- Исправлена инфраструктура CI: `.github/workflows/ci.yml` запускался только для PR с базой `main`,
  а вся цепочка PR #1–#16 стоит друг на друге — CI почти никогда не выполнялся. Теперь запускается на
  любой PR и дополнен проверками #015 (обе архитектуры, генерация bindings.ts, `ui` typecheck/test/build).
  `release.yml` был сломан (`working-directory: apps/desktop`, каталога не существует) — исправлено на
  `ui`. Добавлен `build-dev.yml` — ручная сборка universal `.app` по кнопке в Actions, артефакт на
  14 дней, чтобы не собирать релизный бинарь локально на слабой машине.
- Проверки лично: `cargo check --workspace`, `cargo test --workspace` (211 тестов), `cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo check --target aarch64-apple-darwin` — все зелёные локально.
  `--target x86_64-apple-darwin` и `cargo tauri build --target universal-apple-darwin` перенесены в CI
  (GitHub Actions, macOS runner), чтобы не грузить слабый ноутбук заказчика (i3-1000G4) длительной сборкой.
- Живая проверка (`cargo tauri dev`, собранный `.app`) не делалась и не будет: читает реальный Keychain
  и логинится на живом HDrezka. Проверяет сам заказчик.
- CI на PR #17 (GitHub Actions, macOS runner): все три job зелёные — `rust` (fmt, clippy, test,
  обе архитектуры aarch64+x86_64, 9м15с), `bindings` (генерация и детерминированность, 4м41с),
  `ui` (typecheck/test/build, 22с). Это закрывает `x86_64-apple-darwin`, который локально не прогонялся.
- `cargo tauri build --target universal-apple-darwin` (через `build-dev.yml`) пока не запущен:
  `workflow_dispatch` регистрируется в GitHub Actions только для воркфлоу, уже попавшего в `main`,
  а цепочка PR #1–#17 ещё не смержена. Запустить вручную после мержа.
