---
id: 003
type: feature
status: review
model: claude-sonnet-5
crate: kino-core
created: 2026-11-13
---

## Задача

`KinoError` и `trait KinoProvider` в `kino-core`: поиск, каталог, детали, список серий/переводов, получение потоков. Это контракт между `kino-api` и `kino-ipc`, поэтому ошибки различают сетевую, парсинговую, «все зеркала недоступны» и «требуется авторизация/челлендж».

## Ограничения

- Async-методы трейта (`async fn` in trait, без `async-trait`, если хватает MSRV).
- Ошибки через `thiserror`, без `anyhow` в публичном API.
- Метод получения потока возвращает `Vec<StreamQuality>` (результат деобфускации), а не сырую строку.
- Файлы: `provider.rs`, `error.rs`.

## Готово когда

- `cargo test -p kino-core` зелёный, есть тест на mock-реализацию трейта
- `CRATE.md` описывает трейт и варианты `KinoError`

## Заметки

Зависит от #002. Любое изменение границ между крейтами согласуется с заказчиком.

2026-11-13: `KinoError`, `CatalogQuery`, `StreamRequest`, `KinoProvider` готовы. 19 тестов `kino-core`, `check`/`fmt`/`clippy`/aarch64 зелёные.
- Зависимости зафиксированы: `thiserror =2.0.21`, `pollster =1.0.1` (dev, без default-features).
- Трейт не object-safe (`impl Future` в возвращаемых типах), поэтому `kino-ipc` использует его как generic-параметр, не `dyn KinoProvider`.
- `rust-version = 1.82` не проверялась на самом 1.82: сборка шла на stable (cargo 1.100.0). Проверить в CI отдельным job при необходимости.
- PR #3 открыт поверх ветки #002 (PR #2). После мержа #1 и #2 сменить базу на `main`.
