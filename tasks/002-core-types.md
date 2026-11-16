---
id: 002
type: feature
status: review
model: deepseek-flash
crate: kino-core
created: 2026-11-13
---

## Задача

Доменные типы в `kino-core`: `MediaItem`, `MediaKind`, `MediaDetails`, `StreamQuality`, `StreamSource`, `SubtitleTrack`, `UserSession` (без пароля). Все с `serde::{Serialize, Deserialize}`, поля соответствуют структуре из `ARCHITECTURE.md`.

## Ограничения

- Ноль сетевых зависимостей. Только `serde`, `thiserror`, `url`.
- Файлы: `media.rs`, `stream.rs`, `user.rs`, реэкспорт в `lib.rs`.
- Типы пока не знают про specta (это M3, `kino-ipc`).
- В `UserSession` нет поля пароля ни в каком виде.

## Готово когда

- `cargo test -p kino-core`: round-trip serde-тест для каждого типа
- `CRATE.md` перечисляет публичные типы

## Заметки

Зависит от #001.

2026-11-13: типы готовы, 10 serde round-trip тестов, `check`/`test`/`fmt`/`clippy`/aarch64 зелёные.
- Зависимости зафиксированы точными версиями: `serde =1.0.229`, `url =2.5.8`, `serde_json =1.0.154` (только dev).
- `Translator` и `Season` добавлены как вложенные типы `MediaDetails`.
- Домены в тестовых URL (`hdrezka.ag`) условные, на логику не влияют.
- PR #2 открыт поверх ветки #001 (PR #1). После мержа #1 сменить базу на `main`.
