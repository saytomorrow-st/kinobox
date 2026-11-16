---
id: 020
type: feature
status: review
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

2026-11-15: команды реализованы.
- `AppService` не хранит `HdrezkaApi` в полях: он строится «по требованию» (`provider()` в
  `service.rs`) из текущего `HttpClient` и активного зеркала (`mirror_url()`, с авторезолвом, если
  зеркала ещё нет — тот же путь, что уже использовал `login`). Так зеркало в каждом вызове
  актуальное, даже если `select_mirror`/`add_mirror` поменяли его между вызовами.
- `kino-api`/`kino-core` не менялись.
- `catalog.rs` — все четыре метода делегируют `KinoProvider`. Нюанс: у `HdrezkaApi` свой inherent
  `search(query, page)`, который перекрывает `KinoProvider::search(query)` без пагинации —
  вызывается явно `KinoProvider::search(&provider, &query)`.
- DTO: `MediaKindDto`, `MediaItemDto` (URL как строки — у specta нет типа для `Url`, обратная
  конвертация валидирует и даёт `InvalidInput` на битый адрес), `MediaDetailsDto`, `TranslatorDto`,
  `SeasonDto`, `EpisodeDto`. `raw_html` физически отсутствует как поле у `MediaDetailsDto` —
  отдельный тест проверяет набор ключей сериализованного JSON.
- Ошибки — без новых вариантов `IpcErrorKind`, всё через существующий `From<KinoError>`.
- Тесты через `wiremock`: 44/44 зелёных (`cargo test -p kino-ipc`), включая авторезолв зеркала,
  `NoMirrors`, пустой поиск без сети, `details`/`episodes` делегирование и отказ на битом DTO.
- Проверено лично: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test -p kino-ipc` (44 passed), `cargo check --target aarch64-apple-darwin`/
  `x86_64-apple-darwin` — все зелёные. `kino-player`/`kino-store` unused-dependency warnings в
  clippy предсуществующие (пустые крейты-заглушки), не из этой задачи.
- PR #20, база — `docs/m3-plan`. CI на PR #20 прогнан и проверен лично (`gh pr checks 20`):
  `ui` 22с, `bindings` 4м0с, `rust` 19м17с — все три job зелёные.
