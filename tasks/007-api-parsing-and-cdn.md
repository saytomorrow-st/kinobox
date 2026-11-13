---
id: 007
type: feature
status: open
model: deepseek-v4-pro
crate: kino-api
created: 2026-11-13
---

## Задача

`parse/` и вызовы AJAX в `kino-api`: поиск (`/ajax/search/`), каталог и детали (HTML публичных страниц), серии и потоки через `POST /ajax/get_cdn_series/` с `action=get_movie|get_stream|get_episodes`. Склеить всё в реализацию `KinoProvider` поверх `MirrorPool` и клиента.

## Ограничения

- Формат запросов и ответов только из `ARCHITECTURE.md`. Селекторы HTML определять по реальным сохранённым страницам, не угадывать.
- Фикстуры (HTML и JSON) сохраняются из публичных страниц в `crates/kino-api/tests/fixtures/`. Массовая выкачка запрещена: нужно по одной-две страницы на каждый тип.
- Парсер на `scraper`, пустой результат и смена вёрстки дают `KinoError::Parse`, а не панику.
- Файлы: `parse/search.rs`, `parse/catalog.rs`, `parse/details.rs`, `parse/episodes.rs`, `provider.rs` (реализация).

## Готово когда

- `cargo test -p kino-api` зелёный на фикстурах, сеть не используется
- `cargo run -p kino-api --example cli -- search "<запрос>"` печатает результаты (ручная проверка заказчиком)
- `CRATE.md` обновлён

## Заметки

Зависит от #004, #005, #006. Самая крупная задача M1. Если разрастается, разбить на `007a` поиск/каталог и `007b` детали/серии/потоки.
