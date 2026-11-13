# kino-api

HTTP-клиент, зеркала, сессия и парсинг HDrezka. Зависит только от `kino-core`.

HTTP-стек — `wreq` (форк-преемник `rquest`, BoringSSL-имперсонация браузера) поверх `tokio`.
Типы `wreq` в публичный API не попадают. Включены фичи `cookies` и `form`.

## HTTP-клиент (`client`)

Реэкспортировано из корня крейта: `HttpClient`, `ClientConfig`, `Response`.

### ClientConfig

| Поле | По умолчанию | Смысл |
|---|---|---|
| `timeout: Duration` | 15 с | лимит на один запрос целиком (соединение, заголовки, тело) |
| `max_retries: u32` | 3 | число повторов после первой попытки (всего до `max_retries + 1` запросов) |
| `base_delay: Duration` | 300 мс | база backoff: `base_delay * 2^попытка` |
| `user_agent: String` | Safari на macOS | значение заголовка `User-Agent` |

### HttpClient

```rust
let client = HttpClient::new(ClientConfig::default())?;
let page = client.get(&url).await?;
let reply = client.post_form(&url, &[("q", "кино")]).await?;
```

- `new(ClientConfig) -> Result<HttpClient>`: ошибка сборки клиента приходит как `KinoError::Network`.
- `get(&Url) -> Result<Response>`
- `post_form(&Url, &[(&str, &str)]) -> Result<Response>`: тело `application/x-www-form-urlencoded`.
- `config() -> &ClientConfig`
- Клиент дёшево клонируется, клоны делят пул соединений и cookie-хранилище.
- Cookie-хранилище включено и общее для всех запросов клиента (для сессии в M2).
- Базовый URL в клиент не зашит: методы принимают полный `url::Url` (его выдаёт `MirrorPool`, #005).

### Response

| Поле / метод | Смысл |
|---|---|
| `status: u16` | статус успешного ответа (< 400) |
| `headers: Vec<(String, String)>` | заголовки, имена в нижнем регистре |
| `body: Vec<u8>` | сырое тело |
| `attempts: u32` | сколько запросов отправлено, включая успешный (минимум 1) |
| `final_url: Url` | адрес, с которого получен ответ, после всех редиректов |
| `text() -> String` | тело как UTF-8 (lossy) |
| `header(name) -> Option<&str>` | заголовок без учёта регистра |

Статусы 4xx и 5xx возвращаются как ошибка, а не как `Response`. Редиректы клиент следует сам (до 10). Встроенный retry `wreq` отключён, повторы делает только `HttpClient`, поэтому `attempts` точен.

### Политика retry

Повторяется только то, у чего `KinoError::is_retryable() == true`:

| Ситуация | Результат | Retry |
|---|---|---|
| сбой транспорта, DNS, соединение, таймаут | `Network(описание)` | да |
| 429 | `Http { status: 429 }` | да |
| 5xx | `Http { status }` | да |
| 404 | `NotFound` | нет |
| прочие 4xx | `Http { status }` | нет |
| защитный челлендж | `Challenge` | нет |

- Задержка: `base_delay * 2^попытка`, не больше 30 с. Половина задержки фиксирована,
  вторая половина случайная (jitter, источник — `std::hash::RandomState`).
- Для 429 с заголовком `Retry-After: <секунды>` ждём указанное время вместо backoff, но не больше 30 с.
  Формат HTTP-даты не поддержан, тогда работает обычный backoff.
- После `max_retries` неудачных повторов возвращается последняя ошибка.

### Правило про Challenge

Ответ со статусом 403 или 503 считается защитным челленджем (Anubis), если в теле или в заголовках
(имя или значение) есть `anubis` или `Making sure you're not a bot` без учёта регистра.
Результат — `KinoError::Challenge`, без retry. Прохождение PoW клиент не реализует: это отдельная
задача, её запуск согласуется с заказчиком. 403 и 503 без маркеров обрабатываются как обычные ошибки.

## Зеркала (`mirrors`)

Реэкспортировано из корня крейта: `MirrorPool`, `MirrorConfig`, `MirrorStatus`, `MirrorStore`,
`MemoryMirrorStore`, `LOGIN_STUB_MARKERS`.

### MirrorConfig

`MirrorConfig::new() -> Result<MirrorConfig>` возвращает значения по умолчанию из таблицы.
`Default` не реализован: разбор адреса standby может вернуть ошибку, паниковать нельзя.

| Поле | По умолчанию | Смысл |
|---|---|---|
| `known: Vec<Url>` | пусто | известные зеркала, пробуются по очереди. Списка рабочих доменов в репозитории нет, его даёт заказчик |
| `standby: Url` | `https://standby-rezka.tv/` | адрес, который должен редиректить на актуальное зеркало |
| `probe_timeout: Duration` | 5 с | лимит на одну пробу |
| `dead_ttl: Duration` | 60 с | сколько зеркало после `mark_dead` пропускается |

### MirrorStore

```rust
pub trait MirrorStore: Send + Sync {
    fn active(&self) -> impl Future<Output = Option<Url>> + Send;
    fn set_active(&self, url: Url) -> impl Future<Output = ()> + Send;
    fn clear_active(&self) -> impl Future<Output = ()> + Send;
}
```

Трейт объявлен в `kino-api`, потому что зависимость `kino-api` -> `kino-store` запрещена.
`MemoryMirrorStore` (`new()`, `with_active(Url)`) держит значение в памяти. SQLite-реализация
придёт из `kino-store` в M2.

### MirrorPool

```rust
let client = HttpClient::new(ClientConfig::default())?;
let pool = MirrorPool::new(&client, MemoryMirrorStore::new(), MirrorConfig::new()?)?;
let base = pool.resolve().await?;
```

- `new(&HttpClient, S, MirrorConfig) -> Result<MirrorPool<S>>`: строит отдельный клиент для проб
  (`max_retries = 0`, `timeout = probe_timeout`, `User-Agent` как у переданного клиента). Cookie проб
  не смешиваются с основным клиентом. Ошибка сборки клиента приходит как `KinoError::Network`.
- `resolve() -> Result<Url>`: идёт по порядку проб, первое живое зеркало делает активным в store.
  Если живых нет, возвращает `KinoError::AllMirrorsUnavailable`.
- `health_check() -> Vec<MirrorStatus>`: параллельно пробует все `known`, порядок результата как в конфиге.
  Активное зеркало и standby не затрагивает, store не меняет.
- `probe_standby() -> MirrorStatus`: проба standby; если редирект есть, `url` в статусе — корень зеркала.
- `add_manual(Url) -> Result<Url>`: проба введённого адреса. Живой становится активным. Недоступный
  возвращает ошибку с причиной (`Http`, `Challenge`, `Network` или `Parse` для заглушки входа),
  активное не меняется.
- `mark_dead(&Url)`: зеркало пропускается `dead_ttl`; если оно было активным, активное сбрасывается,
  и следующий `resolve` ротирует на другое. Пометка живёт в памяти пула.
- `store() -> &S`, `config() -> &MirrorConfig`.

`MirrorStatus { url, alive, reason: Option<String>, latency: Option<Duration> }`: у живого `reason = None`,
`latency` пуста, если ответа не было (сеть, таймаут, челлендж, 4xx/5xx).

### Порядок проб

1. Активное зеркало из store (если не помечено мёртвым).
2. `known` по очереди (уже опробованное активное пропускается).
3. `standby`: GET с редиректами, берётся итоговый URL. Если хост и порт итогового URL совпадают
   с standby, редиректа нет и standby зеркалом **не считается**. Иначе корень итогового URL проверяется
   теми же правилами и становится активным.
4. Ручной ввод: `add_manual`, вызывается отдельно из настроек.

### Что считается живым зеркалом

Статус 2xx, непустое тело и тело **не** содержит ни одного маркера из `LOGIN_STUB_MARKERS`
(`id="loginForm"`, `Вход на HDrezka`). Часть зеркал и сам standby отдают 200 со страницей входа,
поэтому один статус 200 ничего не доказывает. Список маркеров лежит одной константой в начале
`src/mirrors.rs`, обновлять при поломке. Других признаков «настоящей главной» нет.
Статусы 4xx/5xx и `Challenge` (Anubis) делают зеркало недоступным, причина попадает в `reason`.
Проба: один GET главной страницы, без retry.

### Проверка руками

```
cargo run -p kino-api --example mirrors -- [--known URL]... [--manual URL]
```

Печатает `health_check` и результат пробы standby, по строке на адрес:
`OK|DEAD  <url>  <latency ms>  <причина>`. Затем `active: <url>` (код 0) или ошибку в stderr (код 1).
С `--manual` вместо `resolve` вызывается `add_manual`. Неверные аргументы дают код 2.
Без аргументов standby сейчас сообщается как заглушка входа, итог `AllMirrorsUnavailable`.
Каждый адрес запрашивается до двух раз: один раз для вывода статуса, второй раз внутри `resolve`.

## Деобфускация (`deobfuscate`)

Реэкспортирована из корня крейта: `deobfuscate`.

```rust
pub fn deobfuscate(raw: &str) -> kino_core::Result<Vec<StreamQuality>>
```

Чистая синхронная функция без сети. Вход — значение поля `url` из ответа
`get_cdn_series`: либо уже распакованный список `[360p]url,[720p]url`, либо
обфусцированная строка с префиксом `#h`. Возвращает список качеств в том же
порядке, что и во входной строке.

Порядок обработки обфусцированного входа: снять префикс `#h`, склеить части по
разделителю `//_//`, удалить известные мусорные токены, base64-decode
(стандартный алфавит, отсутствие `=`-padding допускается), результат — UTF-8.
Затем разбор `[метка]url`: url длится до первой запятой, за которой сразу `[`
(поэтому запятая внутри url, например в query, не ломает разбор).

Мусорные токены лежат одной константой `JUNK_TOKENS` в начале
`src/deobfuscate.rs` — обновлять при поломке. Любое отклонение от формата
(битый base64, не UTF-8, нет метки, невалидный url, пустой вход) возвращает
`KinoError::Deobfuscation` с коротким описанием причины.

Пример запуска:

```
cargo run -p kino-api --example deobfuscate -- '<строка>'
```

Печатает по строке на качество (`720p  https://...`); при ошибке пишет
`KinoError` в stderr и завершается с кодом 1. Без аргументов запускает
встроенную демонстрацию (собирает обфусцированную строку и деобфусцирует её).

## Проверка руками

```
cargo run -p kino-api --example fetch -- https://example.com
```

Печатает статус, число попыток и первые 200 символов тела. При ошибке пишет `KinoError` в stderr
и завершается с ненулевым кодом (2 — неверные аргументы, 1 — ошибка запроса).

## Тесты

```
cargo test -p kino-api
```

Тесты на `wiremock`, без реальной сети: retry на 503, исчерпание попыток, 404 без retry, Challenge,
таймаут, `Retry-After`, `post_form`, редирект (включая `final_url`), cookie между запросами, отказ соединения.
Для `mirrors`: выбор активного без обхода списка, fallback на `known`, заглушка входа, standby с редиректом и
без, таймаут пробы, Challenge, `mark_dead` с TTL, `add_manual`, порядок `health_check`.

## Сборка

`wreq` собирает BoringSSL: нужны `cmake`, `clang` и `go`. Минимальная версия Rust — 1.98 (требование `wreq`).
Universal binary собирается из таргетов `x86_64-apple-darwin` и `aarch64-apple-darwin`.
