---
id: 008
type: feature
status: open
model: kimi-k3.1-pro
crate: kino-api
created: 2026-11-13
---

## Задача

Закрытие M1: example `cli` в `kino-api` (поиск, детали, потоки), полные `CRATE.md` для `kino-core` и `kino-api`, обновление `README.md` (как запустить), отметка M1 в `ARCHITECTURE.md`.

## Ограничения

- Только документация и example, логику крейтов не менять.
- Команды в README должны реально выполняться.

## Готово когда

- Все команды из README проходят на чистой копии репозитория
- `cargo check --workspace && cargo test --workspace` зелёные

## Заметки

Зависит от #007.
