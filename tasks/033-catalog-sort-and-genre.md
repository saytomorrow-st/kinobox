---
id: 033
type: feature
status: open
model: deepseek-v4-pro
crate: kino-core, kino-api, kino-ipc
created: 2026-11-18
---

## Задача

Новый дизайн вкладки «Смотреть» (Apple TV-подобный) требует «топ чартов» (сортировка
по популярности) и «подборок по категориям» (жанровые ряды). Сейчас `CatalogQuery`
умеет только тип произведения + страницу — нет ни сортировки, ни жанра. Это
расследовано и АРХИТЕКТУРНО РЕШЕНО лид-агентом сверкой с реальным протоколом
HDrezka через эталонную Python-библиотеку `kristal374/hdrezka-api` (MIT,
`github.com/kristal374/hdrezka-api`, ветка `master`). Ниже — точная спецификация,
не нужно ничего придумывать заново, только реализовать.

## Подтверждённая механика URL HDrezka (источник: `core_navigation.py`,
`site_navigation.py` этой библиотеки, прочитаны лид-агентом целиком)

Текущий `catalog_url()` строит `{base}/{kind_section}/{page/N/}` где
`kind_section` ∈ `films|series|cartoons|animation`. Это ПОДТВЕРЖДЕНО корректным
и остаётся без изменений как базовый случай.

Полная схема с жанром и сортировкой:

```
{base}/{kind_section}/{genre_slug}/{page/N/}{query}
```

- `genre_slug` — путь-сегмент вида `action/`, `comedy/`, `drama/` и т.п.
  (пусто, если жанр не выбран). Это СЕГМЕНТ ПУТИ, не query-параметр.
- `page/N/` — только если `N > 1`, иначе пусто (уже так работает в текущем коде,
  сохранить).
- `{query}` — `?filter=<value>`, добавляется ТОЛЬКО если сортировка задана и
  отличается от дефолта сайта (`last`). Если сортировка не задана/дефолт — пусто
  (`?` не добавлять вообще).
- Значения сортировки (`filter=`): `last` (новинки, дефолт сайта), `popular`
  (топ по популярности — это и есть источник данных для «топ чартов»), `soon`
  (скоро выйдет), `watching` (сейчас смотрят).

Примеры:
- Жанр `action`, сортировка `popular`, страница 1: `{base}/films/action/?filter=popular`
- Без жанра, сортировка `popular`, страница 2: `{base}/films/page/2/?filter=popular`
- Без жанра, без сортировки (дефолт), страница 1: `{base}/films/` (текущее поведение, не менять)

## Нужно сделать

### 1. `crates/kino-core/src/media.rs` или новый модуль — новый enum

```rust
/// Сортировка/фильтр каталога (соответствует `filter=` HDrezka).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSort {
    Last,
    Popular,
    Soon,
    Watching,
}
```

`#[serde(rename_all = "snake_case")]` даёт `"last"`/`"popular"`/`"soon"`/`"watching"` —
это ЖЕ значения, что в `filter=`, воспользуйся этим через `impl CatalogSort { fn as_query_value(&self) -> &'static str }`
(или serde-сериализацией в строку — на твоё усмотрение, главное не продублировать
строки руками в двух местах).

### 2. `crates/kino-core/src/provider.rs::CatalogQuery` — два новых поля

```rust
pub struct CatalogQuery {
    pub kind: Option<MediaKind>,
    pub page: u32,
    pub sort: Option<CatalogSort>,   // новое; None = дефолт сайта, в URL не попадает
    pub genre: Option<String>,       // новое; slug (напр. "action"), None = без фильтра
}
```

Обнови ВСЕ существующие конструкции `CatalogQuery { kind, page }` по всему
workspace (grep покажет — `kino-ipc/src/catalog.rs`, тесты в `provider.rs`,
`api.rs`, `kino-ipc/src/tests.rs` и т.д.) — добавь `sort: None, genre: None`
везде, где не требуется иное. Это ломающее изменение публичного API — найди
ВСЕ места компиляцией (`cargo check --workspace` покажет каждое).

### 3. `crates/kino-api/src/parse/catalog.rs::catalog_url()` — новая сигнатура

Сейчас: `catalog_url(base: &Url, kind: Option<MediaKind>, page: u32) -> Result<Url>`.
Расширь до `catalog_url(base: &Url, kind: Option<MediaKind>, genre: Option<&str>, page: u32, sort: Option<CatalogSort>) -> Result<Url>`
(порядок параметров можешь скорректировать под стиль файла, главное — все 5
входов). Собери URL строго по схеме выше. `genre` — произвольная строка-слаг,
НЕ валидируй её против списка (список жанров — забота будущей UI-задачи, бэкенд
просто пробрасывает строку как path-сегмент; но URL-экранируй её корректно,
как уже делается для прочих сегментов в этом файле).

Обнови `crates/kino-api/src/api.rs::catalog()` (строка ~61-62) под новую
сигнатуру — пробрасывай `query.genre.as_deref()` и `query.sort`.

### 4. `crates/kino-ipc/src/catalog.rs::catalog()` — новая сигнатура метода

```rust
pub async fn catalog(
    &self,
    kind: Option<MediaKindDto>,
    page: u32,
    sort: Option<CatalogSortDto>,
    genre: Option<String>,
) -> Result<Vec<MediaItemDto>, IpcError>
```

Новый DTO в `crates/kino-ipc/src/dto.rs`, по образцу существующих (`MirrorStateDto`
и т.п. — camelCase serde, `#[derive(... specta::Type)]`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CatalogSortDto {
    Last,
    Popular,
    Soon,
    Watching,
}
```

С `From`/`Into` в `kino_core::CatalogSort`, как уже сделано для `MediaKindDto`
(найди этот паттерн в `dto.rs`, повтори один в один).

Найди и обнови вызов этого метода в `src-tauri` (команда Tauri, скорее всего
`src-tauri/src/commands.rs` или аналог — grep `fn catalog` по `src-tauri`) — там
тоже новая сигнатура с двумя новыми опциональными параметрами. Специфика Tauri
`#[tauri::command]` + specta-генерация биндингов — найди существующий паттерн
в этом же файле для других команд и повтори. После правки обязательно
перегенерируются TS-биндинги (`ui/src/bindings.ts`) — это часть CI
(`bindings` job), убедись что он зелёный.

### 5. Тесты

- `kino-core`: roundtrip-тест для `CatalogSort`, обновлённый roundtrip для
  `CatalogQuery` с новыми полями (по образцу существующих в `provider.rs`).
- `kino-api`: тест на `catalog_url()` с жанром+сортировкой+страницей — сверь
  итоговую строку URL буквально с примерами из секции «Примеры» выше.
- `kino-ipc`: обнови существующие тесты `catalog_delegates_to_provider_*` в
  `tests.rs` под новую сигнатуру.

## Ограничения

**Копия ограничений окружения заказчика (AGENTS.md), обязательна:**
Машина заказчика — слабый Intel Mac (i3-1000G4, 2 ядра, 8 ГБ ОЗУ, macOS 13.7.8).
`kino-core`/`kino-api`/`kino-ipc` — НЕ линкуют `-lmpv`, `cargo check -p <crate>`/
`cargo test -p <crate>` локально разрешены и предпочтительны для быстрой
итерации. НО если правки затронут `src-tauri` (Tauri-команда) — `cargo check`/
`build` для `src-tauri` ЗАПРЕЩЕНЫ локально (линкует `-lmpv` через `kino-player`
транзитивно), финальная проверка полного workspace — только через CI.
Никогда не запускай `cargo tauri dev`, собранный `.app`, `cargo run -p kinobox-app`.
Не пытайся собрать `libmpv`/`mpv` из исходников локально ни при каких
обстоятельствах.

- Не выдумывай URL-схему заново — она уже подтверждена сверкой с эталонной
  библиотекой, следуй секции «Подтверждённая механика» буквально.
- Публичный API `kino-core` меняется (`CatalogQuery`, новый `CatalogSort`) —
  обнови `crates/kino-core/CRATE.md` в том же коммите (таблица типов провайдера).
  Публичный API `kino-api` (сигнатура `catalog_url`) — тоже обнови
  `crates/kino-api/CRATE.md`, если он там задокументирован.
- Один PR = эта задача. Не трогай UI (`ui/`) — это отдельные задачи (038, 039),
  им нужен только готовый IPC-метод с новой сигнатурой.

## Готово когда

- CI (`ci.yml`) зелёный на PR-ветке: rust (`cargo test --workspace`, включая
  новые тесты), clippy, fmt, cross-check, bindings (сгенерированные
  `ui/src/bindings.ts` не ломают сборку UI), ui job.
- `cargo check -p kino-core -p kino-api -p kino-ipc` зелёный локально (можно
  прогнать самостоятельно, не дожидаясь CI, если они не требуют `src-tauri`).
- PR открыт по шаблону `.github/pull_request_template.md`, лид-агент делает
  финальное ревью перед мержем.

## Заметки

[Ведёт subagent: ход работы, найденные нюансы, blockers]

Subagent (deepseek-v4-pro), 2026-11-18:
- Ветка `feature/033-catalog-sort-genre`.
- Добавлен `kino_core::CatalogSort` (`Last/Popular/Soon/Watching`, serde snake_case,
  метод `as_query_value()`) в `crates/kino-core/src/provider.rs`; `CatalogQuery` расширен
  полями `sort: Option<CatalogSort>` и `genre: Option<String>`.
- `catalog_url()` в `crates/kino-api/src/parse/catalog.rs` получил сигнатуру
  `(base, kind, genre: Option<&str>, page, sort: Option<CatalogSort>)`; собирает
  `{base}/{kind}/{genre}/{page/N/}{?filter=}` строго по подтверждённой схеме. `filter=`
  добавляется только если sort задан и != `Last`; genre percent-кодируется как единый
  path-сегмент. `api.rs::catalog()` пробрасывает `query.genre.as_deref()` и `query.sort`.
- `kino-ipc`: новый DTO `CatalogSortDto` (camelCase, specta::Type) + `From`/`Into` в
  `kino_core::CatalogSort` по образцу `MediaKindDto`; метод `AppService::catalog` получил
  `(kind, page, sort, genre)`. Реэкспорт `CatalogSortDto` из `kino_ipc`.
- `src-tauri/src/commands.rs`: команда `catalog` дополнена `sort`/`genre` (паттерн
  существующих команд соблюдён, specta-биндинги перегенерируются в CI). НЕ компилировал
  `kinobox-app` локально (линкует -lmpv) — проверка через CI.
- Обновлены `crates/kino-core/CRATE.md` и `crates/kino-api/CRATE.md` (публичный API).
- Обновлены все конструкции `CatalogQuery {..}` и вызовы `catalog_url`/`catalog` по
  workspace (provider.rs-тесты, api.rs-тесты, kino-ipc tests.rs, examples/cli.rs,
  examples/capture.rs).
- Тесты: `catalog_sort_roundtrip_and_snake_case`, расширенный `catalog_query_roundtrip`
  (kino-core); `catalog_url_with_genre_sort_and_page`, `catalog_url_percent_encodes_genre_slug`,
  обновлённый `catalog_url_paths` (kino-api); `catalog_delegates_genre_and_sort_to_provider`,
  обновлённые `catalog_delegates_*` (kino-ipc).
- Локально: `cargo check`/`clippy`/`fmt` зелёные для kino-core/kino-api/kino-ipc;
  `cargo test -p kino-core -p kino-api -p kino-ipc` — 25 + 160(+3) + 52 тестов зелёные.
- Полная проверка workspace (src-tauri, specta-биндинги `ui/src/bindings.ts`, universal
  binary) — только через CI после пуша.
