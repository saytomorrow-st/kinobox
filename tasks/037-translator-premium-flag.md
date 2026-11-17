---
id: 037
type: feature
status: done
model: deepseek-v4-pro
crate: kino-core
created: 2026-11-18
---

## Задача

Пользователь (живой тест): платная/premium озвучка вместо реального потока
подгрузила короткий напоминающий ролик HDrezka про покупку премиума.
Пользователь просит — такие озвучки должны быть явно помечены как premium в
UI, чтобы пользователь не пытался их включить без доступа.

## Расследование (сделано, используй как есть)

У `kino_core::media::Translator` (`crates/kino-core/src/media.rs`, строки
~60-63) сейчас только `{id: u64, name: String}` — флага premium нет вообще.
Парсер `crates/kino-api/src/parse/details.rs::parse_translators()` (строки
~210-242), селектор `#translators-list li[data-translator_id]` — извлекает
только `id` и текст названия, не проверяет класс/атрибут на premium-маркер.

В проекте уже есть точный прецедент того же паттерна на уровне качества
потока (не озвучки): `crates/kino-core/src/stream.rs:20` —
`StreamQuality.premium: bool`, парсится в
`crates/kino-api/src/deobfuscate.rs:151` из маркера `pjs-prem-quality` внутри
строки `streams`, прокинут до UI через `StreamQualityDto.premium` в
`ui/src/bindings.ts`. Повтори этот же паттерн для `Translator`.

Референс-библиотека `kristal374/hdrezka-api`
(`HDrezka/movie_player_builder.py::extract_translators`) парсит именно такой
флаг: `premium = any(cls == "b-prem_translator" for cls in item["class"])` —
то есть маркер — CSS-класс `b-prem_translator` на том же `<li>`, что несёт
`data-translator_id`. Это НЕ подтверждено живым HTML текущего зеркала
(`kvk.pub`) — референс-библиотека может быть под другой скин/версию сайта.

## Нужно сделать

1. Добавить `premium: bool` в `kino_core::media::Translator`.
2. В `parse_translators()` проверять CSS-класс `b-prem_translator` на `<li>`
   рядом с `data-translator_id` (аналогично тому, как `deobfuscate.rs`
   проверяет `pjs-prem-quality`) и заполнять `premium`.
3. Прокинуть `premium` до DTO: `TranslatorDto` (или как называется в
   `kino-ipc`) → `ui/src/bindings.ts` (регенерация через CI — локально
   `export_bindings`-тест запускать нельзя, линкует `-lmpv`; если бэкендная
   часть меняет сигнатуру команд/типы, подготовь diff для `bindings.ts`
   вручную по аналогии с тем, как это уже делалось в задаче #033 — читай
   итоговый CI-лог джобы `bindings`, если локальный прогон недоступен, и
   применяй показанный diff вручную).
4. В UI (`ui/src/components/DetailsView.tsx`, блок выбора озвучки) — если
   `translator.premium === true`, показать явный визуальный маркер (бейдж/
   иконка/текст «Premium») на кнопке выбора озвучки. Не обязательно
   блокировать выбор (пользователь может иметь доступ) — только явно
   пометить, чтобы не было сюрприза с напоминающим роликом вместо потока.
5. **Перед кодированием селектора**, если есть возможность — предпочтительно
   через explore-subagent — сверить реальную HTML-разметку хотя бы одной
   известной premium-озвучки на текущем зеркале (`kvk.pub`), чтобы
   подтвердить точное имя CSS-класса. Если живая проверка недоступна в
   этой среде — реализуй по референс-библиотеке, явно пометь в «Заметках»,
   что селектор не подтверждён живым HTML, попроси заказчика подтвердить
   визуально после сборки.

## Ограничения

**ВАЖНО — копия ограничений окружения заказчика из AGENTS.md, обязательна:**
Машина заказчика — слабый Intel Mac (i3-1000G4, 2 ядра, 8 ГБ ОЗУ, macOS
13.7.8). `src-tauri`/`kino-player` линкуют `-lmpv` — локальный
`cargo check`/`test`/`build` для них ЗАПРЕЩЁН; `kino-api`/`kino-core`/
`kino-ipc` — разрешён локальный `cargo check`/`test`. UI: `pnpm typecheck`/
`pnpm lint` — разрешены (не линкуют `-lmpv`). Запрещены `cargo tauri dev`,
собранный `.app`, `cargo run -p kinobox-app`. Запрещены циклы повторных
прогонов «на всякий случай».

- Следуй существующему прецеденту (`StreamQuality.premium`) буквально —
  не выдумывай отдельную новую архитектуру для того же паттерна.
- Если публичный API `kino-core`/`kino-api`/`kino-ipc` меняется — обнови
  `CRATE.md` соответствующих крейтов в том же коммите.
- Обновить все места конструирования `Translator` (тесты/фикстуры) —
  поле `premium` обязательное, не `Option`, значение по умолчанию `false`
  там, где premium не применимо.

## Готово когда

- CI зелёный (`rust`, `ui`, `bindings` джобы) на PR-ветке.
- PR открыт по шаблону, с явной пометкой уровня уверенности в выбранном
  CSS-селекторе (подтверждён живым HTML или только референс-библиотекой).

## Заметки

[Ведёт агент: ход работы, найденные нюансы, blockers]

Готово (2026-11-18). `Translator.premium: bool` (+`#[serde(default)]` для
старого кэша), парсинг `has_class("b-prem_translator")` в `parse_translators()`,
`TranslatorDto.premium`, бейдж «Premium» в DetailsView (цвет `premium:
"#FFD60A"` добавлен в tailwind-тему, выбора не блокирует, есть tooltip).

**Уровень уверенности в селекторе: СРЕДНИЙ-НИЗКИЙ.** Класс
`b-prem_translator` взят только из референс-библиотеки
kristal374/hdrezka-api (`movie_player_builder.py::extract_translators`);
живой разметкой текущего зеркала kvk.pub НЕ подтверждён (живой вход в этой
среде запрещён ограничениями). **Просьба заказчику:** после сборки открыть
фильм с известной платной озвучкой и проверить, что бейдж появляется. Если
нет — прислать фрагмент HTML `<ul id="translators-list">`, поправим селектор.

UI-фикстуры обновлены (`premium: false`), добавлены тесты: парсинг класса
(kino-api), roundtrip premium=true/false (kino-core), camelCase DTO
(kino-ipc), бейдж в DetailsView (ui). bindings.ts обновлён вручную по
паттерну #033 — ожидается прогон CI-джобы `bindings` для сверки.
