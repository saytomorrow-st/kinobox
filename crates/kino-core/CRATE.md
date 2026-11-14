# kino-core

Доменные типы KinoBox. Зависимости крейта — только `serde`, `url` и
`thiserror`; сеть, хранилище и плеер здесь не живут. Все публичные типы также реэкспортированы
из корня крейта (`kino_core::MediaItem` и т.д.).

## Медиа (`media`)

| Тип | Поля |
|---|---|
| `MediaKind` | `Movie`, `Series`, `Cartoon`, `Anime` (serde `snake_case`) |
| `MediaItem` | `id: u64`, `title: String`, `original_title: Option<String>`, `year: Option<u16>`, `kind: MediaKind`, `poster_url: Option<Url>`, `page_url: Url`, `rating: Option<f32>` |
| `MediaDetails` | `item: MediaItem`, `description: Option<String>`, `genres: Vec<String>`, `countries: Vec<String>`, `duration_minutes: Option<u32>`, `translators: Vec<Translator>`, `seasons: Vec<Season>` |
| `Translator` | `id: u64`, `name: String` |
| `Season` | `number: u16`, `episodes: Vec<u16>` |

`MediaDetails::seasons` пуст для фильмов и заполнен для сериалов.

## Поток (`stream`)

| Тип | Поля |
|---|---|
| `StreamQuality` | `label: String`, `url: Url`, `premium: bool`, `hls: Option<Url>`, `fallbacks: Vec<Url>` |
| `SubtitleTrack` | `lang: String`, `label: String`, `url: Url`, `is_default: bool` |
| `StreamSource` | `qualities: Vec<StreamQuality>`, `subtitles: Vec<SubtitleTrack>`, `default_quality: Option<String>` |

`SubtitleTrack::lang` — код или метка языка как пришла с сайта.

`StreamQuality` (поля `premium`, `hls`, `fallbacks` помечены `#[serde(default)]`,
поэтому старые данные с только `label` и `url` десериализуются):

- `url` — основной прямой mp4. Если прямых адресов нет, но есть HLS, сюда
  кладётся адрес HLS без суффикса `:hls:manifest.m3u8` (допущение `kino-api`).
- `premium` — признак премиум-качества (метка сайта содержала класс
  `pjs-prem-quality`).
- `hls` — первый HLS-манифест (`...:hls:manifest.m3u8`), если качество его
  предоставляет; прочие HLS игнорируются.
- `fallbacks` — резервные прямые mp4 после основного `url`, в исходном порядке.

## Пользователь (`user`)

| Тип | Поля |
|---|---|
| `UserSession` | `user_id: u64`, `username: String` |

Секретов в типе нет: пароль, куки и токены хранятся только в macOS Keychain.

## Ошибки (`error`)

`pub type Result<T> = std::result::Result<T, KinoError>`.

`KinoError` — `Debug, Clone, PartialEq, Eq`, сообщения (`Display`) на русском.
Типы сетевых библиотек в варианты не попадают: транспортные ошибки
конвертируются в строку.

| Вариант | Смысл |
|---|---|
| `Network(String)` | транспортная ошибка: DNS, таймаут, соединение |
| `Http { status: u16 }` | сервер ответил неуспешным статусом |
| `Parse(String)` | сменилась вёрстка или неожиданный формат ответа |
| `Deobfuscation(String)` | не удалось разобрать ссылку потока |
| `AllMirrorsUnavailable` | ни одно зеркало не ответило |
| `AuthRequired` | нужна авторизация |
| `Challenge` | сайт требует защитный челлендж (например, Anubis PoW), клиент его не прошёл |
| `NotFound` | произведение или страница не найдены |

`is_retryable(&self) -> bool` — `true` для `Network` и для `Http` со статусом
`429` или `500..=599`; для остальных вариантов `false`.

## Провайдер (`provider`)

| Тип | Поля |
|---|---|
| `CatalogQuery` | `kind: Option<MediaKind>` (`None` — все типы), `page: u32` (с 1) |
| `StreamRequest` | `id: u64`, `translator_id: u64`, `season: Option<u16>`, `episode: Option<u16>` (для фильмов `season`/`episode` = `None`) |

Оба типа: `Debug, Clone, PartialEq, Serialize, Deserialize`.

```rust
pub trait KinoProvider: Send + Sync {
    fn search(&self, query: &str)
        -> impl Future<Output = Result<Vec<MediaItem>>> + Send;
    fn catalog(&self, query: CatalogQuery)
        -> impl Future<Output = Result<Vec<MediaItem>>> + Send;
    fn details(&self, item: &MediaItem)
        -> impl Future<Output = Result<MediaDetails>> + Send;
    fn episodes(&self, id: u64, translator_id: u64)
        -> impl Future<Output = Result<Vec<Season>>> + Send;
    fn streams(&self, request: StreamRequest)
        -> impl Future<Output = Result<StreamSource>> + Send;
}
```

Без `async-trait`: реализации пишут обычные `async fn`, а `Send` у future
гарантирует сигнатура трейта. Трейт не object-safe, используется как
generic-параметр.

Контракт ошибок. Любой метод вправе вернуть `Network`, `Http`,
`AllMirrorsUnavailable` и `Challenge`. Дополнительно:

| Метод | Специфичные ошибки и особенности |
|---|---|
| `search` | `Parse`; запрос не нормализуется, пустая выдача — `Ok(vec![])`, не `NotFound` |
| `catalog` | `Parse`; страницы нумеруются с 1 |
| `details` | `NotFound`, `Parse` |
| `episodes` | `NotFound`, `Parse` |
| `streams` | `NotFound`, `Parse`, `Deobfuscation`, `AuthRequired`; возвращает `StreamSource` после деобфускации, не сырую строку |

## Соглашения

- Все типы: `Debug, Clone, PartialEq, Serialize, Deserialize`; для enum и
  простых структур дополнительно `Eq`/`Hash`/`Copy`, где это возможно
  (`MediaItem` и `MediaDetails` — без `Eq` из-за `rating: f32`).
- Для каждого типа есть serde round-trip тест, плюс тест на отсутствие
  секретных ключей в JSON `UserSession`.
