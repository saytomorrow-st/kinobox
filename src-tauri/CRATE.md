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
| `catalog` | `(kind, page) -> Result<Vec<MediaItemDto>, IpcError>` | без изменений |
| `search` | `(query) -> Result<Vec<MediaItemDto>, IpcError>` | без изменений |
| `details` | `(item) -> Result<MediaDetailsDto, IpcError>` | без изменений |
| `episodes` | `(details) -> Result<Vec<EpisodeDto>, IpcError>` | без изменений |
| `streams` | `(item, translatorId, episode) -> Result<Vec<StreamQualityDto>, IpcError>` | раскрывает `Option<TranslatorId>` в `Option<u64>` (`.map(\|id\| id.0)`); детали перезапрашиваются внутри сервиса |
| `player_load` | `(quality, qualities) -> Result<(), IpcError>` | открывает окно плеера, привязывает рендер, грузит качество как **новый файл** (`Player::load_quality`) — позиция не сохраняется; `qualities` — полный список доступных качеств (из `streams`), сохраняется в `PlayerState` для `player_qualities` |
| `player_qualities` | `() -> Result<Vec<StreamQualityDto>, IpcError>` | отдаёт список качеств, сохранённый последним `player_load` — читает окно `player-controls`, у которого нет доступа к стору главного окна |
| `player_play` / `player_pause` | `() -> Result<(), IpcError>` | `Player::play` / `Player::pause` |
| `player_seek` | `(seconds: f64) -> Result<(), IpcError>` | `Player::seek_to` |
| `player_set_quality` | `(quality) -> Result<(), IpcError>` | **сохраняет позицию** (`Player::switch_quality`), только когда плеер уже играет то же произведение |
| `player_set_volume` | `(volume: f64) -> Result<(), IpcError>` | `Player::set_volume` |
| `player_stop` | `() -> Result<(), IpcError>` | закрывает окно `player`; очистка плеера происходит в обработчике события окна |

Логика окон нигде не дублируется: обе точки входа (`boot`, `login`) используют `windows::enter_main`,
а `logout` — `windows::enter_login`.

### Плеер (`player_state.rs`, `commands.rs`)

`WindowedPlayer` (владеющий `Player` + рендер-контекст) живёт в первом поле двухполевого кортежного
`PlayerState(Arc<Mutex<Option<WindowedPlayer>>>, Arc<Mutex<Vec<StreamQualityDto>>>)` и реализует только
`Send`, не `Sync` — поэтому состояние сериализуется `Mutex`. Он привязывается к `NSView` окна `player`
на главном потоке (`run_on_main_thread` + `oneshot`), а очистка (`*lock = None`) происходит в
обработчике события окна (`CloseRequested`/`Destroyed`) — снова на главном потоке, где доставляются события.
Воспроизведение останавливается автоматически, утечек mpv/рендера нет.

Второе поле — список качеств последнего `player_load` (`set_qualities`/`qualities`). Нужен отдельно от
`WindowedPlayer`, потому что окно `player-controls` — отдельная JS-среда без доступа к стору главного
окна: полный список качеств (из `streams`) передаётся в `player_load` и оседает здесь, а `player_qualities`
отдаёт его обратно по запросу контролов.

Фоновый поток прогресса (`start_progress_thread`) опрашивает события mpv (`next_event(0.1)`) и эмиттит снимок
`PlayerProgressDto` в окно `player-controls` событием `player-progress` (сырой `tauri::Emitter::emit_to`,
без tauri-specta — событие не входит в сигнатуры команд). Поток держит мьютекс на время `next_event`
(≤ ~0.1 с), за это время команды `player_play`/`player_pause` могут кратко заблокироваться — осознанная
цена за то, что `WindowedPlayer` (самоссылающаяся структура) нельзя поделить на `Arc`-части. Поток
завершается сам, когда `shared` становится `None`.

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
- `player`: 1280×720, без системных украшений (`decorations: false`), `create: false` — окно **без
  веб-контента**: в его `NSView` встраивается рендер `libmpv`. Создаётся через
  `tauri::window::WindowBuilder::from_config` (unstable API), а не `WebviewWindowBuilder`, поэтому из
  `Manager` его возвращает `get_window`, а `get_webview_window` — `None`.
- `player-controls`: прозрачное окно поверх `player` (`parent: "player"`, `transparent: true`, `shadow:
  false`), с веб-контентом — это UI-оверлей (кнопки play/pause/seek/качество/громкость). Открывается и
  закрывается вместе с `player` (`open_player`/`close_player` и обработчик события окна).

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

Для `streams` нужен `Option`-идентификатор перевода, а specta не пускает голый `u64` в аргумент
команды — поэтому `kino-ipc` вводит newtype `TranslatorId(#[specta(type = Number)] u64)`
(`#[serde(transparent)]`). В TS он рендерится как `export type TranslatorId = number`.

## Capabilities и CSP

`capabilities/default.json` — минимальный набор прав: только команды приложения
(`allow-boot`, `allow-login`, …, `allow-catalog`, `allow-search`, `allow-details`, `allow-episodes`,
`allow-streams`, `allow-player-*`, `allow-player-qualities`), `core:window:allow-start-dragging`
(для перетаскивания окна без системной рамки через `data-tauri-drag-region`),
`core:window:allow-set-ignore-cursor-events` (клик-проходимость прозрачной зоны `player-controls` —
#024) и `core:event:allow-listen`/`core:event:allow-unlisten` (подписка `player-controls` на
`player-progress`). Никаких плагинов `fs`/`shell`/`http`/`opener`.

Окна `player` и `player-controls` добавлены в массив `windows` этой capability, иначе Tauri не
пропускал бы к ним IPC (включая событие `player-progress`).

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
