# KinoBox

macOS-клиент онлайн-кинотеатра HDrezka в дизайне Apple TV.

Rust + Tauri v2 + React. Открытый исходный код, MIT.

## Статус

**M1 (ядро без UI) — завершён 2026-11-14.** Готовы крейты `kino-core` и
`kino-api`: HTTP-клиент с retry и backoff, пул зеркал с health-check, вход
через macOS Keychain, парсинг страниц, деобфускация потоков, трейт
`KinoProvider`. Всё работает из терминала через примеры (`cargo run -p kino-api
--example …`).

Окна пока нет. UI (React + Tailwind), Tauri-оболочка, плеер `libmpv`,
SQLite-хранилище, автообновление и focus-навигация запланированы на M2.
Возможности из списка ниже описывают конечную цель, а не текущее состояние:

- Главная: hero-карусель и ряды постеров
- Каталог с фильтрами и поиском
- Плеер: выбор качества, перевода, субтитров, запоминание позиции
- Сериалы с сезонами и сериями
- Продолжить просмотр, избранное
- Автообновление из GitHub Releases

## Быстрый старт (ядро, терминал)

Требуется: Rust stable (≥ 1.98), Xcode CLT, `cmake` и `clang` (для сборки BoringSSL).

```bash
# Проверить зеркала и выбрать активное
cargo run -p kino-api --example mirrors

# Войти в аккаунт HDrezka (email спрашивается, пароль — скрыто;
# данные сохранятся в macOS Keychain после успешного входа)
cargo run -p kino-api --example login
# Удалить сохранённые учётные данные:
cargo run -p kino-api --example login -- --logout

# Поиск, детали, каталог (нужен предварительный вход через login)
cargo run -p kino-api --example cli -- search "оно 2"
cargo run -p kino-api --example cli -- details https://example.com/films/85038-odisseya.html
cargo run -p kino-api --example cli -- catalog --kind movie

# Резолв потоков фильма или сериала
cargo run -p kino-api --example play -- https://example.com/films/85038-odisseya.html
# Для сериала нужны --season и --episode:
cargo run -p kino-api --example play -- <url-сериала> --season 1 --episode 2 --quality 720p

# Сохранить обезличенные страницы как фикстуры для тестов
cargo run -p kino-api --example capture -- --out ./captured
```

### Таблица примеров

| Пример | Команда | Назначение |
|---|---|---|
| `mirrors` | `cargo run -p kino-api --example mirrors` | Health-check зеркал, выбор активного |
| `login` | `cargo run -p kino-api --example login` | Вход в аккаунт, сохранение в Keychain |
| `cli` | `cargo run -p kino-api --example cli -- <subcommand>` | Поиск, каталог, детали страницы |
| `play` | `cargo run -p kino-api --example play -- <url>` | Резолв потоков (MP4/HLS) по странице |
| `capture` | `cargo run -p kino-api --example capture -- --out <dir>` | Сбор обезличенных страниц-фикстур |
| `fetch` | `cargo run -p kino-api --example fetch -- <url>` | Сырой GET: статус, попытки, первые 200 символов тела |
| `deobfuscate` | `cargo run -p kino-api --example deobfuscate -- '<строка>'` | Разбор строки потоков (без сети); без аргумента — встроенная демо |

Каталог `./captured` добавлен в `.gitignore`.

## Вход

Приложение работает только после входа в аккаунт HDrezka. Пароль хранится
только в macOS Keychain (сервис `kinobox`), в аргументы командной строки,
логи и переменные окружения не попадает. Куки (`dle_user_id`, `dle_password`,
`PHPSESSID`) живут в памяти HTTP-клиента и на диск не пишутся. После
перезапуска или смены зеркала вход повторяется автоматически сохранёнными
данными. Пользователь должен иметь собственную учётную запись.

## Известные ограничения

- **(а)** Защита Anubis (проверка браузера, PoW) не обходится. Если зеркало
  отдаёт челлендж, оно пропускается; нужно другое зеркало или `--mirror`.
- **(б)** Формат страниц сериалов, каталога и поиска проверен не на реальных
  страницах, а по описанию и синтетическим фрагментам. Реальный формат
  сверяется после запуска `capture`.
- **(в)** Вход и Keychain проверены автоматически только на заглушках
  (`wiremock`, `MemorySecretStore`). Живой вход проверяет заказчик.
- **(г)** Перевод по умолчанию — первый в списке. Выбор перевода для фильма
  через AJAX (`action=get_movie` с другим `translator_id`) не реализован.
- **(д)** Anubis PoW и Tauri UI вне M1.

## Сборка

    cargo tauri dev      # (M2, пока недоступно)
    cargo tauri build    # (M2, пока недоступно)

Требуется: Rust stable, Node 22, pnpm 9, Xcode CLT.

## Документация

    AGENTS.md                      правила работы агентов
    ARCHITECTURE.md                архитектура и дизайн-система
    crates/kino-core/CRATE.md      доменные типы, трейт провайдера
    crates/kino-api/CRATE.md       HTTP-клиент, зеркала, парсинг, деобфускация
    tasks/                         доска задач
    decisions/                     лог решений

## Дисклеймер

Неофициальный клиент. Работает под вашей учётной записью HDrezka, не обходит
DRM и оплату, не скачивает контент массово. Пользователь должен иметь
собственную учётную запись.

Проект не связан с HDrezka и его правообладателями.
