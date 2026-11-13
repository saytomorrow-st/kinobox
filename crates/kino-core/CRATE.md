# kino-core

Доменные типы KinoBox. Зависимости крейта — только `serde` и `url`; сеть,
хранилище и плеер здесь не живут. Все публичные типы также реэкспортированы
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
| `StreamQuality` | `label: String` (`"360p"`, `"720p"`), `url: Url` |
| `SubtitleTrack` | `lang: String`, `label: String`, `url: Url`, `is_default: bool` |
| `StreamSource` | `qualities: Vec<StreamQuality>`, `subtitles: Vec<SubtitleTrack>`, `default_quality: Option<String>` |

`SubtitleTrack::lang` — код или метка языка как пришла с сайта.

## Пользователь (`user`)

| Тип | Поля |
|---|---|
| `UserSession` | `user_id: u64`, `username: String` |

Секретов в типе нет: пароль, куки и токены хранятся только в macOS Keychain.

## Соглашения

- Все типы: `Debug, Clone, PartialEq, Serialize, Deserialize`; для enum и
  простых структур дополнительно `Eq`/`Hash`/`Copy`, где это возможно
  (`MediaItem` и `MediaDetails` — без `Eq` из-за `rating: f32`).
- Для каждого типа есть serde round-trip тест, плюс тест на отсутствие
  секретных ключей в JSON `UserSession`.
