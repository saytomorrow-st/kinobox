---
id: 002
type: feature
status: open
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
