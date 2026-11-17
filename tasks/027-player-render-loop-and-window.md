---
id: 027
type: bug
status: done
model: deepseek-v4-pro
crate: kino-player, kino-ipc
created: 2026-11-17
---

## Задача

Пользователь протестировал сборку на реальном железе (macOS 13.7.8, Intel)
и сообщил 5 багов. Этот task закрывает два из них — оба в пути открытия
окна плеера:

1. **Серый экран плеера** (баг #3): открывается окно `player`, фильм
   пытается играть, но видео не отображается — пустой/серый `NSView`.
2. **Плеер открывается в отдельном неинтерактивном окне** (баг #4):
   окно `player` визуально не привязано к `main` (вылетает в случайное
   место экрана) и не реагирует на клики.

## Расследование (сделано lead-агентом, см. ниже — используй как есть, не
переделывай с нуля)

### Баг #3 — корень найден точно, 100% подтверждено гэрепом по всему репо

`crates/kino-player/src/render.rs` и `windowed.rs` определяют
`PlayerRenderContext::render(width, height)`, `::view_size()`,
`::set_update_callback(callback)` — но **ни один из этих трёх методов
не вызывается НИГДЕ в проекте** (проверено исчерпывающим grep по всем
`*.rs` на `.render(`, `set_update_callback`, `view_size`, `redraw`,
`RedrawRequested`, `request_redraw` — только определения, ни одного
вызова). `WindowedPlayer::attach` успешно создаёт GL-контекст и привязывает
его к `NSView` окна `player` (поэтому окно само открывается нормально) —
но raw-кадр в этот `NSView` никогда не рисуется, поэтому видно только
исходный пустой фон `NSView` (серый/чёрный) — независимо от того,
воспроизводит ли libmpv видео в фоне.

Сам `render.rs` прямо документирует эту недоделку:
> Подписка на «кадр готов»/«нужен редрав» — вызывающая сторона должна
> запланировать повторный `render` на главном потоке. **Самого
> redraw-цикла здесь нет (см. #021/#024).**

`tasks/021-ipc-streams-player-window.md` (статус до сих пор `open`) и
`tasks/024-ui-player-screen.md` (grep на redraw/render_context/
update_callback/view_size — ноль совпадений) оба не реализовали эту часть.

**Нужно сделать**: завести redraw-цикл в `src-tauri`. Рекомендуемый подход
(соответствует задумке автора крейта — именно под это `set_update_callback`
и существует):

1. В `attach_player()` (`src-tauri/src/commands.rs`, ~строка 311-320) сразу
   после успешного `attach` вызвать
   `windowed.render_context_mut().set_update_callback(move || { ... })`.
2. Callback вызывается из потока libmpv (НЕ главного) — внутри callback
   нужно запланировать рендер на главном потоке, т.е. переиспользовать
   существующий паттерн `on_main`/`window.run_on_main_thread` (см.
   `commands.rs` рядом, функция `on_main<T, F>`).
3. На главном потоке: `view_size()` отдаёт размер NSView в points — нужно
   умножить на `window.scale_factor()` (у `tauri::Window` есть такой метод)
   чтобы получить пиксели для `render(width_px, height_px)`.
4. `set_update_callback` — единственный `&mut self` метод, поэтому нужен
   мьютабельный доступ к `render_context_mut()` — уже есть на
   `WindowedPlayer`.
5. Убедиться, что сам `render()`-вызов тоже промаршален через `on_main`
   (как и `attach`/`view_size` уже промаршалены) — AppKit/GL вызовы должны
   идти только с главного потока (см. комментарии в `windowed.rs`:
   `unsafe impl Send for WindowedPlayer {}`, НЕ `Sync`).

Альтернатива (проще, но менее отзывчиво): дёргать `render()` периодически
прямо из уже существующего `start_progress_thread`'а (0.1с поллинг-цикл в
`commands.rs`) через тот же `on_main`. Предпочтительно использовать
update-callback-подход, но если он не взлетит с первой попытки CI —
таймерный вариант как fallback.

Проверить отдельно: нет ли гонки между `window.show()` в
`windows::open_player()` и `attach_player()`'s вызовом `WindowRenderTarget::
new`/`WindowedPlayer::attach` (оба идут через главный поток разными путями —
порядок вызовов в `commands.rs`'s `player_load` выглядит ОК на первый
взгляд, но если фикс не взлетит с первого раза — смотреть сюда).

### Баг #4 — геометрия/фокус окна `player`

`src-tauri/src/windows.rs`'s `open_player()` (строки 80-116):
- Окно `player` создаётся через `tauri::window::WindowBuilder::from_config`
  (не webview — голое окно под NSView/OpenGL, см. задачу #018).
- После `window.show()` (строка 113) **НЕТ вызова `window.set_focus()`**
  — в отличие от обобщённого хелпера `open()` (строки 32-48, вызывает и
  `show()`, и `set_focus()`) и от `open_player_controls()` (строки 118-134,
  тоже вызывает оба).
- В `src-tauri/tauri.conf.json` у окна `player` **нет `center: true` и
  нет явной позиции** (`x`/`y`) — окно появляется там, где ОС/Tauri решит
  по умолчанию (обычно каскадом/в углу), никак не привязано к позиции
  `main`. Это и есть «выпал в отдельное окно».
- Геометрия `player`/`player-controls` НИГДЕ не синхронизируется между
  собой или с `main` — grep по resize/move-обработчикам в `src-tauri/src`
  не нашёл ничего, кроме `CloseRequested`/`Destroyed` в `windows.rs`.
  Задача #021 явно обещала «синхронизацию геометрии» в своём scope, но
  её статус до сих пор `open` — похоже, эта часть просто не сделана.

**Нужно сделать**:
1. Добавить `window.set_focus()` в `open_player()` после `show()` (строка
   ~113), как в `open()`/`open_player_controls()`.
2. Дать окну `player` центрирование относительно `main` (или относительно
   экрана, если проще и достаточно для фикса) — либо через `center: true`
   в `tauri.conf.json` (если Tauri это поддерживает для не-webview окна —
   проверить), либо явным вычислением позиции в `open_player()` через
   `main_window.outer_position()`/`outer_size()` и `player_window.
   set_position(...)` до/после `show()`.
3. `player-controls` уже имеет `parent: "player"` в конфиге — если `player`
   позиционируется корректно, `player-controls` как child-окно должен идти
   автоматически синхронно (проверить это допущение, не факт).

## Ограничения

**ВАЖНО — ограничения окружения заказчика (копия из AGENTS.md, обязательна
к соблюдению, не нарушать):**

Машина заказчика — слабый Intel Mac (i3-1000G4, 2 физических ядра, 8 ГБ ОЗУ,
macOS 13.7.8). На ней:

- **Запрещено `brew install mpv` и любая попытка собрать `libmpv`/`mpv` из
  исходников вручную**, в том числе как «обходной путь». На этой машине
  нет x86_64-бутылки mpv в homebrew-core — сборка тянет полный граф
  зависимостей и может идти часами на двух ядрах.
- `kino-player` и `src-tauri` линкуют `-lmpv` — **локальный `cargo check`/
  `cargo test`/`cargo build` для них на машине заказчика НЕВОЗМОЖЕН и
  ЗАПРЕЩЁН**. Вся проверка — только через CI (`gh pr checks`/`gh run list`,
  ждать `completed`), никогда локальным запуском.
- **Запрещены** `cargo tauri dev`, собранный `.app`, бинарь `KinoBox`,
  `cargo run -p kinobox-app` — живой запуск приложения лид-агентом или
  subagent'ом запрещён в принципе.
- **Запрещены циклы повторных прогонов** («на всякий случай ещё раз») —
  один прогон CI, зелёный — готово, красный — чини осознанно и перезапускай
  один раз, не вслепую по кругу.
- Universal-сборка, реальная линковка `libmpv`, живой рендер в окно —
  проверяются ТОЛЬКО через CI. У тебя (subagent) нет возможности визуально
  убедиться, что видео реально рисуется — весь путь должен быть
  обоснован чтением кода + компиляцией через CI, не живым запуском.

Эти ограничения тебе не наследуются автоматически — применяй их буквально
к себе прямо сейчас, в этой задаче.

- Не трогать файлы вне `crates/kino-player/src/render.rs`,
  `crates/kino-player/src/windowed.rs`, `src-tauri/src/commands.rs`,
  `src-tauri/src/windows.rs`, `src-tauri/tauri.conf.json` без необходимости.
- Если меняешь публичный API `kino-player` (например, сигнатуру
  `set_update_callback`/`render`) — обнови `crates/kino-player/CRATE.md` в
  том же коммите.
- Пароль/токены — не логировать (но эта задача их не касается).

## Готово когда

- Реализован redraw-цикл: `set_update_callback` подключён, рендер реально
  вызывается на главном потоке с корректной конвертацией points→pixels.
- `open_player()` вызывает `set_focus()` после `show()`.
- Окно `player` появляется центрированным/привязанным к `main` (не в
  случайном месте экрана).
- `cargo check`/`cargo test` на НЕ-mpv-линкующих крейтах (kino-core,
  kino-api, kino-store) — если правка их не касается, этот пункт не
  применим, вся проверка через CI.
- CI (`build-dev.yml`) зелёный на PR-ветке — единственное реальное
  подтверждение, что всё компилируется и собирается.
- PR открыт по шаблону, лид-агент делает финальное ревью перед мержем.

## Заметки

[Ведёт агент: ход работы, найденные нюансы, blockers]

Расследование и формулировка задачи — lead-агент (claude-sonnet-5),
17.11.2026, по прямому коду (без живого запуска). Оба бага (#3 grey-screen,
#4 detached window) подтверждены исчерпывающим grep + построчным чтением
`render.rs`/`windowed.rs`/`macos.rs`/`commands.rs`/`windows.rs`/
`tauri.conf.json` — корни независимы (разные слои), но правятся в одном PR
т.к. соседние по коду.

Реализация влита: PR #29, merge commit `00ea924` (17.11.2026). CI на PR зелёный: rust, bindings, ui; одиночный rerun UI прошёл после временной нехватки hosted runner. Свежий dev build `44113442158` на `main` запущен для проверки на реальном Intel Mac; результат ожидается.


- **Баг #3** (redraw-цикл): `commands.rs::attach_player` теперь принимает
  `shared: Arc<Mutex<Option<WindowedPlayer>>>` и после `attach` вызывает
  `render_context_mut().set_update_callback(move || schedule_render(...))`.
  Новая `schedule_render(window, shared)` маршалит рендер на главный поток
  через `run_on_main_thread`: берёт `view_size()` (points) → умножает на
  `window.scale_factor()` → `render(pw, ph)` с логом ошибки. Callback `'static`
  (владеет клонами `Window`/`Arc`), сам AppKit/GL-код идёт только с главного
  потока — инвариант `windowed.rs` соблюдён. Публичный API `kino-player` НЕ
  менялся (`set_update_callback`/`render`/`view_size` сигнатуры прежние), поэтому
  `crates/kino-player/CRATE.md` не трогался.
- **Баг #4** (окно `player`): `windows.rs::open_player` теперь зовёт
  `window.set_focus()` после `show()` (как `open`/`open_player_controls`), а в
  `tauri.conf.json` окну `player` добавлен `center: true`. Центрирование по
  экрану (вариант «проще и достаточно» из ТЗ) — явное позиционирование
  относительно `main` не делалось: `player-controls` уже `parent: "player"`,
  следует за родителем автоматически.

Нюансы, выявленные при реализации:
- `tauri::Window::scale_factor()` возвращает `Result<f64>` (не `f64`) — проверено
  по исходникам tauri 2.12.5 (`src/window/mod.rs:1570`), обработано
  `.unwrap_or(1.0)`.
- `view_size()` в `kino-player` помечен `#[cfg(target_os = "macos")]` — ок,
  src-tauri целиком под macOS (CI чекает только apple-darwin-цели).
- Порядок `show()` в `open_player` → `attach_player` не менялся; гонки не видно
  (оба идут через главный поток, а первый кадр рисуется уже после `load_quality`).

Вся проверка — через CI (`ci.yml` на PR: fmt/clippy/test/check). Локальный cargo
на src-tauri/kino-player не запускался (линкуют -lmpv, запрещено).
