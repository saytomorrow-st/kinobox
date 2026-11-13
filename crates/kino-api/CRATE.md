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
таймаут, `Retry-After`, `post_form`, редирект, cookie между запросами, отказ соединения.

## Сборка

`wreq` собирает BoringSSL: нужны `cmake`, `clang` и `go`. Минимальная версия Rust — 1.98 (требование `wreq`).
Universal binary собирается из таргетов `x86_64-apple-darwin` и `aarch64-apple-darwin`.
