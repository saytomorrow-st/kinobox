# kino-player

Обёртка `libmpv2`: headless-ядро воспроизведения (play/pause/seek/volume, качество, субтитры,
события — #017) и встраивание OpenGL render API `libmpv` в `NSView` окна (#018). Оба модуля
независимы — `src-tauri` (#021) связывает их: создаёт `Player`, получает хендл окна `player` и
передаёт его `mpv_handle` в `PlayerRenderContext::attach`.

Реэкспортировано из корня крейта: `Player`, `PlayerEvent`, `EndReason`,
`WindowRenderTarget`, `PlayerRenderContext`, `RenderError`, `WindowedPlayer`.

## Player (headless-ядро, #017)

```rust
pub struct Player { /* приватно */ }

impl Player {
    pub fn headless() -> libmpv2::Result<Self>;
    pub fn windowed() -> libmpv2::Result<Self>;
    pub fn mpv_handle(&self) -> &libmpv2::Mpv;

    pub fn play(&self) -> libmpv2::Result<()>;
    pub fn pause(&self) -> libmpv2::Result<()>;
    pub fn is_paused(&self) -> libmpv2::Result<bool>;

    pub fn seek_to(&self, seconds: f64) -> libmpv2::Result<()>;
    pub fn position(&self) -> libmpv2::Result<Option<f64>>;
    pub fn duration(&self) -> libmpv2::Result<Option<f64>>;

    pub fn volume(&self) -> libmpv2::Result<f64>;
    pub fn set_volume(&self, volume: f64) -> libmpv2::Result<()>;

    pub fn load_quality(&self, quality: &kino_core::StreamQuality) -> libmpv2::Result<()>;
    pub fn switch_quality(&self, quality: &kino_core::StreamQuality) -> libmpv2::Result<()>;
    pub fn add_subtitle(&self, subtitle: &kino_core::SubtitleTrack) -> libmpv2::Result<()>;

    pub fn observe_playback_properties(&self) -> libmpv2::Result<()>;
    pub fn next_event(&self, timeout: f64) -> Option<PlayerEvent>;
}
```

### Создание

`Player::headless()` создаёт `Mpv::with_initializer`, выставляя `vo=null` и `ao=null` до старта
плеера. Никакого окна и звукового устройства при этом не требуется — используется в тестах этого
крейта.

`Player::windowed()` выставляет `vo=libmpv` (звук — платформенный выбор mpv, не переопределяется):
это обязательное условие для `render::PlayerRenderContext::attach` — без `vo=libmpv`,
выставленного **до** старта mpv, `create_render_context` не создаёт OpenGL render-контекст (так же
в примере `libmpv2` `examples/opengl.rs`). Поэтому `vo` нельзя сменить после `headless()` отдельным
вызовом — нужен отдельный конструктор. `mpv_handle()` отдаёт `&libmpv2::Mpv` наружу: `src-tauri`
(#021) создаёт `Player::windowed()`, получает хендл окна `player` и передаёт `player.mpv_handle()`
в `PlayerRenderContext::attach` вместе с `WindowRenderTarget::new(window_handle)`.

### Воспроизведение

`play`/`pause`/`is_paused` — свойство mpv `pause`. `seek_to` — абсолютная перемотка (`seek <с>
absolute`). `position`/`duration` читают `time-pos`/`duration`; пока файл не загружен или
длительность неизвестна (например прямой поток без точного `Content-Length`), mpv возвращает
`PropertyUnavailable` — это превращается в `Ok(None)`, а не в ошибку. `volume`/`set_volume` —
свойство `volume` (0–130 по умолчанию, ограничение `volume-max` в mpv).

### Загрузка потока и субтитров

`load_quality` запускает `loadfile <url> replace`, используя `StreamQuality::url` (прямой адрес;
`hls`/`fallbacks` в этой задаче не задействованы — mpv доигрывает HLS и по прямому адресу,
выбор резервного потока при обрыве основного решается в #019).

`switch_quality` запоминает `position()` до переключения и передаёт её как per-file опцию
`start=<с>` самой команде `loadfile` (четвёртый аргумент, `-1` третьим — индекс вставки,
игнорируется для `replace`), а не отдельным `seek_to` после неё: `loadfile` асинхронна (команда
возвращается, не дожидаясь, пока новый файл фактически загрузится), поэтому немедленный `seek`
после неё в большинстве случаев получает `MPV_ERROR_COMMAND` (на момент вызова ещё нет текущего
файла, которому можно сказать «перематывай») — это подтверждено эмпирически при headless-тестах
(`switch_quality_preserves_position` падал с `Raw(-12)` на исходной версии через `seek_to`).
`start=` — опция, которую устанавливает сам mpv, когда новый файл будет готов, поэтому гонки нет.
Для `None` (например пока предыдущий файл ещё не загрузился) опция не передаётся.

`add_subtitle` добавляет внешнюю дорожку командой `sub-add <url> <select|auto> <label> <lang>`:
`select`, если `SubtitleTrack::is_default`, иначе `auto` (трек добавляется, но не включается
автоматически).

### События

`observe_playback_properties` подписывает плеер на `time-pos`/`duration`/`pause` один раз после
создания. `next_event(timeout)` оборачивает `wait_event` и проецирует `libmpv2::events::Event` в
`PlayerEvent`:

```rust
pub enum PlayerEvent {
    Position(f64),
    Duration(f64),
    Paused(bool),
    EndOfFile(EndReason),
    Error(String),
    Other,
}

pub enum EndReason { Eof, Error, Quit, Stop, Redirect, Unknown(u32) }
```

События, не входящие в этот список (`Seek`, `FileLoaded`, `VideoReconfig`, `Shutdown` и т.д.),
приходят как `PlayerEvent::Other` — так сигнатура `next_event` не меняется при появлении новых
событий mpv, а вызывающий сам решает, обрабатывать ли `Other`.

### Headless-тестовая стратегия

Тесты (`cargo test -p kino-player`) требуют установленный `libmpv.dylib` (`brew install mpv`,
есть в CI, см. `.github/workflows/ci.yml`), но не открывают окно, не используют звуковое
устройство и не делают сетевых запросов:

- `Player::headless()` — `vo=null`, `ao=null`.
- Источник — локальный файл `tests/fixtures/silence.wav` (16-бит PCM моно 8 кГц, 2 секунды, синус
  440 Гц), сгенерированный `python3`+`wave` при разработке задачи. Для субтитров тест пишет
  временный `.srt` рядом и удаляет его после себя.

**Почему файл, а не `av://lavfi`:** проверялось вручную перед написанием тестов. Официальный
Homebrew-формуль `mpv` сам тестирует себя через `mpv --ao=null --vo=null <фикстура>.wav`, а не
через `av://lavfi:testsrc` — в документации mpv `av://` описан только для `libavdevice`-входов
вида `av://v4l2:/dev/video0`, синтаксис `av://lavfi:...` не документирован и не был подтверждён
как рабочий на этой машине. Локальный WAV-файл воспроизводится предсказуемо headless и не требует
видеодекодера — этого достаточно для play/pause/seek/volume/события/смена качества.

Покрыто: `position`/`duration` равны `None` до загрузки, play/pause переключают `is_paused`,
`seek_to` двигает позицию, `volume`/`set_volume` работают без устройства вывода, `next_event`
сообщает `Paused`/`EndOfFile(EndReason::Eof)` после `observe_playback_properties`, `switch_quality`
восстанавливает позицию после смены файла, `add_subtitle` не возвращает ошибку без видеодорожки.

## Встраивание рендера в окно (`render`, #018)

Реэкспортировано из корня крейта: `RenderError`, `WindowRenderTarget`, `PlayerRenderContext`.

### Архитектурная причина

`libmpv` умеет рисовать либо в offscreen-буфер, либо напрямую в нативное окно через свой OpenGL
render API (`mpv_render_context`). Рисовать поверх/внутри `WKWebView` (как в обычных окнах Tauri)
он не может — `WKWebView` — отдельный `NSView` с собственным Core Animation слоем, который не
подставить как цель для стороннего OpenGL-рендера. Поэтому в `src-tauri` окно `player` создаётся
**без веб-контента** (не `tauri::WebviewWindow`, а чистое `tauri::window::Window`): у него есть
собственный `NSView`, и его хендл отдаётся в `WindowRenderTarget::new`. Элементы управления
(прогресс-бар, кнопки) рисует отдельное прозрачное окно `player-controls` с HTML/CSS/React —
оно лежит поверх `player` и не мешает OpenGL-рисованию под собой. Синхронизация размера и позиции
двух окон — задача #021, этот крейт её не делает.

### Публичный API

```rust
pub struct WindowRenderTarget { /* GL-окружение конкретного окна: NSView + NSOpenGLContext */ }
impl WindowRenderTarget {
    pub fn new(handle: impl raw_window_handle::HasWindowHandle) -> Result<Self, RenderError>;
}

pub struct PlayerRenderContext<'a> { /* RenderContext libmpv, привязанный к окну */ }
impl<'a> PlayerRenderContext<'a> {
    pub fn attach(mpv_handle: &'a libmpv2::Mpv, target: WindowRenderTarget) -> Result<Self, RenderError>;
    pub fn render(&self, width: i32, height: i32) -> Result<(), RenderError>;
    pub fn view_size(&self) -> (f64, f64);                      // размер NSView в точках
    pub fn set_update_callback<F: Fn() + Send + 'static>(&mut self, callback: F);
}
```

- `WindowRenderTarget::new` принимает любой `HasWindowHandle` (значит — и `tauri::window::Window`,
  и тестовые заглушки), достаёт `RawWindowHandle` и на macOS требует именно вариант `AppKit` —
  иначе `RenderError::UnsupportedHandleKind` без обращения к системным API.
- `PlayerRenderContext::attach` принимает `mpv_handle: &Mpv` — тот же `Mpv`, что держит внутри себя
  `Player::headless()` (у `kino-player` пока нет публичного метода отдать свой `&Mpv` наружу; это
  добавит #021 вместе со связыванием `Player` и рендера в окне `src-tauri`). Создаёт
  `mpv_render_context` c `RenderParamApiType::OpenGl` и переводит владение GL-окружением
  (`WindowRenderTarget`) внутрь контекста — после `attach` окном управляет только
  `PlayerRenderContext`.
- `render(width, height)` — один кадр: `makeCurrentContext`, `mpv_render_context_render` в задний
  буфер (`fbo = 0`), `flushBuffer`, `report_swap`. `width`/`height` — размер в **пикселях
  фреймбуфера**, не в точках `NSView` (вызывающая сторона умножает на `backingScaleFactor`).
- `view_size()` — текущий размер `NSView` в точках, для расчёта `width`/`height` выше.
- `set_update_callback` — подписка на «кадру нужен редрав» от `libmpv` (например, пришёл новый
  видеокадр); сам redraw-цикл (таймер/`CVDisplayLink`) эта задача не реализует — это #021/#024.

### Платформенная реализация (macOS)

- GL-контекст — `NSOpenGLContext` + `NSOpenGLPixelFormat` (`objc2-app-kit`), привязанный к `NSView`
  через `setView`. API помечено Apple как deprecated с Catalina в пользу Metal, но остаётся рабочим
  минимум до текущих версий macOS; риск и минимальная поддерживаемая версия системы (`12.0` в
  `tauri.conf.json`) описаны в `ARCHITECTURE.md`, раздел «Встраивание libmpv в окно». Если Apple
  когда-нибудь убирает `OpenGL.framework` — переход на Metal — отдельная задача.
- Резолв GL-функций для `libmpv` (`get_proc_address`) — через `dlopen`/`dlsym` на системном
  `/System/Library/Frameworks/OpenGL.framework/OpenGL`, без зависимости `libc` (две строки `extern
  "C"` на сами функции) и без фичи `objc2-open-gl` (не нужна: резолв идёт через `dlsym`, не через
  CGL-контекст).
- `NSView` из хендла получается через `Retained::retain` на `ns_view` из `AppKitWindowHandle` —
  безопасно только потому, что `raw-window-handle` гарантирует живой объект на момент вызова.
  Создание GL-контекста и привязка к `NSView` требуют главного потока (`MainThreadMarker`) —
  иначе `RenderError::NotMainThread`.

### Ошибки (`RenderError`)

| Вариант | Когда |
|---|---|
| `HandleUnavailable` | `HasWindowHandle::window_handle()` вернул ошибку |
| `UnsupportedHandleKind` | хендл не `AppKit` на macOS (или любой — не на macOS) |
| `InvalidWindowHandle` | `ns_view` хендла не указывает на живой `NSView` |
| `NotMainThread` | `WindowRenderTarget::new` вызван не с главного потока |
| `GlContextInitFailed` | не удалось создать `NSOpenGLPixelFormat`/`NSOpenGLContext` |
| `GlLoadFailed` | не удалось открыть `OpenGL.framework` (`dlopen`) |
| `Mpv` | `libmpv2` вернул ошибку при создании/использовании `mpv_render_context` |
| `UnsupportedPlatform` | платформа не macOS — встраивание реализовано только под macOS |

### Что проверено и что нет

Компилируется и проходит юнит-тесты (`cargo test -p kino-player`): резолв известного GL-символа
(`glClear`) и `null` для неизвестного через собственный `dlopen`/`dlsym`, отказ по типу хендла
(`AppKit` vs `UiKit`/`Win32`) без обращения к objc2. Намеренно **не покрыто** тестом создание
GL-контекста на настоящем `NSView` и сам вызов `render()` — это требует реального окна и главного
потока с запущенным `NSApplication`/event loop, то есть GUI-запуска, который агенту запрещён.

**Не проверено визуально без GUI — проверяет заказчик после `build-dev.yml`.** В частности: что
кадр действительно появляется в `NSView` окна `player`, что `view_size()`/масштаб к пикселям дают
корректный результат на Retina-дисплее, и что `set_update_callback` реально вызывается при
декодировании кадров libmpv.

## `WindowedPlayer`: владеющая пара `Player` + `PlayerRenderContext` (#021)

```rust
pub struct WindowedPlayer { /* приватно */ }

impl WindowedPlayer {
    pub fn attach(target: WindowRenderTarget) -> Result<Self, RenderError>;
    pub fn player(&self) -> &Player;
    pub fn render_context(&self) -> &PlayerRenderContext<'static>;
    pub fn render_context_mut(&mut self) -> &mut PlayerRenderContext<'static>;
}
```

### Зачем нужен

`PlayerRenderContext<'a>::attach` (выше) заимствует `&'a libmpv2::Mpv` из `Player`. `src-tauri`
(#021) должно держать и `Player`, и привязанный к нему `PlayerRenderContext` вместе в одном
`tauri::State` на всё время, пока открыто окно `player` — то есть именно в виде одной владеющей
структуры, а не пары с явным параметром времени жизни (который было бы невозможно выразить для
значения, создаваемого в рантейме уже после старта приложения). `WindowedPlayer::attach` создаёт
`Player::windowed()`, кладёт его в `Box` и безопасно продлевает заимствование `&Mpv` до
`'static` внутри одного `unsafe`-блока (единственного во всём крейте за пределами `render::macos`),
обоснование и оба инварианта, которые делают это корректным — в комментарии модуля
`src/windowed.rs`: `player` в `Box` (адрес на куче не меняется при перемещении самого
`WindowedPlayer`) и порядок полей (`render_context` раньше `player`, Rust гарантированно дропает
поля в порядке объявления — значит заимствование отпускается раньше, чем уходит сам `Player`).

`attach` — атомарная операция: при ошибке привязки к окну (`RenderError`) уже созданный `Player`
уничтожается вместе с ней, наружу не просачивается «голый» `Player` без рендера.

### `WindowedPlayer: Send`, но не `Sync`

`PlayerRenderContext` хранит на macOS `Retained<NSView>`/`Retained<NSOpenGLContext>` —
`objc2`/`objc2-app-kit` намеренно не реализуют для них `Send`/`Sync`, потому что вызывать методы
AppKit разрешено только с главного потока. Но `tauri::State`/`Manager::manage` (нужны `src-tauri`,
#021, чтобы положить `Mutex<Option<WindowedPlayer>>` в управляемое состояние) требуют
`T: Send + Sync + 'static` — для `Mutex<T>` это сводится к `T: Send`.

Крейт даёт `unsafe impl Send for WindowedPlayer {}` (не `Sync`): перемещать владение
`Retained<NSView>`/`Retained<NSOpenGLContext>` между потоками безопасно само по себе — опасность
представляет не факт обладания указателем, а вызов AppKit-методов не с главного потока, а это
`Send`/`Sync` в принципе не могут выразить (это свойство конкретного рантайма, не типа). Поэтому
соблюдение инварианта «методы, трогающие `render_context` (`attach`, `render`, `view_size`,
`set_update_callback`) — только с главного потока» остаётся обязанностью вызывающей стороны
(`src-tauri`), не гарантируется типами этого крейта. Методы `Player` (`play`/`pause`/`seek_to`/
`volume`/`load_quality`/...) трогают только `libmpv2::Mpv` (сам `Send + Sync`, гарантия
`libmpv2`) и безопасны с любого потока без этой оговорки.

### Что не покрыто тестом

Как и `render::attach` — реальное создание `WindowedPlayer` требует живого `NSView`/главного
потока (GUI), агенту запрещён. Корректность `unsafe`-блока проверена логически (комментарий
модуля) и `cargo clippy`/`cargo check`, а не юнит-тестом.

## Зависимости

`libmpv2 =7.0.1` (общий для `Player` и `render` — одна версия, одна линковка `-lmpv`),
`raw-window-handle =0.6.2` (фича `std` — нужна для `std::error::Error` на `HandleError`, через
который `thiserror` реализует `#[from]`), `url` (фикстуры тестов). На macOS: `objc2 =0.6.5`,
`objc2-app-kit =0.3.2` (`default-features = false`, фичи `std`, `NSOpenGL`, `NSResponder`,
`NSView`).

## Лицензия

`libmpv2` — LGPL-2.1: https://github.com/anlumo/libmpv2-rs/blob/master/LICENSE. `kino-player`
линкуется с `libmpv.dylib` динамически (через `libmpv2-sys`, pregenerated bindings без
`use-bindgen`) — распространение динамической линковки с LGPL-библиотекой само по себе не требует
открытия кода `kino-player`. Сама библиотека mpv (используемая как `brew install mpv` на этапе
сборки/тестов, не распространяется в составе приложения) — `GPL-2.0-or-later AND
LGPL-2.1-or-later`; `kino-player` использует только публичный API `libmpv.dylib`, а не исходники
mpv.

## Тесты

```
cargo test -p kino-player
```

Требует установленный `libmpv` (линковка `-lmpv`, `brew install mpv` на macOS) — сам крейт
собирается без него (через `libmpv2-sys`), но тесты и сборка финального бинарника линкуются.
`cargo check -p kino-player` не требует `libmpv.dylib` (pregenerated bindings).

## Сборка

Universal binary: `cargo check --workspace --target x86_64-apple-darwin` и
`--target aarch64-apple-darwin`. Крейт целиком заточен под macOS (`ARCHITECTURE.md`); на других
платформах `render` компилируется (платформенно-независимый API), но любой вызов возвращает
`RenderError::UnsupportedPlatform`.
