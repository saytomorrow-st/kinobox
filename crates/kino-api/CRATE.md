# kino-api

HTTP-клиент, зеркала, сессия и парсинг HDrezka. Зависит только от `kino-core`.

HTTP-стек — `wreq` (форк-преемник `rquest`, BoringSSL-имперсонация браузера) поверх `tokio`.
Типы `wreq` в публичный API не попадают. Включены фичи `cookies`, `form` и `gzip`.

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
- `post_form_with_headers(&Url, form, headers: &[(&str, &str)]) -> Result<Response>`: то же с дополнительными заголовками запроса (имя, значение). Retry и определение `Challenge` те же.
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

Реальный Anubis (проверено вручную 2026-11-14 на `rezka.ag`, `hdrezka-home.tv`) отвечает статусом
**200 OK**, ставит куки `techaro.lol-anubis-auth-<hex>` и `techaro.lol-anubis-cookie-verification-<hex>`
и отдаёт HTML с `<title>Проверяем, что вы не бот!</title>` (в английской версии
`Making sure you're not a bot!`). Поэтому проверка идёт в `send_once` при **любом** статусе, в том числе 2xx,
до возврата `Ok`. Признак челленджа (достаточно любого):

1. В заголовках `Set-Cookie` имя куки (до первого `=`) начинается с `techaro.lol-anubis`, без учёта регистра.
   Заголовков `Set-Cookie` может быть несколько.
2. Внутри элемента `<title>` (в первых 64 КиБ тела) есть `Проверяем, что вы не бот` или
   `Making sure you're not a bot` (в том числе с типографским апострофом), без учёта регистра.
   Остальное тело не просматривается.

Дополнительно для статусов 403 и 503 сохранён прежний широкий поиск: в теле или в заголовках
(имя или значение) есть `anubis` или `Making sure you're not a bot` без учёта регистра. При остальных
статусах слово `anubis` в теле **не** ищется: оно может встретиться на обычной странице (название фильма).
403 и 503 без маркеров обрабатываются как обычные ошибки. Страница входа (`<title>Вход на HDrezka</title>`)
челленджем не считается.

Результат — `KinoError::Challenge`, без retry. Тело перед проверкой распаковано: у `wreq` включена
фича `gzip`, без неё сжатое тело Anubis (`Content-Encoding: gzip`) осталось бы нечитаемым для детектора.
Прохождение PoW клиент не реализует, это вне M1: отдельная задача, её запуск согласуется с заказчиком.
Проверка руками: `cargo run -p kino-api --example fetch -- https://rezka.ag/` печатает ошибку `Challenge`
и завершается с кодом 1 (результат зависит от IP).

## Зеркала (`mirrors`)

Реэкспортировано из корня крейта: `MirrorPool`, `MirrorConfig`, `MirrorState`, `MirrorStatus`,
`ResolvedMirror`, `MirrorStore`, `MemoryMirrorStore`, `LOGIN_STUB_MARKERS`.

### Состояния зеркала

`MirrorState { Open, NeedsLogin, Challenge, Dead }` (`Debug, Clone, Copy, PartialEq, Eq`).
Определяется одним GET главной страницы, без retry. `is_usable()` истинно для `Open` и `NeedsLogin`.

| Состояние | Признак | Что делает пул |
|---|---|---|
| `Open` | 2xx, непустое тело, нет маркеров из `LOGIN_STUB_MARKERS` | использует |
| `NeedsLogin` | 2xx и есть маркер из `LOGIN_STUB_MARKERS` (`id="loginForm"`, `Вход на HDrezka`) | использует, перед работой нужен вход |
| `Challenge` | `HttpClient` вернул `Err(KinoError::Challenge)` (см. «Правило про Challenge») | пропускает, PoW не реализован |
| `Dead` | сеть, DNS, таймаут, не-2xx, пустое тело | пропускает |

`NeedsLogin` считается живым зеркалом: приложение работает только со входом, и часть зеркал отдаёт форму
входа вместо каталога. Куки привязаны к домену, поэтому `resolve` возвращает состояние вместе с адресом.
Один статус 200 ничего не доказывает: форму входа и Anubis отдают тоже с 200. Список маркеров лежит
константой в начале `src/mirrors.rs`, обновлять при поломке. Других признаков «настоящей главной» нет.
Челлендж определяет только `HttpClient`, `mirrors` его не дублирует.

### MirrorConfig

`MirrorConfig::new() -> Result<MirrorConfig>` возвращает значения по умолчанию из таблицы.
`Default` не реализован: разбор адресов может вернуть ошибку, паниковать нельзя.

| Поле | По умолчанию | Смысл |
|---|---|---|
| `known: Vec<Url>` | `https://kvk.pub/`, `https://rezka.fi/` (в этом порядке) | известные зеркала, пробуются по очереди |
| `standby: Url` | `https://standby-rezka.tv/` | адрес, который должен редиректить на актуальное зеркало |
| `probe_timeout: Duration` | 5 с | лимит на одну пробу |
| `dead_ttl: Duration` | 60 с | сколько зеркало после `mark_dead` пропускается |

В списке по умолчанию только два домена: их хост проверил заказчик. `rezka.si` отдаёт ту же страницу,
но заказчик не подтвердил, что домен официальный, поэтому его нет в списке. Добавлять другие домены
только после подтверждения заказчика.

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
придёт из `kino-store` в M2. Store хранит только адрес, состояние не хранится: оно приходит из пробы.

### Результаты

```rust
pub struct MirrorStatus {
    pub url: Url,
    pub state: MirrorState,
    pub reason: Option<String>,
    pub latency: Option<Duration>,
}
impl MirrorStatus { pub fn is_usable(&self) -> bool }

pub struct ResolvedMirror {
    pub url: Url,
    pub state: MirrorState, // только Open или NeedsLogin
}
```

`reason`: у `Open` `None`; у `NeedsLogin` «требуется вход»; у `Challenge` и `Dead` причина (`HTTP 503`,
таймаут, пустой ответ и т. п.). `latency` пуста, если ответа не было (сеть, таймаут, `Challenge`, 4xx/5xx
из-за ошибки клиента).

### MirrorPool

```rust
let client = HttpClient::new(ClientConfig::default())?;
let pool = MirrorPool::new(&client, MemoryMirrorStore::new(), MirrorConfig::new()?)?;
let mirror = pool.resolve().await?;
if mirror.state == MirrorState::NeedsLogin {
    // сначала вход на mirror.url
}
```

- `new(&HttpClient, S, MirrorConfig) -> Result<MirrorPool<S>>`: строит отдельный клиент для проб
  (`max_retries = 0`, `timeout = probe_timeout`, `User-Agent` как у переданного клиента). Cookie проб
  не смешиваются с основным клиентом. Ошибка сборки клиента приходит как `KinoError::Network`.
- `resolve() -> Result<ResolvedMirror>`: идёт по порядку проб, первое пригодное (`Open` или `NeedsLogin`)
  делает активным в store. `Challenge` и `Dead` пропускаются. Если пригодных нет,
  возвращает `KinoError::AllMirrorsUnavailable`.
- `health_check() -> Vec<MirrorStatus>`: параллельно пробует все `known`, порядок результата как в конфиге.
  Активное зеркало и standby не затрагивает, store не меняет.
- `probe_standby() -> MirrorStatus`: проба standby; если редирект есть, `url` в статусе — корень зеркала.
- `add_manual(Url) -> Result<ResolvedMirror>`: проба введённого адреса. Пригодный (`Open` или `NeedsLogin`)
  становится активным. `Challenge` возвращает `Err(KinoError::Challenge)`, прочее непригодное
  возвращает ошибку пробы (`Http`, `Network`, `Parse` для пустого ответа). Активное при ошибке не меняется.
- `mark_dead(&Url)`: зеркало пропускается `dead_ttl`; если оно было активным, активное сбрасывается,
  и следующий `resolve` ротирует на другое. Пометка живёт в памяти пула.
- `store() -> &S`, `config() -> &MirrorConfig`.

### Порядок проб

1. Активное зеркало из store (если не помечено мёртвым).
2. `known` по очереди (уже опробованное активное пропускается).
3. `standby`: GET с редиректами, берётся итоговый URL. Если хост и порт итогового URL совпадают
   с standby, редиректа нет и standby зеркалом **не считается**. Иначе корень итогового URL проверяется
   теми же правилами и, если пригоден, становится активным.
4. Ручной ввод: `add_manual`, вызывается отдельно из настроек.

Приоритет задаёт порядок списка, а не «лучшее» состояние: `Open`-зеркало после `NeedsLogin`-зеркала в
`known` не выбирается, пока `NeedsLogin` отвечает. Порядок `known` определяет пользователь, пул его
не переставляет.

### Standby

Сейчас `standby-rezka.tv` отдаёт форму входа без редиректа. Принимает ли он вход напрямую, не проверено
(выяснится в #012), поэтому без редиректа он **не зеркало**. `probe_standby` в этом случае возвращает
`state = NeedsLogin` (так выглядит его ответ) и `reason = "standby без редиректа на зеркало"`, а `resolve`
его не использует. Для статусов без редиректа с другим состоянием (`Dead`, `Challenge`) к причине
добавляется исходная в скобках.

### Проверка руками

```
cargo run -p kino-api --example mirrors -- [--known URL]... [--manual URL]
```

Печатает `health_check` и результат пробы standby, по строке на адрес:
`OPEN|LOGIN|CHALLENGE|DEAD  <url>  <latency ms>  <причина>`. Затем `active: <url> (open|login required)`
(код 0) или ошибку в stderr (код 1). `--known` можно повторять; если задан, список заменяет
значение по умолчанию. С `--manual` вместо `resolve` вызывается `add_manual`. Неверные аргументы дают код 2.
Каждый адрес запрашивается до двух раз: один раз для вывода статуса, второй раз внутри `resolve`.
Результат зависит от IP и состояния сайта.

## Сессия и вход (`session`)

Реэкспортировано из корня крейта: `Password`, `Credentials`, `SecretStore`, `KeychainSecretStore`,
`MemorySecretStore`, `login`, `ensure_logged_in`, `login_and_remember`.

```rust
pub struct Password(/* приватно */);          // Debug = "Password(***)", без Display и Serialize
pub struct Credentials { pub email: String, pub password: Password }

pub trait SecretStore: Send + Sync {
    fn load(&self)  -> impl Future<Output = Result<Option<Credentials>>> + Send;
    fn save(&self, credentials: &Credentials) -> impl Future<Output = Result<()>> + Send;
    fn clear(&self) -> impl Future<Output = Result<()>> + Send;
}

pub async fn login(client: &HttpClient, mirror: &Url, credentials: &Credentials) -> Result<UserSession>;
pub async fn ensure_logged_in<S: SecretStore>(client: &HttpClient, mirror: &Url, store: &S) -> Result<UserSession>;
pub async fn login_and_remember<S: SecretStore>(client: &HttpClient, mirror: &Url, store: &S, credentials: &Credentials) -> Result<UserSession>;
```

`Password::new(impl Into<String>)`, `expose() -> &str` (только для отправки на сайт и записи в Keychain).

### Поток входа (`login`)

1. `POST {корень зеркала}/ajax/login/`, `application/x-www-form-urlencoded`: `login_name` (email), `login_password`,
   `login_not_save=0`. Заголовки `X-Requested-With: XMLHttpRequest`, `Referer: {корень зеркала}`. Путь, запрос и
   фрагмент переданного адреса отбрасываются.
2. Разбор JSON. Читаются только `success`, `message`, `logged_email`:
   - `success: true`: дальше;
   - `success: false` и `logged_email` (непустая строка): `LoginRejected("уже выполнен вход под другим аккаунтом: <email>")`;
   - `success: false` и `message`-строка: `LoginRejected(<текст без HTML>)`; пустой текст даёт «причина не указана сайтом»;
   - `success: false` и `message`-объект: значения ключей `email`, `password`, `login_name`, `login_password` (в таком порядке)
     через `; `. Если ни одного известного ключа нет, `Parse`;
   - не JSON, пустое тело, не объект, нет `success` или он не булев, `success: false` без `message` и `logged_email`: `Parse`.
3. `user_id` из `Set-Cookie: dle_user_id=<число>` ответа (при повторах берётся последняя). Нет куки или не число: `Parse`.
4. `GET` главной. Если в теле есть `id="loginForm"`, вернётся `LoginRejected("вход не засчитан: ...")`.
5. Ник: `<input ... id="name" ... value="...">` на главной; если поля нет или оно пустое, часть email до `@`
   (при пустой части `user`). Что поле `input#name` присутствует на главной, не проверено: видели его только на странице фильма.
6. Результат `UserSession { user_id, username }`.

`Challenge` на любом шаге приходит как `Err(KinoError::Challenge)`. Куки (`dle_user_id`, `dle_password`, `PHPSESSID`)
остаются в cookie-хранилище `HttpClient` в памяти. При смене зеркала или перезапуске вход повторяется сохранёнными данными.

Очистка причины отказа: теги заменяются пробелом, сущности (`&amp;`, `&lt;`, `&#33;` и т. п.) раскрываются, пробелы схлопываются.
Пароль из запроса вырезается из текста до и после очистки (замена на `***`), поэтому сервер, «эхом» вернувший пароль, его не раскроет.

### Высокоуровневые функции

- `ensure_logged_in`: `store.load()`; нет данных: `Err(KinoError::AuthRequired)`; иначе `login`. Ничего не сохраняет.
- `login_and_remember`: `login`, и только после успеха `store.save()`. Отклонённый, неверный или не засчитанный
  пароль в хранилище не попадает.

### Что хранится и что нет

- Keychain: сервис `kinobox`, одна запись (аккаунт `hdrezka-credentials`) с JSON `{"email", "password"}`.
- Куки в Keychain **не** пишутся.
- `UserSession` пароля не содержит.
- Блокирующий API `keyring` вызывается через `tokio::task::spawn_blocking`.
- Ошибки Keychain приходят как `KinoError::Keychain(описание)`; описание берётся из `keyring::Error`, пароль в него не попадает.
  Повреждённая запись даёт `Keychain("запись с учётными данными повреждена")`. Отсутствие записи не ошибка: `load` возвращает `None`, `clear` успешен.
- `MemorySecretStore` для тестов и примеров без Keychain.

### Правила безопасности

- Пароль только в `Password`: `Debug` печатает `Password(***)`, `Display` и `Serialize` не реализованы.
- Пароль не попадает в логи, тексты ошибок, `UserSession`, аргументы командной строки и переменные окружения.
- Пример читает пароль только через `rpassword::prompt_password` (скрытый ввод в терминале).
- Тесты и CI используют `MemorySecretStore` и `wiremock`, реальный Keychain и реальный сайт не трогаются.

### Зависимости

`keyring =4.2.0` (по умолчанию фича `v1`: на macOS подтягивает `apple-native-keyring-store` с Keychain Services, API `keyring::Entry`),
`rpassword =7.5.5` (только `[dev-dependencies]`, для примера), `serde_json` (разбор ответа входа и запись в Keychain).
Оба таргета `x86_64-apple-darwin` и `aarch64-apple-darwin` собираются (`cargo check --workspace --target ...`).

### Проверка руками

```
cargo run -p kino-api --example login -- [--mirror URL] [--email EMAIL] [--logout]
```

- Без `--mirror` зеркало выбирает `MirrorPool` (список по умолчанию), печатается `mirror: <url> (open|login required)`.
- Если в Keychain есть данные и `--email` не задан, берутся они. Если `--email` задан или данных нет: email спрашивается
  (когда не задан), пароль спрашивается скрыто.
- Успех: данные сохраняются в Keychain, печатается `logged in as <ник> (id N)`.
- Ошибка: текст в stderr (без пароля), код выхода 1. Челлендж и отсутствие зеркал дают отдельные сообщения.
- `--logout` удаляет запись из Keychain и печатает `credentials removed`.
- Неверные аргументы: справка, код 2. `--help`: справка, код 0.

## Парсинг и API (`parse`, `api`, `sanitize`)

Зависимость — `scraper = "=0.29.0"` (зафиксирована в `[workspace.dependencies]`,
`default-features = false`, чтобы не тянуть демо-CLI `getopts`). Парсинг чистых
функций без сети; сеть — только в `HdrezkaApi`.

Реэкспортировано из корня крейта: `HdrezkaApi`, `parse_cards`, `parse_details`,
`parse_last_page`, `catalog_url`, `search_url`, `sanitize_page`, `contains_email`,
`extract_player_data`, `parse_episodes_html`, `PlayerData`.

```rust
pub fn parse_cards(html: &str, base: &Url) -> Result<Vec<MediaItem>>;
pub fn parse_details(html: &str, page_url: &Url) -> Result<MediaDetails>;
pub fn parse_last_page(html: &str) -> Option<u32>;
pub fn catalog_url(base: &Url, kind: Option<MediaKind>, page: u32) -> Result<Url>;
pub fn search_url(base: &Url, query: &str, page: u32) -> Result<Url>;
pub fn sanitize_page(html: &str) -> String;
pub fn contains_email(html: &str) -> bool;
```

`HdrezkaApi` — обёртка над `HttpClient` и корнем зеркала (inherent-методы, не трейт):

```rust
pub struct HdrezkaApi { /* клиент + корень зеркала */ }
impl HdrezkaApi {
    pub fn new(client: HttpClient, mirror: Url) -> Self;          // mirror приводится к `/`
    pub fn base(&self) -> &Url;
    pub async fn search(&self, query: &str, page: u32) -> Result<Vec<MediaItem>>;
    pub async fn catalog(&self, query: CatalogQuery) -> Result<Vec<MediaItem>>;
    pub async fn details(&self, item: &MediaItem) -> Result<MediaDetails>;
    pub async fn details_at(&self, page_url: &Url) -> Result<MediaDetails>; // детали по адресу
    pub async fn episodes_list(&self, item: &MediaItem, translator_id: u32)
        -> Result<HashMap<u32, Vec<Episode>>>;   // action=get_episodes
    pub async fn stream_url(&self, post_id: u32, translator_id: u32,
        season: Option<u32>, episode: Option<u32>)
        -> Result<Vec<StreamQuality>>;           // action=get_stream, после деобфускации
}
```

### Что проверено на реальной странице

Парсер деталей (`parse_details`) проверен на реальной обезличенной странице фильма
`tests/fixtures/film_85038.html` (kvk.pub, после входа, 2026-11-14). Подтверждённые
селекторы: `input#post_id`, `meta[property="og:url"]`, `meta[property="og:type"]`,
`.b-post__title h1`, `.b-post__origtitle`, `.b-post__description_text`,
таблица `.b-post__info` (строки `tr`, в первой ячейке `h2`-заголовок поля),
`#translators-list li[data-translator_id]`, рейтинг `.b-post__rating span.num`,
постер `img[itemprop="image"]`.

Расхождения с исходным списком, выясненные по фикстуре (прав реальный HTML):

- Года в виде отдельной строки «Год» нет; год берётся из строки **«Дата выхода»**
  (`6 июля 2026 года` → `2026`), с запасным вариантом «Год».
- Рейтинг HDrezka лежит в `span.num` внутри `.b-post__rating` и равен `7.15`
  (это не IMDb/Кинопоиск из строки «Рейтинги»).
- Длительность — строка «Время» (`172 мин.` → `172`).
- Стран/жанров в фикстуре по две/четыре строки; берутся тексты ссылок внутри
  второй ячейки строки.

### Что НЕ проверено на реальной странице

Каталог и поиск (селекторы `div.b-content__inline_item` и вложенные) не проверены
на живых страницах. Источники: исходники двух открытых библиотек (`ndenissov/HDRezka`,
`kristal374/hdrezka-api`, обе MIT) и просмотр главной kvk.pub 2026-11-14 (на главной
подтверждён только контейнер карточки с `data-id` и `data-url`). Пути разделов
`series`/`animation` и разметка сезонов/серий (`li.b-simple_season__item[data-tab_id]`,
`li.b-simple_episode__item[data-season_id][data-episode_id]`) — предположение по
исходникам, проверяется только синтетическим фрагментом.

### Как получить реальные фикстуры

```
cargo run -p kino-api --example capture -- --out <каталог>
```

После входа (`cargo run -p kino-api --example login`) делает ровно по одному GET:
главная, каталог фильмов (`/films/`), поиск `the`, одна страница фильма и одна
страница сериала (адреса — первая карточка `Movie` и первая `Series` из
распарсованного каталога; если сериала нет — пропускается с сообщением). Каждая
страница обезличивается `sanitize_page` (скрытые поля `member_user_id`,
`member_user_token`, `ctrl_token_id`, `ctrl_favs`, `dle_login_hash`, `profileId`,
`email`, ник в `name`-поле комментариев, `geo_ip`, `"streams":"…"` и `thumbnails`)
и проверяется, что сырой почты не осталось; при неудаче файл не пишется.

### `KinoProvider`

Трейт `kino_core::KinoProvider` реализован в `src/provider_impl.rs` для
`HdrezkaApi` (#013). `search`/`catalog`/`details` делегируют одноимённым
inherent-методам, `episodes` и `streams` — данным плеера и AJAX (см. ниже).

### CDN и потоки (`cdn`, `api`, `provider_impl`)

Чистые парсеры — в `src/cdn.rs`, сеть — в `HdrezkaApi::episodes_list` и
`stream_url`, реализация трейта — в `src/provider_impl.rs`.

#### Откуда берутся поля плеера

На странице произведения скрипт содержит вызов
`sof.tv.initCDNMoviesEvents(post_id, translator_id, …, {…})` (для сериалов
`initCDNSeriesEvents`). Последним аргументом — JSON-объект с полем `streams`
(строка потоков, обфусцирована) и `thumbnails` (превью, пока игнорируется).
`extract_player_data` ищет первый такой вызов, берёт первые два числовых
аргумента и извлекает JSON-объект по сбалансированным скобкам (JSON в поле
`streams` содержит запятые между качествами, а поле `midroll` — фигурные скобки
внутри строки, поэтому разбор идёт по скобкам, а не по готовому шаблону).

```rust
pub struct PlayerData { pub post_id: u32, pub translator_id: u32, pub streams: String }
pub fn extract_player_data(html: &str) -> Result<PlayerData>;
pub fn parse_episodes_html(html: &str, season_id: u32) -> Result<Vec<Episode>>;
```

Проверено на реальной обезличенной странице фильма `tests/fixtures/film_85038.html`
(kvk.pub, 2026-11-14): `post_id = 85038`, `translator_id = 111`, `streams`
начинается с `[360p]`. Разметка `initCDNSeriesEvents` на реальной странице
сериала не проверялась — только синтетический фрагмент в тестах.

#### AJAX `/ajax/get_cdn_series/`

`POST /ajax/get_cdn_series/` с полями `id`, `translator_id`, `action`. Успех:
`{"success":true,…}`, ошибка: `{"success":false,"message":"…"}`. Статус 200 с
`success:false` — это не сетевая ошибка, а `KinoError::Parse` (текст из `message`).
Нет `success` вообще или неожиданная структура — тоже `Parse`.

- `action=get_episodes` → `{"success":true,"episodes":{"1":"<ul><li>1 серия</li>…</ul>",…}}`.
  `episodes_list` разбирает строку каждого сезона через `parse_episodes_html` и
  возвращает `HashMap<u32, Vec<Episode>>`; ключ — номер сезона, `episode_id` —
  порядковый номер `<li>`, `title` — текст `<li>`. Пустой `<ul>` или его
  отсутствие — `Ok(vec![])`.
- `action=get_stream` → `{"success":true,"url":"#h…"}`. `stream_url` добавляет
  `season`/`episode` в форму, только если заданы, и деобфусцирует `url`.

#### Провайдер

- `episodes(&MediaDetails)`: для фильма (`seasons` пусто) — `Ok(vec![])`; иначе
  берёт перевод по умолчанию (`details.translators[0]`), вызывает `episodes_list`,
  складывает серии всех сезонов и сортирует по `(season_id, episode_id)`.
- `streams(&MediaDetails, Option<&Episode>)`: для фильма (`episode = None`)
  извлекает `PlayerData` из `MediaDetails::raw_html` и деобфусцирует `streams`
  без AJAX — данные перевода по умолчанию уже лежат на странице. Для сериала —
  `stream_url` с `post_id`, `translator_id` и `(season_id, episode_id)` серии.

Для фильма можно обойтись без AJAX, только если `"streams"` на странице непустой;
другой перевод для фильма через AJAX в #013 не реализован (нужен `action=get_movie`
с другим `translator_id`).

Пример запуска (реальные запросы; для сериала без `--season`/`--episode`
печатается список серий и код 2):

```
cargo run -p kino-api --example play -- <url страницы> [--translator ID] [--quality 1080p|720p|…] [--season N --episode M] [--mirror URL]
```

## Деобфускация (`deobfuscate`)

Реэкспортирована из корня крейта: `deobfuscate`.

```rust
pub fn deobfuscate(raw: &str) -> kino_core::Result<Vec<StreamQuality>>
```

Чистая синхронная функция без сети. Вход — значение поля `url` из ответа
`get_cdn_series`: либо уже распакованный список качеств, либо обфусцированная
строка с префиксом `#h`. Возвращает список качеств в том же порядке, что и во
входной строке.

Порядок обработки обфусцированного входа: снять префикс `#h`, склеить части по
разделителю `//_//`, удалить известные мусорные токены, base64-decode
(стандартный алфавит, отсутствие `=`-padding допускается), результат — UTF-8.
Затем разбор списка качеств. Без `#h` строка разбирается напрямую.

### Формат списка качеств

Качество записано как `[метка]адрес or адрес …`. Адреса внутри качества
разделены точной подстрокой ` or `. Качества разделены запятой, за которой
сразу начинается следующая метка `[` (поэтому запятая внутри url, например в
query, не ломает разбор).

Метка может содержать HTML — премиум-качество выглядит как
`[<span class="pjs-prem-quality">1080p Ultra<img …></span>]`. Граница метки:

- если метка начинается с `<`, закрывающая `]` ищется после закрывающего
  `</span>`;
- иначе — первая `]`, стоящая вне HTML-тега (не внутри `<…>`).

Разбор идёт простым конечным автоматом по символам, без регулярных выражений
и без новых зависимостей; границы всегда по UTF-8.

Из метки вырезаются HTML-теги (всё между `<` и `>`) и убираются пробелы по
краям. Пустая метка после очистки — ошибка. `premium = true`, только если сырая
метка содержит класс `pjs-prem-quality`.

Адрес, оканчивающийся на `:hls:manifest.m3u8`, — HLS, остальные — прямые mp4.
`url` — первый прямой mp4; `fallbacks` — остальные прямые (порядок сохраняется);
`hls` — первый HLS (остальные HLS игнорируются, но не падают). Если прямых
адресов нет, а HLS есть, `url` выводится из HLS-адреса срезом суффикса
`:hls:manifest.m3u8` (допущение: так делает одна из неофициальных библиотек;
реального прямого адреса сайт может не отдавать). Нет ни одного адреса — ошибка.

Мусорные токены лежат одной константой `JUNK_TOKENS` в начале
`src/deobfuscate.rs` — обновлять при поломке. Любое отклонение от формата
(битый base64, не UTF-8, нет метки, нет адресов, невалидный url, пустой вход)
возвращает `KinoError::Deobfuscation` с коротким описанием причины.

Пример запуска:

```
cargo run -p kino-api --example deobfuscate -- '<строка>'
```

Печатает по строке на качество (`<label>[ premium]  mp4=<хост>  hls=<хост|-> fallbacks=<n>`),
в выводе только хосты, не полные ссылки (в реальных ссылках подписи). При ошибке
пишет `KinoError` в stderr и завершается с кодом 1. Без аргументов запускает
встроенную демонстрацию (новый формат и обфусцированный вариант).

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
таймаут, `Retry-After`, `post_form`, Challenge при 200 (кука `techaro.lol-anubis*`, `<title>` на русском и
английском, gzip-тело), отсутствие ложных срабатываний (слово «Anubis» в теле, страница входа), редирект (включая `final_url`), cookie между запросами, отказ соединения.
Для `mirrors`: все четыре состояния (в том числе 200 со страницей входа и 200 с Anubis по куке и по `<title>`),
выбор активного без обхода списка, fallback на `known`, приоритет по порядку списка, standby с редиректом и без,
таймаут пробы, `mark_dead` с TTL, `add_manual` (`NeedsLogin` принимается, `Challenge` не меняет активное),
порядок и состояния в `health_check`.
Для `session`: успешный вход (поля формы, заголовки, `UserSession`), отказ со строкой и объектом в `message`, `logged_email`,
не-JSON, пустое тело и JSON без `success`, нет куки или она не число, форма входа после входа, `Challenge`,
ник из `input#name` и запасной вариант, `AuthRequired`, пароль не сохраняется при отказе, `Debug` без пароля,
эхо пароля сервером, очистка HTML. Для `post_form_with_headers`: заголовки уходят, `Challenge` определяется.

Для `parse`: `details` по реальной фикстуре фильма (`id=85038`, 8 переводов, год,
страны, жанры, длительность), `parse_cards` на синтетических фрагментах (полная
карточка, без рейтинга/постера, год-диапазон, относительные ссылки, kind по пути,
мусор без `data-id`, пустая страница, страница входа), сезоны/серии на синтетике,
формирование URL поиска (кириллица, percent-encoding, `page=1` без `&page`) и
каталога (каждый `MediaKind`, `page>1`). Для `api` (wiremock): `search`/`catalog`/
`details` шлют верный путь, пустой запрос без сети, 404 → `NotFound`, страница
входа → `AuthRequired`. Для `sanitize`: каждое перечисленное поле заменяется,
структура HTML сохраняется, `contains_email` отличает реальную почту от
заглушки `user@example.com`.

## Сборка

`wreq` собирает BoringSSL: нужны `cmake`, `clang` и `go`. Минимальная версия Rust — 1.98 (требование `wreq`).
Universal binary собирается из таргетов `x86_64-apple-darwin` и `aarch64-apple-darwin`.
