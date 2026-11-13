---
id: 005
type: feature
status: open
model: claude-sonnet-5
crate: kino-api
created: 2026-11-13
---

## Задача

`mirrors.rs`: `MirrorPool` с порядком проб из `ARCHITECTURE.md`: активное зеркало, список известных, `https://standby-rezka.tv/`, ручной ввод. Probe с таймаутом 5с, параллельный health-check, пометка мёртвых зеркал, ротация при ошибке.

## Ограничения

- Хранилище активного зеркала скрыто за трейтом (`MirrorStore`) с in-memory реализацией. SQLite-реализация придёт в M2 из `kino-store`, зависимость `kino-api` -> `kino-store` запрещена.
- Трейт `MirrorStore` объявлен в `kino-api`, не в `kino-core`. Если окажется, что он нужен в `kino-core`, сначала спросить заказчика.
- Список известных зеркал лежит константой, редирект standby разбирается по `Location`.
- Тесты на `wiremock`.

## Готово когда

- `cargo test -p kino-api mirrors`: выбор первого живого, fallback на standby, исчерпание списка даёт `KinoError`
- `CRATE.md` обновлён

## Заметки

Зависит от #004. Актуальный список зеркал взять у заказчика или из проверенного источника, не выдумывать.
