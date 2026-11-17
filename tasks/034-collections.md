---
id: 034
type: feature
status: done
model: deepseek-v4-pro
crate: kino-core, kino-api, kino-ipc
created: 2026-11-18
---

## Задача

Редизайн вкладки «Смотреть» (Apple TV-подобный) требует секции «Подборки/Browse
by Collection» — крупные плитки курируемых коллекций (не жанров, не типов
произведений — отдельная сущность HDrezka). Нужно добавить бэкенд-поддержку:
список коллекций + постеры конкретной коллекции. Это НЕ зависит от задачи #033
(сортировка/жанр уже смержена в `main`, но коллекции — отдельный путь URL, не
через `catalog_url()`).

Архитектура подтверждена сверкой с эталонной Python-библиотекой
`kristal374/hdrezka-api` (MIT, `github.com/kristal374/hdrezka-api`, ветка
`master`, файл `HDrezka/movie_collections.py` — прочитан лид-агентом целиком).
Ниже — точная спецификация.

## Подтверждённая механика HDrezka

Коллекция — курируемая подборка с числовым ID в URL:
`{base}/collections/<id>-<slug>/` (slug — SEO-хвост, можно игнорировать при
построении URL, HDrezka резолвит по ID; но для перехода по готовой ссылке со
страницы — slug нужен, см. ниже).

- **Список коллекций**: на главной странице сайта (`{base}/`) есть блок
  `b-collections__newest` с карточками коллекций — тот же контейнер-паттерн
  `b-content__collections_item`, что и у `movie_collections.py`:
  `CollectionBriefInfo{id: int, title: str, url: str}` (минимальный вариант) —
  в полной версии ещё `amount_film: int` (кол-во фильмов) и `img_url: str`
  (обложка плитки). Для наших целей (крупная плитка с заголовком+обложкой)
  нужны: `id`, `title`, `url` (полный, с slug — бери как есть из `href`
  атрибута карточки, не реконструируй), `img_url`, `amount_film`.
- **Постеры внутри коллекции**: `GET {collection_url}/page/N/` (та же
  пагинация `page/N/`, что и у каталога) — страница рендерит карточки в ТОМ ЖЕ
  контейнере `div.b-content__inline_item`, что и обычный каталог — значит
  существующий `parse_cards()` из `kino-api/src/parse/catalog.rs` ПОДХОДИТ
  БЕЗ ИЗМЕНЕНИЙ для парсинга постеров коллекции, нужно только построить
  правильный URL и передать HTML в тот же парсер.
- Сортировка внутри коллекции (`?filter=`) тоже работает (как у каталога) —
  можно опционально поддержать, но для MVP не обязательно (`None` = дефолт).

## Нужно сделать

### 1. `crates/kino-core/src/media.rs` — новый тип

```rust
/// Курируемая коллекция HDrezka (не тип произведения, не жанр — отдельная сущность).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: u64,
    pub title: String,
    pub url: Url,              // полный URL коллекции (как со страницы, с slug)
    pub poster_url: Option<String>,
    pub items_count: Option<u32>,
}
```

(Сверь стиль полей с существующим `MediaItem` в этом же файле — используй те
же трейты/паттерны сериализации, что уже приняты в крейте; `Url` или `String`
для `url`/`poster_url` — смотри как сделано в `MediaItem.page_url`/`poster_url`
и повтори тот же подход для консистентности.)

### 2. `crates/kino-core/src/provider.rs::KinoProvider` — два новых метода трейта

```rust
/// Список курируемых коллекций с главной страницы.
fn collections(&self) -> impl Future<Output = Result<Vec<Collection>>> + Send;

/// Постеры конкретной коллекции, с пагинацией (page с 1).
fn collection_items(&self, collection: &Collection, page: u32) -> impl Future<Output = Result<Vec<MediaItem>>> + Send;
```

Добавь дефолтные тестовые реализации в `MockProvider` (см. существующий
паттерн в `#[cfg(test)]` модуле этого файла) + тесты на вызов этих методов.

### 3. `crates/kino-api` — реализация парсинга

- Новый модуль `crates/kino-api/src/parse/collections.rs` (по образцу
  `parse/catalog.rs`): функция `parse_collections(html: &str, base: &Url) ->
  Result<Vec<Collection>>` — парсит блок `b-collections__newest` /
  `.b-content__collections_item` карточки (селекторы — ПОМЕТЬ флагом
  «НЕ проверены на реальной странице», как уже сделано в `catalog.rs`, это
  принятый в проекте стиль честности насчёт недоказанных селекторов).
  И функция `collection_url(base: &Url, collection: &Collection, page: u32) ->
  Result<Url>` — строит `{collection.url}/page/N/` (N>1 иначе без суффикса,
  как у каталога).
- В `crates/kino-api/src/api.rs::HdrezkaApi` (или аналогичном impl) — добавь
  методы `collections(&self) -> Result<Vec<Collection>>` (фетчит `{base}/`,
  парсит через `parse_collections`) и `collection_items(&self, collection:
  &Collection, page: u32) -> Result<Vec<MediaItem>>` (фетчит `collection_url`,
  парсит через уже существующий `parse_cards()` — ПЕРЕИСПОЛЬЗУЙ его, не
  копируй логику).
- Реализуй эти два метода трейта `KinoProvider` для `HdrezkaApi` в
  `provider_impl.rs` (см. как там уже сделано для `catalog`/`search`/`details`
  — тот же тонкий delegating-паттерн).
- Экспортируй `Collection`, `parse_collections`, `collection_url` через
  `lib.rs`/`parse/mod.rs` по аналогии с существующими экспортами.

### 4. `crates/kino-ipc` — IPC-мост

Новый файл `crates/kino-ipc/src/collections.rs` (по образцу `catalog.rs`):

```rust
impl<S: SecretStore, M: MirrorStore> AppService<S, M> {
    pub async fn collections(&self) -> Result<Vec<CollectionDto>, IpcError> { ... }
    pub async fn collection_items(&self, collection: CollectionDto, page: u32) -> Result<Vec<MediaItemDto>, IpcError> { ... }
}
```

Новый DTO `CollectionDto` в `dto.rs` (camelCase serde, `specta::Type`, по
образцу `MediaItemDto`) с `From`/`Into` для `kino_core::Collection`. Регистрируй
новый модуль в `kino-ipc/src/lib.rs`.

### 5. `src-tauri` — Tauri-команды

Найди файл с существующими командами (`src-tauri/src/commands.rs`, там уже
есть `catalog`/`search`/`details` — grep `#[tauri::command]`). Добавь команды
`collections()` и `collection_items(collection: CollectionDto, page: u32)` по
тому же паттерну (specta-аннотации, регистрация в `tauri::generate_handler!`
или аналогичном месте сборки — найди, где регистрируются существующие команды,
и добавь туда же). НЕ компилируй `src-tauri` локально (линкует `-lmpv`).

### 6. Тесты

- `kino-core`: тест на `MockProvider.collections()`/`collection_items()`.
- `kino-api`: тест на `parse_collections()` с фикстурой HTML (создай минимальную
  инлайн-фикстуру с нужными классами-контейнерами, по образцу существующих
  тестов в `parse/catalog.rs` — там уже есть паттерн инлайн-HTML фикстур для
  `parse_cards`); тест на `collection_url()` построение (N=1 → без `page/`,
  N=2 → с `page/2/`).
- `kino-ipc`: тест делегирования `collections()`/`collection_items()` к
  провайдеру (по образцу `catalog_delegates_to_provider_on_active_mirror`).

## Ограничения

**Копия ограничений окружения заказчика (AGENTS.md), обязательна:**
Машина заказчика — слабый Intel Mac (i3-1000G4, 2 ядра, 8 ГБ ОЗУ, macOS
13.7.8). `kino-core`/`kino-api`/`kino-ipc` — НЕ линкуют `-lmpv`, `cargo check`/
`cargo test` для них локально РАЗРЕШЕНЫ и предпочтительны. `src-tauri`/
`kino-player` линкуют `-lmpv` — локальная компиляция ЗАПРЕЩЕНА (ни `cargo
check`, ни `build`, ни `cargo tauri dev`, ни запуск `.app`/бинаря). Не пытайся
собирать `libmpv`/`mpv` из исходников локально ни при каких обстоятельствах
(`brew install mpv`, `meson`/`ninja`/`cmake` напрямую — запрещены). Финальная
проверка полного workspace, перегенерация `ui/src/bindings.ts` — только через
CI после пуша ветки. Один прогон тестов, зелёный — готово, не гоняй повторно
«на всякий случай».

- Если правишь `src-tauri/src/commands.rs` и команды регистрируются вместе с
  specta-экспортом биндингов — после пуша CI job `bindings` перегенерирует
  `ui/src/bindings.ts`; если он покажет diff (несовпадение) — это ожидаемо и
  нормально доделать вручную по логу CI (см. прецедент из #033: там лид-агент
  вручную применял diff из CI-лога без локальной сборки `src-tauri`).
- Не трогай `ui/` — это отдельные задачи (038/039/040). Твоя задача — только
  бэкенд (`kino-core`/`kino-api`/`kino-ipc`/`src-tauri` команды).
- Публичный API `kino-core`/`kino-api`/`kino-ipc` меняется — обнови
  `CRATE.md` каждого из них в том же коммите (новые типы/методы в таблицах).
- Не выдумывай URL-схему/селекторы заново сверх того, что описано в секции
  «Подтверждённая механика» — при неуверенности в деталях селектора следуй
  принятому в проекте стилю: помечай как «не проверено на реальной странице»
  (см. `catalog.rs` docstring) вместо придумывания точного класса без
  подтверждения.
- Один PR = эта задача. Атомарно.

## Готово когда

- CI (`ci.yml`) зелёный на PR-ветке: rust (все тесты), clippy, fmt,
  cross-check, bindings, ui job.
- `cargo check -p kino-core -p kino-api -p kino-ipc` и `cargo test` для них же
  зелёные локально до пуша.
- PR открыт по шаблону `.github/pull_request_template.md`.

## Заметки

Реализация начата subagent'ом (deepseek-v4-pro), который зависал на upstream-
ошибках сервиса ("The upstream service is temporarily unavailable", 5 попыток
подряд) после того, как основной код уже был написан (uncommitted WIP на
ветке). Лид-агент принял решение не гонять subagent вслепую дальше, а
доделать вручную:

- Запущены локально `cargo fmt`/`check`/`test`/`clippy` для `kino-core`,
  `kino-api`, `kino-ipc` — нашли 2 реальных бага в написанном коде:
  1. `COLLECTION_LINK` селектор был `.b-content__collections_item-link a`
     (вложенный `<a>`), но реальная/тестовая разметка кладёт класс прямо на
     `<a>` — поправлено на `a.b-content__collections_item-link`.
  2. `collection_id_from_url` брал цифры до первого `-`, что ломалось на URL
     вида `/collections/7.html` (без слага) — поправлено: берёт ведущую
     цифровую последовательность до первого нецифрового символа.
- После фикса: `cargo test -p kino-core -p kino-api -p kino-ipc` — всё
  зелёное (включая 3 ранее падавших теста). `cargo clippy --all-targets -D
  warnings` — чисто (только 2 преэкзистентных unused-dependency warning в
  `kino-ipc`, не от этой задачи). `cargo fmt --check` — чисто после разового
  `cargo fmt` (включая `kinobox-app`/`src-tauri`, fmt не требует линковки
  `-lmpv`, компиляция не запускалась).
- Полный ревью диффов по всем изменённым файлам (kino-core/kino-api/kino-ipc/
  src-tauri commands) — реализация соответствует принятым в проекте паттернам
  (DTO camelCase+specta, тонкий delegating-слой в IPC, переиспользование
  `parse_cards` для постеров коллекции, селекторы помечены «не проверено на
  реальной странице»). CRATE.md всех трёх крейтов обновлены корректно.
- `src-tauri` НЕ компилировался локально (запрещено, линкует `-lmpv`) —
  перегенерация `ui/src/bindings.ts` и полная проверка workspace — через CI
  после пуша, по прецеденту из #033 (вручную применить diff из CI-лога, если
  будет расхождение).
