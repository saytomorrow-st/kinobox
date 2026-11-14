# kino-ipc

Сервисный слой между UI и `kino-api`: запуск, вход, выход, зеркала. Без зависимости от Tauri и specta.
Команды `tauri::command` и `specta::Type` добавляет #015, здесь только `AppService` и DTO с `serde`.

Реэкспортировано из корня крейта: `AppService`, `BootState`, `UserDto`, `MirrorDto`, `MirrorStateDto`,
`IpcError`, `IpcErrorKind`.

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
}
```

В приложении: `AppService<KeychainSecretStore, MemoryMirrorStore>`. SQLite-стор зеркал придёт из `kino-store`,
тогда меняется только второй параметр. В тестах: `MemorySecretStore` и `MemoryMirrorStore`.

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

## Правило про пароль

Пароль приходит только аргументом `login` и сразу оборачивается в `kino_api::Password`. В полях сервиса, DTO,
событиях, логах, `Debug` и `IpcError` его нет. Тексты ошибок берутся из `Display` `KinoError`, а `kino-api` вырезает
пароль из ответов сайта, даже если сервер вернул его «эхом». Тесты проверяют подстрокой Debug и JSON каждого DTO и
ошибки.

## DTO

Все типы `Debug, Clone, PartialEq, Serialize, Deserialize`, поля в camelCase.

| Тип | Форма |
|---|---|
| `UserDto` | `{ userId: u64, username: string }` |
| `BootState` | тег `state`: `needsLogin { email: string \| null }`, `signedIn { user: UserDto }`, `unavailable { message: string }` |
| `MirrorStateDto` | `unknown`, `open`, `needsLogin`, `challenge`, `dead` |
| `MirrorDto` | `{ url, state, reason: string \| null, latencyMs: number \| null, active, standby }` |
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

## Как #015 навесит команды

Один `AppService` кладётся в `tauri::State` (`Arc` не нужен: Tauri держит состояние сам, все методы принимают `&self`).
Каждый метод становится тонкой `#[tauri::command] #[specta::specta]` функцией, например
`async fn login(state: State<'_, Service>, email: String, password: String) -> Result<UserDto, IpcError>`.
Типы получают `specta::Type` одним derive. `IpcError` сериализуется как есть. Логику в командах не дублировать.

## Тесты

```
cargo test -p kino-ipc
```

`wiremock` и `MemorySecretStore`, без реальной сети и Keychain. Покрыто: все исходы `boot`, `login` (успех, отказ,
пустые поля с проверкой счётчика запросов), `logout` (стор, сессия, cookie), состояния и флаг `active` в
`mirrors`/`check_mirrors`, валидация и ошибки `add_mirror`, сброс сессии в `select_mirror`, отсутствие пароля в Debug
и JSON, форма JSON у `BootState`, `MirrorDto`, `IpcError`.
