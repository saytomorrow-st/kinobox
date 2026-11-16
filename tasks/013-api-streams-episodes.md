---
id: 013
type: feature
status: review
model: deepseek-v4-pro
crate: kino-api
created: 2026-11-14
---

## Задача

`KinoProvider::episodes` и `streams` в `kino-api`: серии и ссылки на потоки через `POST /ajax/get_cdn_series/` и через данные плеера на странице фильма. Результат проходит через `deobfuscate` (#011).

## Ограничения

- Для перевода по умолчанию данные уже лежат на странице фильма: `sof.tv.initCDNMoviesEvents(id, translator_id, is_camrip, is_ads, is_director, host, is_logged, {...})`, внутри JSON с `streams`, `default_quality`, `subtitle`, `subtitle_lns`, `subtitle_def`. На реальной странице (2026-11-14) у вызова два булевых аргумента после хоста. Регулярное выражение из одной из библиотек ожидает один, поэтому JSON-объект разбирать по скобкам, а не по готовому шаблону.
- Для другого перевода и для сериалов: `POST /ajax/get_cdn_series/` с `action=get_movie|get_stream|get_episodes`, `id`, `translator_id`, для сериала ещё `season`, `episode`. Библиотека `ndenissov/HDRezka` шлёт только эти поля, `kristal374/hdrezka-api` добавляет `favs` (значение `input#ctrl_favs` со страницы), `is_camrip`, `is_ads`, `is_director` и `?t=<мс>`. Что требует сайт сейчас, неизвестно. Сначала минимальный набор, при отказе расширенный. Факт записать в `ARCHITECTURE.md`.
- Ответ: `success`, `url`, `quality`, `subtitle`, `subtitle_lns`, `subtitle_def`. Ошибка: `success:false` и `message`.
- Фикстуры обезличить, адреса потоков заменить на `cdn.example.com`.
- Файлы: `provider.rs`, `parse/episodes.rs`, `parse/player.rs`, `examples/cli.rs`, `CRATE.md`.

## Готово когда

- `cargo test -p kino-api` зелёный на фикстурах
- Руками: `cargo run -p kino-api --example cli -- streams <url-фильма>` после входа печатает качества с метками и признаком premium, ссылки в выводе обрезаны до хоста

## Заметки

Зависит от #007, #011, #012. Живую проверку параметров `get_cdn_series` делает заказчик через пример `cli`, пароль вводится в терминале.

2026-11-14: `KinoProvider` реализован для `HdrezkaApi`. 156 тестов `kino-api` (+3 интеграционных), 24 `kino-core`, `fmt`/`clippy`/обе архитектуры зелёные.
- Проверено на РЕАЛЬНОЙ фикстуре фильма: `extract_player_data` (post_id 85038, translator_id 111) и весь путь `streams(&details, None)` до `StreamQuality` (4+ качества, премиум `1080p Ultra`).
- НЕ проверено на реальных страницах: сериалы (`initCDNSeriesEvents`, `action=get_episodes`, `action=get_stream`). Фикстур сериала нет, код написан по ARCHITECTURE.md и покрыт wiremock и синтетикой. После `capture` с реальной страницей сериала формат нужно сверить.
- Публичное API `kino-core`: добавлены `Episode` и `MediaDetails::raw_html` (`#[serde(skip)]`, HTML нужен для потоков фильма), `KinoProvider::episodes`/`streams` переписаны, `StreamRequest` удалён. `CRATE.md` обновлён.
- Перевод берётся первый из списка (`translators[0]`). Выбор другого перевода для фильма (`action=get_movie`) не реализован.
- Пример `play`: `cargo run -p kino-api --example play -- <url страницы>`.
- PR поверх ветки #007 (PR #12).
