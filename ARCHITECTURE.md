# KinoBox — Архитектура

## Стек

| Слой | Технология |
|---|---|
| Ядро | Rust 2021, workspace из 5 крейтов |
| Оболочка | Tauri v2 (native macOS, WKWebView) |
| UI | React 18 + TypeScript + Zustand + Tailwind CSS |
| Сеть | `wreq` (преемник `rquest`, BoringSSL), `tokio`, `serde` |
| Хранилище | SQLite (`sqlx`) + файловый кэш постеров |
| Секреты | macOS Keychain (`keyring`) |
| Плеер | `libmpv` через FFI |
| Автообновление | Tauri v2 updater + minisign |

## Крейты

    kino-core     доменные типы, трейты провайдера. Ноль зависимостей от сети.
    kino-api      HTTP-клиент, зеркала, сессия, парсинг, деобфускация.
    kino-player   libmpv: воспроизведение, качество, субтитры, события прогресса.
    kino-store    SQLite, миграции, кэш просмотренного и избранного.
    kino-ipc      типизированные Tauri-команды (tauri-specta) — мост Rust <-> TS.

Правило зависимостей: `kino-core` не знает ни о ком другом крейте.
`kino-api`, `kino-player`, `kino-store` зависят только от `kino-core`.
`kino-ipc` — единственная точка сборки зависимостей.

## Поток данных

    UI (React)
      |  tauri::command (типизированно, через specta)
      v
    kino-ipc  --->  kino-api  --->  MirrorPool --->  HDrezka зеркала
      |                |
      |                +--> deobfuscate(url) -> Vec<StreamQuality>
      |
      +-->  kino-store  ---> SQLite
      |
      +-->  kino-player ---> libmpv

## Крейты — файлы и ответственность

### kino-core
    src/lib.rs
    src/media.rs        MediaItem, MediaKind, MediaDetails
    src/stream.rs       StreamQuality, StreamSource, SubtitleTrack
    src/provider.rs     trait KinoProvider
    src/error.rs        KinoError
    src/user.rs         UserSession (без пароля!)

### kino-api
    src/lib.rs
    src/client.rs       HTTP-клиент: retry, backoff, timeout, UA
    src/mirrors.rs      MirrorPool: список, probe, health-check
    src/session.rs      login, cookies, Keychain
    src/deobfuscate.rs  разбор "[360p]url,[720p]url" + снятие мусора
    src/parse/          поиск, каталог, детали, серии
    src/tiles.rs        превью-таймлайн
    src/trailers.rs     трейлеры

### kino-store
    src/lib.rs
    src/db.rs           пул соединений
    src/migrations/     001_init.sql, 002_...
    src/progress.rs     прогресс просмотра
    src/favorites.rs    избранное (кэш + синк с сайтом)
    src/cache.rs        файловый кэш постеров

### kino-player
    src/lib.rs
    src/mpv.rs          обёртка libmpv, контекст
    src/source.rs       StreamSource -> mpv
    src/events.rs       события: position, duration, state
    src/subtitles.rs     внешние субтитры

### kino-ipc
    src/lib.rs
    src/commands/       по одному модулю на домен
    src/specta.rs       сборка типов

## API HDrezka — используемое

Главный эндпоинт:

    POST /ajax/get_cdn_series/
    id, translator_id, action=get_movie|get_stream|get_episodes, season?, episode?

Ответ:

    { "success": true,
      "url": "[360p]https://...,[720p]https://...",
      "quality": "720p",
      "subtitle": "...", "subtitle_lns": "...", "subtitle_def": "..." }

Остальное:

    POST /ajax/search/                     поиск
    POST /ajax/favorites/                  закладки
    GET  /engine/ajax/gettrailervideo.php  трейлер
    GET  /ajax/get_cdn_tiles/              превью-таймлайн
    POST /ajax/login/                      сессия (куки dle_user_id, dle_password)

## Деобфускация ссылок

Поток: если url начинается с `#h` — обфусцирован.
1. Убрать префикс `#h`
2. Разбить по `//_//`
3. Убрать известные мусорные токены:

       $$!!@$$@^!@#$$@
       @@@@@!##!^^^
       ####^!!##!@@
       ^^^!@##!!##
       $$#!!@#!@##

4. base64 decode остатка

Список токенов хранить в коде константой и обновлять при поломке.

## Зеркала

Порядок проб:

1. Текущее активное (кэш в SQLite)
2. Список известных зеркал по очереди, HTTP HEAD / главная, таймаут 5с
3. `https://standby-rezka.tv/` — редирект на актуальное зеркало
4. Ручной ввод пользователем в настройках

## Дизайн-система

    Фон:              #000000
    Фон 2:            rgba(255,255,255,0.04) поверх blur(40px)
    Текст:            #F5F5F7
    Текст 2:          #86868B
    Акцент:           #FA233B
    Скругление:       12px карточки, 8px кнопки, 20px модалки
    Отступы:          60px горизонталь, 40px между секциями
    Шрифт:            SF Pro Display / -apple-system
    Hero-заголовок:   42px
    Заголовок секции: 24px
    Текст карточки:   15px

Принципы:
- Никаких видимых рамок и разделителей. Только отступы, blur и тень.
- Крупная hero-обложка 16:9 + логотип фильма.
- Ряд 10×2, при фокусе вторая строка уезжает вниз (shelf push).
- Focus lerp-переход, не скачок.
- Инерционное затухание горизонтального скролла по краям.
- Модалка: постер на весь экран, затемнение, кнопка ▶, выбор качества/перевода/субтитров.
