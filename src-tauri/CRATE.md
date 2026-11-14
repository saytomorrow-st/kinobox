# src-tauri (kinobox-app)

Оболочка Tauri v2: окна, регистрация команд `kino-ipc`, привязки TypeScript. Зависит только от
`kino-ipc` (правило из `ARCHITECTURE.md`).

Пакет называется `kinobox-app`, бинарь — `KinoBox` (`default-run`).

## Состояние приложения

```rust
pub type Service = AppService<KeychainSecretStore, MemoryMirrorStore>;
```

Один `AppService` создаётся при старте (`state::create_service`) и кладётся в `tauri::State`.
Сеть и Keychain не трогаются при создании — первое обращение делает команда `boot`.

**Известное ограничение:** активное зеркало хранится в `MemoryMirrorStore` и не сохраняется между
перезапусками приложения — при каждом старте зеркало выбирается заново (`resolve`). SQLite-стор из
`kino-store` заменит второй параметр `Service`, когда появится.

## Команды

Тонкие обёртки над `AppService` в `src/commands.rs`, помечены `#[tauri::command] #[specta::specta]`:

| Команда | Сигнатура | Чем отличается от `AppService` |
|---|---|---|
| `boot` | `() -> Result<BootState, IpcError>` | при `SignedIn` открывает `main` и закрывает `login`; иначе показывает `login` |
| `login` | `(email, password) -> Result<UserDto, IpcError>` | при успехе открывает `main` и закрывает `login` |
| `logout` | `() -> Result<(), IpcError>` | открывает `login` и закрывает `main` |
| `session` | `() -> Option<UserDto>` | без изменений |
| `mirrors` / `check_mirrors` | `() -> Result<Vec<MirrorDto>, IpcError>` | без изменений |
| `add_mirror` / `select_mirror` | `(url) -> Result<MirrorDto, IpcError>` | без изменений |
| `quit` | `()` | `app.exit(0)` |

Логика окон нигде не дублируется: обе точки входа (`boot`, `login`) используют `windows::enter_main`,
а `logout` — `windows::enter_login`.

## Окна и их жизненный цикл

Окнами управляет Rust (`src/windows.rs`), не фронтенд — это гарантирует, что открытых окон не бывает
нуль:

- `windows::open(app, label)`: показывает окно, если оно уже существует, иначе создаёт его по описанию
  из `tauri.conf.json` (`WebviewWindowBuilder::from_config`).
- `windows::close(app, label)`: закрывает окно, если оно есть; ошибку закрытия игнорирует (окно либо
  уже закрывается, либо приложение завершается).
- `windows::enter_main`: сначала `open(MAIN)`, потом `close(LOGIN)`.
- `windows::enter_login`: сначала `open(LOGIN)`, потом `close(MAIN)`.

Окно `login` в конфигурации создаётся скрытым (`visible: false`) и показывается только когда `boot`
не привёл к тихому входу (`NeedsLogin` или `Unavailable`) — чтобы не мигать при автоматическом входе
сохранёнными данными. Закрытие окна `login` крестиком до успешного входа завершает приложение: вход
обязателен с первого запуска (поведение по умолчанию у Tauri, никакого отдельного обработчика не
требуется — без `main` и без обработчика `close-requested` на `login` закрытие последнего окна
останавливает процесс).

### Настройки окон (`tauri.conf.json`)

- `login`: 560×340, не изменяется по размеру, по центру, без системной заголовочной панели
  (`titleBarStyle: Overlay` + `hiddenTitle`), прозрачный фон и `windowEffects` (`hudWindow`) для
  vibrancy в стиле диалога «Требуется вход» Apple TV. `create: true` (по умолчанию), скрыто до
  первого показа.
- `main`: 1280×800, минимум 960×600, `create: false` (не создаётся автоматически при старте —
  появляется только через `windows::enter_main`).

### `macOSPrivateApi`

Прозрачность и `windowEffects` на macOS требуют приватного API — он включён в `tauri.conf.json`
(`app.macOSPrivateApi: true`). Осознанное решение, согласованное архитектурой: исключает публикацию
в Mac App Store, не мешает распространению через GitHub Releases (как и остальные приложения на Tauri
с vibrancy).

## Привязки TypeScript

Генерируются `tauri-specta` в `ui/src/bindings.ts`:

```
cargo test -p kinobox-app export_bindings
```

Тест вызывает `export_bindings_to` без запуска GUI, без Keychain и без сети — только сборка
JSON-схемы команд. Повторный запуск детерминирован (файл не меняется). Файл помечен заголовком
`// @generated` — не редактировать руками.

### specta и `u64`

`specta` не экспортирует `u64`/`i64` в TypeScript как есть (`BigInt` запрещён по умолчанию). Поля
`UserDto::user_id` и `MirrorDto::latency_ms` помечены `#[specta(type = Number)]` /
`#[specta(type = Option<Number>)]` — экспортируются в TS как `number`, сериализация serde остаётся
`u64`. Обоснование и побочный эффект (точность до 2^53-1) описаны в `crates/kino-ipc/CRATE.md`.

## Capabilities и CSP

`capabilities/default.json` — минимальный набор прав: только команды приложения
(`allow-boot`, `allow-login`, …) и `core:window:allow-start-dragging` (для перетаскивания окна без
системной рамки через `data-tauri-drag-region`). Никаких плагинов `fs`/`shell`/`http`/`opener`.

CSP в `tauri.conf.json` — строгий (`default-src 'self'`, без `unsafe-eval`); для `devCsp` допущен
только `http://localhost:1420` (dev-сервер Vite).

## Сборка

```
cargo tauri dev
cargo tauri build --target universal-apple-darwin
```

Universal binary (x86_64 + aarch64) обязателен для релиза — иначе автообновление не работает у части
пользователей. Обе цели также проверяются без сборки бандла: `cargo check --workspace --target
aarch64-apple-darwin` и `--target x86_64-apple-darwin`.

**Важно:** запускать собранное приложение в тестовом/CI окружении нельзя — при старте `boot` читает
реальный Keychain и логинится на живом HDrezka под настоящим аккаунтом. Проверки ограничиваются
`cargo check`/`test`/`clippy`/`fmt` и сборкой без запуска результата.
