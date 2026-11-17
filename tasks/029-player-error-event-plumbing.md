---
id: 029
type: bug
status: in-review
model: deepseek-v4-pro
crate: kino-player, kino-ipc, src-tauri, ui
created: 2026-11-17
---

## Задача

Пользователь протестировал сборку (после #027) на реальном железе и сообщил:
«в некоторых озвучках фильмов серый экран, в некоторых ошибка воспроизведения».
Это два симптома одной причины — отсутствие связи между асинхронной ошибкой
mpv (после того как `loadfile` уже принят) и UI.

## Расследование (сделано lead-агентом, используй как есть)

`Player::load_quality()` (`crates/kino-player/src/source.rs`) шлёт mpv команду
`loadfile <url> replace` — **она асинхронна**: `Ok(())` возвращается сразу,
не дожидаясь, пока файл реально откроется. Если URL мёртв/геоблок/неверный
кодек/HLS не парсится — ошибка проявляется ПОЗЖЕ как событие
`PlayerEvent::EndOfFile(EndReason::Error)` или `PlayerEvent::Error(String)`
(`crates/kino-player/src/events.rs`), которое `next_event()` сейчас только
`log::error!`-ит — не даёт вызывающей стороне никакого канала, куда это
доставить.

`src-tauri/src/commands.rs::start_progress_thread()` (хвост файла, ~строки
384-422) — цикл поллинга `next_event(0.1)` на фоновом потоке, эмитит Tauri
событие `player-progress` для `Position`/`Duration`/`Paused`. **У матча нет
веток для `EndOfFile`/`Error` — они проваливаются в `_ => false` и
отбрасываются молча.** Поэтому даже когда `kino-player` уже залогировал
ошибку, ничего не долетает до фронтенда.

Отсюда ровно два разных симптома у пользователя:
- **«Серый экран»** — когда `streams()`/`player_load` сами успешны (URL
  получен, loadfile принят), но сам стрим не открылся — grey screen, т.к.
  ошибка потерялась на пути `events.rs` → `start_progress_thread` → фронтенд.
- **«Ошибка воспроизведения» (`ERROR_LAUNCH`)** — когда `streams()` или сам
  `player_load` (синхронный вызов) вернули ошибку ДО/ВО ВРЕМЯ запуска — этот
  путь уже работает (`ui/src/details-store.ts::play()`, строки 104-132,
  ставит `ERROR_LAUNCH = "Не удалось запустить воспроизведение. Попробуйте
  ещё раз."`), поэтому для части озвучек пользователь видит сообщение, а для
  других (асинхронный провал) — просто серый экран без объяснения.

## Нужно сделать

1. `crates/kino-player/src/events.rs` — не менять enum `PlayerEvent` (не
   обязательно, но проверь по месту: `EndOfFile`/`Error` уже есть, этого
   достаточно) — скорее всего правка не нужна здесь, достаточно читать то,
   что уже возвращает `next_event`.
2. `src-tauri/src/commands.rs::start_progress_thread` — расширить матч:
   на `PlayerEvent::EndOfFile(EndReason::Error)` и `PlayerEvent::Error(msg)`
   эмитить **новое** Tauri-событие, например `player-error`, с payload
   (строка сообщения на русском, человекочитаемая — не технический mpv-текст
   в UI, технический — только в лог). `EndOfFile(EndReason::Eof)` (нормальное
   завершение файла) — НЕ ошибка, не путать с `EndReason::Error`.
3. Добавить DTO для этого события в `kino-ipc` (аналогично существующему
   `PlayerProgressDto`) — посмотри, как сейчас типизирован `player-progress`
   (specta/tauri-specta привязки, `bindings.ts` генерируется) и сделай
   `player-error`/`PlayerErrorDto` тем же паттерном, чтобы `ui/src/bindings.ts`
   обновился автоматически при сборке.
4. `ui/src/player-store.ts` — рядом с существующей подпиской на
   `"player-progress"` в `init()` добавить подписку на `"player-error"`,
   устанавливающую `error` в сторе (тот же механизм, что уже используют
   `ERROR_ACTION`/`ERROR_QUALITIES`).
5. `ui/src/components/PlayerControlsView.tsx` уже рендерит `error` как
   `<p role="alert">` — ничего менять не нужно, если проп `error` придёт из
   стора так же, как раньше.
6. Текст ошибки для пользователя — по-русски, без технических деталей mpv
   (например: «Не удалось воспроизвести поток. Попробуйте другую озвучку или
   качество.»). Технический текст (`EndReason`/raw message) — только в
   `log::error!`, не в UI.

## Ограничения

**ВАЖНО — копия ограничений окружения заказчика из AGENTS.md, обязательна:**
Машина заказчика — слабый Intel Mac (i3-1000G4, 2 ядра, 8 ГБ ОЗУ, macOS
13.7.8). Запрещено `brew install mpv`/сборка `libmpv`/`mpv` из исходников
вручную в любом виде. `kino-player`/`src-tauri` линкуют `-lmpv` —
локальный `cargo check`/`test`/`build` для них ЗАПРЕЩЁН, вся проверка —
только через CI (`gh pr checks`/`gh run list`, ждать `completed`). Запрещены
`cargo tauri dev`, собранный `.app`, `cargo run -p kinobox-app` — живой запуск
приложения запрещён в принципе (лид-агенту и subagent'у). Запрещены циклы
повторных прогонов тестов «на всякий случай» — один прогон CI, зелёный —
готово, красный — чини осознанно, перезапускай один раз.

- Не трогать unrelated части `commands.rs`/`events.rs` без необходимости.
- Если меняется публичный API `kino-player` (например сигнатура `PlayerEvent`
  — не должно понадобиться, но если понадобится) — обнови
  `crates/kino-player/CRATE.md` в том же коммите.
- Если добавляется новый tauri-specta command/event в `kino-ipc`/`src-tauri`
  — проверь, что `ui/src/bindings.ts` действительно регенерируется (обычно
  через `cargo test`/сборку с фичей specta-export, посмотри как это сделано
  для существующих событий) и не редактируй `bindings.ts` руками.
- Русский текст ошибки для пользователя — без утечки URL/токенов потока в UI.

## Готово когда

- CI (`ci.yml`: fmt/clippy/test/check на rust+bindings+ui) зелёный на
  PR-ветке.
- Асинхронный провал mpv (эмулируемый юнит-тестом на `start_progress_thread`,
  если тестируется без живого mpv — иначе логическая проверка кода +
  описание в `tasks/029-...md`, что живая проверка — только у заказчика) ведёт
  к видимому сообщению об ошибке в `PlayerControlsView`, а не к бесконечному
  серому экрану.
- PR открыт по шаблону, лид-агент делает финальное ревью перед мержем.

## Заметки

Расследование — lead-агент (claude-sonnet-5), 17.11.2026, по коду (без живого
запуска): `source.rs`/`mpv.rs`/`events.rs`/`commands.rs`/`player-store.ts`/
`details-store.ts`/`PlayerControlsView.tsx` прочитаны целиком. Корень —
подтверждённый отсутствующий путь `PlayerEvent::EndOfFile(Error)`/`Error(msg)`
→ `start_progress_thread`'s `_ => false` catch-all → ничего не долетает до
фронтенда.

Выполнено — deepseek-v4-pro, 17.11.2026. Сделано:

- `kino-ipc`: добавлен `PlayerErrorDto { message: String }` (camelCase, specta::Type)
  в `dto.rs`, реэкспорт из `lib.rs`, тест `player_error_dto_is_camel_case`,
  `CRATE.md` обновлён.
- `src-tauri/commands.rs::start_progress_thread`: на `PlayerEvent::EndOfFile(Error)`
  и `PlayerEvent::Error(_)` эмитится событие `player-error` с русским текстом
  `PLAYBACK_ERROR_MESSAGE = "Не удалось воспроизвести поток. Попробуйте другую
  озвучку или качество."` (без технических деталей mpv и без утечки URL/токенов —
  техническая причина логируется в `events.rs` у источника). Нормальный
  `EndOfFile(Eof)` ошибкой не считается.
- `ui/src/api.ts`: тип `PlayerErrorDto` (вручную, как `PlayerProgressDto` — событие
  сырое `emit_to`, specta его в `bindings.ts` не генерирует, что подтвердил CI-джоб
  `bindings` — прошёл без diff).
- `ui/src/player-store.ts`: вторая подписка на `player-error` в `init()`
  (отдельный `unlistenError`, закрывается в `dispose()`), выставляет `error` в сторе.
- `PlayerControlsView` рендерит `error` уже как есть — не трогал.
- `ui/src/player-store.test.ts`: мок `listen` переписан на реестр по имени события,
  добавлены тесты подписки на оба события и эмуляции `player-error`.

Нюансы:

- Локально прогнаны только UI-тесты (`pnpm test`, 120 passed) и `pnpm typecheck`
  (чисто); Rust (`kino-ipc`→`kino-player`→`-lmpv`) не компилировался локально —
  только CI.
- `cargo fmt --check` на моих файлах чистый; в выводе fmt есть несвязанные diffs
  в `crates/kino-api/*` — это параллельная незакоммиченная работа другой сессии
  (#030/#032), не моя, в мой коммит не входит.
- Гонка с параллельными сессиями: коммит изначально упал на чужую ветку
  `fix/031-...` (другая сессия переключила ветку), пересажен через `cherry-pick`
  на `fix/029-player-error-event-plumbing` (`32a62f4`), `fix/031` возвращён к
  `a3acb23`. В коммит вошли только 8 файлов этой задачи.

Статус CI: зелёный. `rust` pass (5m20s), `bindings` pass (2m17s), `ui` pass (40s).
Первый прогон `ui` упал по transient infra («The job was not acquired by Runner…
hosted even after multiple attempts» — capacity constraints macOS arm64), не по
коду; перезапущен `gh run rerun --failed`, на повторе зелёный.

PR: https://github.com/saytomorrow-st/kinobox/pull/32 (мерж — за lead-агентом).

Живая проверка (реальный мёртвый стрим → видимое сообщение вместо серого экрана) —
только у заказчика; юнит-тестом эмулируется в `player-store.test.ts`.
