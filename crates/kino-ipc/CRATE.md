# kino-ipc

Сервисный слой между UI и `kino-api`: запуск, вход, выход, зеркала, каталог. Без зависимости от Tauri.
Команды `tauri::command` добавляет `src-tauri` (#015), здесь только `AppService` и DTO с `serde` и `specta::Type`.

Реэкспортировано из корня крейта: `AppService`, `BootState`, `UserDto`, `MirrorDto`, `MirrorStateDto`,
`MediaKindDto`, `MediaItemDto`, `MediaDetailsDto`, `CollectionDto`, `TranslatorDto`, `SeasonDto`, `EpisodeDto`,
`StreamQualityDto`, `PlayerProgressDto`, `PlayerErrorDto`, `IpcError`, `IpcErrorKind`, `ClientConfig`, `MirrorConfig`,
`KeychainSecretStore`, `MemorySecretStore`, `MemoryMirrorStore`.

## AppService

```rust
pub struct AppService<S: SecretStore, M: MirrorStore> { /* приватно */ }

impl<S: SecretStore, M: MirrorStore> AppService<S, M> {
    pub fn new(client_config: ClientConfig, secrets: S, mirror_store: M, mirror_config: MirrorConfig)
        -> kino_core::Result<Self>;

    pub async fn boot(&self) -> BootState;
    pub async fn login(&self, email: String, password: String) -> Result<UserDto, IpcError>;
    pub async fn logout(&self) -> Result<(), IpcError>;
    pub fn session(&self) -> Option<UserDto>;

    pub async fn mirrors(&self) -> Vec<MirrorDto>;
    pub async fn check_mirrors(&self) -> Vec<MirrorDto>;
    pub async fn add_mirror(&self, url: String) -> Result<MirrorDto, IpcError>;
    pub async fn select_mirror(&self, url: String) -> Result<MirrorDto, IpcError>;

    pub async fn catalog(&self, kind: Option<MediaKindDto>, page: u32) -> Result<Vec<MediaItemDto>, IpcError>;
    pub async fn search(&self, query: String) -> Result<Vec<MediaItemDto>, IpcError>;
    pub async fn details(&self, item: MediaItemDto) -> Result<MediaDetailsDto, IpcError>;
    pub async fn collections(&self) -> Result<Vec<CollectionDto>, IpcError>;
    pub async fn collection_items(&self, collection: CollectionDto, page: u32) -> Result<Vec<MediaItemDto>, IpcError>;
    pub async fn episodes(&self, details: MediaDetailsDto) -> Result<Vec<EpisodeDto>, IpcError>;
    pub async fn streams(&self, item: MediaItemDto, translator_id: Option<u64>, episode: Option<EpisodeDto>) -> Result<Vec<StreamQualityDto>, IpcError>;
}
```

В приложении: `AppService<KeychainSecretStore, MemoryMirrorStore>`. SQLite-стор зеркал придёт из `kino-store`,
тогда меняется только второй параметр. В тестах: `MemorySecretStore` и `MemoryMirrorStore`.

**Известное ограничение:** активное зеркало хранится в `MemoryMirrorStore` и не сохраняется между запусками приложения.

Сервис хранит `HttpClient` (с cookie-хранилищем), `MirrorPool`, `SecretStore` и текущую `UserSession`.
Сессия живёт в памяти. Куки в Keychain не пишутся.

### Методы

- `boot`: загружает данные из `SecretStore`, выбирает зеркало (`resolve`), входит (`login` из `kino-api`).
  Исходы см. в «BootState». Данные в Keychain `boot` не меняет и не удаляет.
- `login`: пустой email (после `trim`) или пустой пароль дают `InvalidInput` без запросов. Иначе берётся активное
  зеркало (если его нет, `resolve`) и вызывается `login_and_remember`: данные попадают в Keychain только после успеха.
- `logout`: `SecretStore::clear`, затем сброс сессии и cookie. Если Keychain вернул ошибку, сессия остаётся.
  У `HttpClient` нет сброса cookie-хранилища, поэтому сервис строит новый клиент с теми же настройками.
- `session`: текущий пользователь или `None`.
- `mirrors`: известные зеркала из `MirrorConfig` и активное. Без сети, состояние `Unknown`, `latencyMs: null`.
  Активное зеркало вне списка (введённое вручную) добавляется в конец.
- `check_mirrors`: `health_check` и `probe_standby` параллельно. Порядок: известные зеркала как в конфиге,
  активное вне списка (проверяется отдельно), последним standby с `standby: true`.
- `add_mirror` и `select_mirror`: одинаковы. URL проверяется (только `http`/`https`, есть хост, без логина и пароля
  в адресе), путь, запрос и фрагмент отбрасываются. Затем `add_manual` и, при успехе, сброс сессии и cookie, потому что
  куки привязаны к домену. Следующие `login` и `boot` идут на новом зеркале. При ошибке (в том числе `Challenge`)
  активное зеркало и сессия не меняются. Введённый адрес в текст ошибки не попадает.
- `catalog`, `search`, `details`, `episodes`, `streams`, `collections`, `collection_items`: тонкий слой над
  `kino_core::KinoProvider`, который реализует `kino_api::HdrezkaApi` (`provider_impl.rs`). Логики парсинга или
  сети здесь нет — только выбор зеркала и конвертация DTO. Ошибки `KinoProvider` (включая `AuthRequired`,
  `NotFound` → `internal`) идут через обычный `From<KinoError> for IpcError`.
- `streams` принимает `item: MediaItemDto` (не `MediaDetailsDto`) и **перезапрашивает детали внутри**
  (`provider.details(&item)`): потокам фильма нужен `raw_html` страницы (данные плеера `initCDN...Events`),
  а `MediaDetailsDto` его физически не несёт. `raw_html` остаётся в памяти и не сериализуется в сеть —
  это осознанная цена одного лишнего сетевого запроса за каждый клик «смотреть».

### `HdrezkaApi` и активное зеркало

`AppService` не хранит `HdrezkaApi` в полях: активное зеркало может поменяться в любой момент через
`select_mirror`/`add_mirror` (они делают `reset_client`), и застрявший в поле провайдер держал бы старый `base`.
Вместо этого приватный метод `provider()` на каждый вызов каталога:

1. берёт активное зеркало через тот же путь, что `login` (`mirror_url`: `pool.store().active()`, а если зеркала
   ещё нет — `pool.resolve()`, чтобы первый вызов `catalog`/`search` до входа тоже работал);
2. строит свежий `HdrezkaApi::new(self.client(), mirror)` — конструктор дешёвый (клонирует `HttpClient`, у которого
   внутри `Arc`-подобное соединение, и присваивает `base`).

`catalog`/`search`/`details`/`episodes` зовут `KinoProvider`-методы этого `HdrezkaApi` (для `search` — явно
`KinoProvider::search(&provider, &query)`, потому что у `HdrezkaApi` есть одноимённый inherent-метод с пагинацией,
который иначе перекрыл бы метод трейта).

## Правило про пароль

Пароль приходит только аргументом `login` и сразу оборачивается в `kino_api::Password`. В полях сервиса, DTO,
событиях, логах, `Debug` и `IpcError` его нет. Тексты ошибок берутся из `Display` `KinoError`, а `kino-api` вырезает
пароль из ответов сайта, даже если сервер вернул его «эхом». Тесты проверяют подстрокой Debug и JSON каждого DTO и
ошибки.

## DTO

Все типы `Debug, Clone, PartialEq, Serialize, Deserialize`, поля в camelCase.

| Тип | Форма |
|---|---|
| `UserDto` | `{ userId: number, username: string }` (userId экспортируется как `number`, точность до 2^53) |
| `BootState` | тег `state`: `needsLogin { email: string \| null }`, `signedIn { user: UserDto }`, `unavailable { message: string }` |
| `MirrorStateDto` | `unknown`, `open`, `needsLogin`, `challenge`, `dead` |
| `MirrorDto` | `{ url, state, reason: string \| null, latencyMs: number \| null, active, standby }` (latencyMs экспортируется как `number`) |
| `MediaKindDto` | `movie`, `series`, `cartoon`, `anime` — зеркало `kino_core::MediaKind`, `From`/`Into` в обе стороны |
| `MediaItemDto` | `{ id: number, title, originalTitle: string \| null, year: number \| null, kind: MediaKindDto, posterUrl: string \| null, pageUrl: string, rating: number \| null }`. `id` экспортируется как `number`; `posterUrl`/`pageUrl` — строки (не `Url`, у `specta` нет для него типа). `try_into_media_item` разбирает их обратно и возвращает `InvalidInput`, если UI подсунул испорченный DTO |
| `MediaDetailsDto` | `{ item: MediaItemDto, description: string \| null, genres: string[], countries: string[], durationMinutes: number \| null, translators: TranslatorDto[], seasons: SeasonDto[] }`. Поля ровно эти — `raw_html` отсутствует физически, не просто скрыт сериализацией (см. тест `media_details_dto_has_no_raw_html_field_at_all`) |
| `CollectionDto` | `{ id: number, title, url, posterUrl: string \| null, itemsCount: number \| null }`. `id` экспортируется как `number`; `url`/`posterUrl` — строки (не `Url`). `try_into_collection` разбирает их обратно и возвращает `InvalidInput`, если UI подсунул испорченный DTO |
| `TranslatorDto` | `{ id: number, name: string }` |
| `SeasonDto` | `{ number, episodes: number[] }` |
| `EpisodeDto` | `{ seasonId, episodeId, title }` |
| `StreamQualityDto` | `{ label, url, premium, hls: string \| null, fallbacks: string[] }`. `try_into_stream_quality` разбирает адреса обратно в `StreamQuality` и возвращает `InvalidInput` на испорченный URL — использует `src-tauri` (#021) для загрузки потока в `kino-player` |
| `PlayerProgressDto` | `{ position: number \| null, duration: number \| null, paused: boolean }` — снимок прогресса, который `src-tauri` эмиттит в окно `player-controls` событием `player-progress` |
| `PlayerErrorDto` | `{ message: string }` — человекочитаемое русское сообщение асинхронной ошибки mpv, которое `src-tauri` эмиттит событием `player-error` |
| `IpcError` | `{ kind: IpcErrorKind, message: string }`, реализует `Display` и `std::error::Error` |
| `IpcErrorKind` | `invalidInput`, `loginRejected`, `authRequired`, `challenge`, `network`, `keychain`, `noMirrors`, `internal` |

`IpcErrorKind` из `KinoError`: `Network` и `Http` в `network`; `Parse`, `Deobfuscation`, `NotFound` в `internal`;
`AllMirrorsUnavailable` в `noMirrors`; `AuthRequired`, `Challenge`, `LoginRejected`, `Keychain` в одноимённые.
`invalidInput` создаёт сам сервис.

### BootState

| Исход | Состояние |
|---|---|
| в Keychain ничего нет | `needsLogin { email: null }` |
| вход прошёл | `signedIn { user }` |
| сайт отклонил сохранённые данные | `needsLogin { email: "<сохранённый>" }`, данные остаются |
| нет рабочих зеркал, все зеркала с Anubis, сеть, ошибка Keychain или сайта | `unavailable { message }`, данные остаются |

## Поток запуска

1. UI вызывает `boot`.
2. `needsLogin`: показывается окно входа (email подставляется, если пришёл). Раскрывающийся список зеркал:
   `mirrors` при открытии, `check_mirrors` по кнопке «Проверить», `add_mirror` для своего адреса,
   `select_mirror` для выбора.
3. `login(email, password)`: успех возвращает `UserDto`, данные сохраняются в Keychain. `loginRejected` показывает
   текст сайта в окне.
4. `signedIn`: приложение открывается. `unavailable`: показывается сообщение и выбор зеркала.
5. Выход: `logout`, затем снова окно входа.

## Интеграция с Tauri (src-tauri, #015)

Один `AppService` кладётся в `tauri::State` (`Arc` не нужен: Tauri держит состояние сам, все методы принимают `&self`).
Каждый метод становится тонкой `#[tauri::command] #[specta::specta]` функцией, например
`async fn login(state: State<'_, Service>, email: String, password: String) -> Result<UserDto, IpcError>`.
Типы получают `specta::Type` одним derive. `IpcError` сериализуется как есть. Логику в командах не дублировать.

**Specta и u64:** поля `user_id`, `latency_ms` и ID в `MediaItemDto`/`TranslatorDto` помечены
`#[specta(type = Number)]` (или `Option<Number>`) для экспорта в TypeScript как `number`. ID HDrezka укладываются
в JS-число (до 2^53-1), потерь точности нет. `latencyMs` в миллисекундах редко превысит 10000, а потери <1мс
несущественны. Serde-сериализация остаётся `u64`.

## Тесты

```
cargo test -p kino-ipc
```

`wiremock` и `MemorySecretStore`, без реальной сети и Keychain. Покрыто: все исходы `boot`, `login` (успех, отказ,
пустые поля с проверкой счётчика запросов), `logout` (стор, сессия, cookie), состояния и флаг `active` в
`mirrors`/`check_mirrors`, валидация и ошибки `add_mirror`, сброс сессии в `select_mirror`, отсутствие пароля в Debug
и JSON, форма JSON у `BootState`, `MirrorDto`, `IpcError`. Для каталога: `catalog`/`search`/`details`/`episodes`
на активном и на не выбранном зеркале (авторезолв), `NoMirrors` без рабочих зеркал, пустой поиск без запроса,
отказ `details` на невалидном `MediaItemDto` без похода в сеть, `episodes` для фильма без сезонов без похода
в сеть. Для потоков: `streams` для серии (мокаются и страница деталей, и `get_stream`) и `streams` для фильма
(перезапрос деталей внутри и деобфускация из данных плеера `initCDNMoviesEvents`). А также DTO-тесты в `dto.rs`:
форма JSON (`camelCase`), оба направления конвертации `MediaItemDto`/`MediaDetailsDto`/`StreamQualityDto`,
и явная проверка, что `MediaDetailsDto` физически не содержит `raw_html`.
