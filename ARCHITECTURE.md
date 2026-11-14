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
    kino-ipc      сервисный слой (AppService, DTO) + специфичные для Tauri типы (specta::Type).
    src-tauri     оболочка Tauri: окна, регистрация команд kino-ipc, привязки TypeScript.
    ui            React 18 + TypeScript + Zustand + Tailwind, один бандл на оба окна.

Правило зависимостей: `kino-core` не знает ни о ком другом крейте.
`kino-api`, `kino-player`, `kino-store` зависят только от `kino-core`.
`kino-ipc` — единственная точка сборки зависимостей ядра.
`src-tauri` зависит только от `kino-ipc` (прямых зависимостей от `kino-api`/`kino-core` нет).
`ui` не знает о Tauri напрямую — только через `ui/src/bindings.ts` (сгенерированный) и `ui/src/api.ts`.

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
    src/service.rs      AppService: конструктор, session()
    src/session.rs      boot, login, logout
    src/mirrors.rs       mirrors, check_mirrors, add_mirror, select_mirror
    src/dto.rs           DTO (serde + specta::Type)
    src/error.rs         IpcError, IpcErrorKind

### src-tauri (пакет kinobox-app, бинарь KinoBox)
    src/lib.rs          сборка бандла команд (tauri-specta), экспорт bindings.ts
    src/main.rs         точка входа
    src/commands.rs     tauri::command — тонкие обёртки над AppService
    src/state.rs        AppService<KeychainSecretStore, MemoryMirrorStore> в tauri::State
    src/windows.rs       открытие/закрытие окон login/main

### ui
    src/App.tsx         выбор экрана по getCurrentWindow().label
    src/screens/        LoginScreen.tsx, MainScreen.tsx
    src/store.ts, src/session-store.ts   Zustand-стор сессии, без пароля
    src/api.ts          единственное место, где UI знает о Tauri
    src/bindings.ts     сгенерировано tauri-specta, не редактировать руками

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

    GET  /search/?do=search&subaction=search&q=<запрос>[&page=<n>]   поиск
    POST /ajax/favorites/                  закладки
    GET  /engine/ajax/gettrailervideo.php  трейлер
    GET  /ajax/get_cdn_tiles/              превью-таймлайн
    POST /ajax/login/                      вход: login_name, login_password, login_not_save=0

Проверено вручную 2026-11-14 на kvk.pub после входа:

- `POST /ajax/login/` отвечает `{"success":true}` и ставит куки `dle_user_id`, `dle_password`, `PHPSESSID`. Форма ответа при ошибке вручную не проверялась. По JS страницы входа это `success:false` и `message` либо `logged_email`.
- На странице фильма данные плеера лежат в `sof.tv.initCDNMoviesEvents(id, translator_id, is_camrip, is_ads, is_director, host, is_logged, {...})`. Внутри JSON с `streams`, `default_quality`, `subtitle`, `subtitle_lns`, `subtitle_def`.

Взято из исходников двух неофициальных библиотек (`ndenissov/HDRezka`, `kristal374/hdrezka-api`, обе MIT), на живых страницах не проверялось:

- Поиск идёт страницей `/search/?...`. Запрос `POST /ajax/search/` в обеих библиотеках не найден.
- `get_cdn_series` в одной библиотеке принимает только поля выше, в другой дополнительно `favs`, `is_camrip`, `is_ads`, `is_director` и параметр `?t=<мс>`. Актуальный набор уточняется в #013.

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

На реальной странице фильма (kvk.pub, 2026-11-14) строка `streams` без префикса `#h`: `[480p]U1 or U2 or U3 or U4,[720p]...`. Внутри качества два HLS-адреса (`...:hls:manifest.m3u8`) и два прямых `.mp4` на двух хостах. Метка премиум-качества содержит HTML (`1080p Ultra` внутри `<span class="pjs-prem-quality">`). Одна из библиотек деобфусцирует всегда, другая по умолчанию выключает деобфускацию, поэтому поддерживаем оба случая (#011).

## Зеркала

Порядок проб:

1. Текущее активное (кэш в SQLite)
2. Список известных зеркал по очереди, HTTP HEAD / главная, таймаут 5с
3. `https://standby-rezka.tv/` — редирект на актуальное зеркало
4. Ручной ввод пользователем в настройках

Состояние зеркала определяется пробой главной страницы:

| Состояние | Признак | Что делает пул |
|---|---|---|
| Open | 2xx, каталог без формы входа | использует |
| NeedsLogin | 2xx, `<form id="loginForm">`, заголовок «Вход на HDrezka» | использует, сначала вход |
| Challenge | куки `techaro.lol-anubis-*` или заголовок «Проверяем, что вы не бот!», статус при этом 200 | пропускает, PoW не реализован |
| Dead | сеть, таймаут, 5xx | пропускает на `dead_ttl` |

Наблюдения 2026-11-14 без VPN: `kvk.pub`, `rezka.fi`, `rezka.si` и `standby-rezka.tv` отдают страницу входа. Первые три побайтно совпадают, `standby-rezka.tv` редиректа не делает. `rezka.ag`, `hdrezka-home.tv`, `hdrezka.name`, `rezkery.com` и `hdrezka.website` отдают Anubis. Состояние зависит от IP, список зеркал меняется. Домен `rezka.si` заказчик не подтвердил как официальный.

## Вход и сессия

Без входа приложением пользоваться нельзя (решение заказчика, 2026-11-14).

- Вход: `POST /ajax/login/` на выбранное зеркало. Куки привязаны к домену, при смене зеркала вход повторяется.
- Email и пароль хранятся только в macOS Keychain (сервис `kinobox`) и записываются туда после успешного входа. Куки `dle_user_id`, `dle_password`, `PHPSESSID` живут в памяти HTTP-клиента, на диск не пишутся. После перезапуска или смены зеркала вход повторяется сохранёнными данными. В логах, в `Debug` и в аргументах командной строки пароля нет. Пароль вводится скрыто в терминале или на экране входа.
- `UserSession` в `kino-core` пароля не содержит.

### Окна

Окнами (`login`, `main`) управляет Rust (`src-tauri/src/windows.rs`), а не фронтенд: это гарантирует, что
открытых окон не бывает нуль. `boot`/`login`, получив успешный вход, сначала создают/показывают `main` и
только потом закрывают `login`. `logout` делает обратное. Закрытие окна `login` крестиком до успешного
входа завершает приложение (`app.exit`) — вход обязателен с первого запуска. Окно входа создаётся скрытым
(`visible: false` в `tauri.conf.json`) и показывается только когда `boot` не привёл к тихому входу
(`NeedsLogin`/`Unavailable`), чтобы не мигать при автоматическом входе сохранёнными данными.

Окно входа использует прозрачность и vibrancy (`windowEffects`, macOS `NSVisualEffectView`) в стиле диалога
«Требуется вход» Apple TV — для этого включён `macOSPrivateApi` в `tauri.conf.json`. Осознанное решение: это
исключает публикацию в Mac App Store, но не мешает распространению через GitHub Releases.

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

## Вехи

### M1 — Ядро без UI (завершено 2026-11-14)

Крейты `kino-core` и `kino-api` полностью реализованы и покрыты тестами.
Всё работает из терминала через примеры (`cargo run -p kino-api --example …`).

Задачи: #001 workspace-scaffold, #002 core-types, #003 core-provider-trait,
#004 api-http-client, #005 api-mirror-pool, #006 api-deobfuscate,
#007 api-parsing-and-cdn, #008 api-fixtures-and-docs, #009 api-anubis-detection,
#010 api-mirror-states, #011 api-deobfuscate-real-format, #012 api-session-login,
#013 api-streams-episodes.

Включено: HTTP-клиент с retry/backoff/Challenge-детекцией, пул зеркал с
health-check и standby, вход через macOS Keychain, парсинг деталей/каталога/поиска,
деобфускация потоков (новый формат и обфусцированный `#h`), AJAX `get_episodes`
и `get_stream`, трейт `KinoProvider`, обезличивание страниц (`sanitize_page`),
одна реальная обезличенная фикстура (страница фильма).

Не включено (план M2): Tauri-оболочка, React UI, плеер `libmpv`, SQLite-хранилище
(`kino-store`), IPC (`kino-ipc`), автообновление, Anubis PoW, выбор перевода для
фильма через AJAX.

### M2 — Вход и оболочка (в работе)

Задачи: #014 `kino-ipc` сервисный слой (сессия, зеркала), #015 оболочка Tauri и каркас UI,
#016 окно входа в стиле Apple TV со списком зеркал.

#014 и #015 реализованы: `AppService` с `boot`/`login`/`logout`/зеркалами, окна `login`/`main`
с привязками TypeScript (`tauri-specta`), минимальные capabilities, строгий CSP. Активное зеркало
пока не сохраняется между запусками (`MemoryMirrorStore`, SQLite-стор из `kino-store` придёт позже).
#016 (красивое окно и список зеркал внутри него) — следующий шаг.
