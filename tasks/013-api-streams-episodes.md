---
id: 013
type: feature
status: open
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
