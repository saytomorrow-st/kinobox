---
id: 020
type: feature
status: open
model: claude-sonnet-5
crate: kino-ipc
created: 2026-11-15
---

## Задача

Команды каталога в `kino-ipc`: `catalog(kind, page)`, `search(query)`, `details(item)`,
`episodes(details)`. Делегируют уже готовому `KinoProvider` (`HdrezkaApi` через
`crates/kino-api/src/provider_impl.rs`) — новой логики парсинга/сети не пишем, только DTO и
сервисный слой по образцу `mirrors.rs`/`session.rs`.

## Ограничения

- `AppService` должен знать текущий `HdrezkaApi`, построенный на активном зеркале —
  посмотреть, как `session.rs`/`mirrors.rs` сейчас получают/пересобирают клиент при смене зеркала
  (`reset_client`), и как провайдер привязывается к текущему `base` URL зеркала. Если `HdrezkaApi`
  сейчас не хранится в `AppService` как провайдер (а только `HttpClient`), добавить минимально
  необходимую обёртку, не меняя `kino-api`.
- Не менять `kino-core`/`kino-api` без необходимости; если нужен метод, добавить минимально и
  обновить `CRATE.md` соответствующего крейта.
- DTO (`MediaItemDto`, `MediaDetailsDto`, `EpisodeDto`...) — `serde` camelCase + `specta::Type`,
  по образцу `dto.rs`. `MediaDetails::raw_html` никогда не попадает в DTO (уже `#[serde(skip)]` в
  ядре, но явно проверить тестом, что DTO его не содержит никаким путём).
- Ошибки через существующий `IpcError`/`IpcErrorKind`, не придумывать новые варианты без нужды
  (кроме, возможно, `NotFound` если ещё не покрыт — проверить `error.rs`).
- Требуется `AuthRequired`-поведение (details без входа и т.п.) — проверить по `KinoProvider`, какие
  методы это уже возвращают, не перепридумывать.

## Готово когда

- `cargo test -p kino-ipc` зелёный (новые тесты через `wiremock`, без реальной сети)
- `cargo clippy --workspace --all-targets -- -D warnings` зелёный, обе архитектуры собираются
- `kino-ipc/CRATE.md` описывает новые методы и DTO

## Заметки

Зависит от #013 (`KinoProvider::episodes/streams` уже реализованы), #014 (`AppService`/зеркала).
Не зависит от #017/#018/#019 (плеер) — можно делать параллельно с ними.
