---
id: 012
type: feature
status: review
model: claude-sonnet-5
crate: kino-api
created: 2026-11-14
---

## Задача

`session.rs`: вход в HDrezka и хранение сессии. Приложением нельзя пользоваться без входа (решение заказчика, 2026-11-14). Пароль и куки `dle_user_id`, `dle_password` хранятся только в macOS Keychain (крейт `keyring`, сервис `kinobox`).

## Ограничения

- `POST /ajax/login/` на выбранное зеркало, поля `login_name`, `login_password`, `login_not_save=0`. Проверено живьём 2026-11-14 на kvk.pub: ответ `{"success":true}`, куки `dle_user_id`, `dle_password`, `PHPSESSID`.
- Форма ошибки живьём не проверялась. По JS страницы входа: `success:false` и `message` (строка с `<li>` либо объект с ключами `email`, `password`, `login_name`, `login_password`) или `logged_email` (уже вошли под другим аккаунтом). Разбирать только эти поля, остальное даёт `KinoError::Parse`.
- Куки привязаны к домену: при смене зеркала вход повторяется паролем из Keychain.
- Пароль не попадает в `Debug`, логи, аргументы командной строки и в `UserSession`. Тип-обёртка с закрытым `Debug`.
- Keychain за трейтом `SecretStore`, в тестах подставляется in-memory реализация.
- Ник для `UserSession`: на странице фильма после входа есть скрытое поле `input#name` в форме комментариев (видел 2026-11-14), на главной его не искал. Если поля нет, брать часть email до `@`. Идентификатор берётся из куки `dle_user_id`.
- Проверка входа: после входа на главной нет `id="loginForm"`.
- Файлы: `session.rs`, `examples/login.rs`, `CRATE.md`. Если нужны `keyring` и `rpassword`, зафиксировать версии точно (`=x.y.z`).

## Готово когда

- Тесты wiremock: успех, неверный пароль, `logged_email`, битый JSON, нет кук, `Debug` без пароля
- Руками: `cargo run -p kino-api --example login -- --mirror https://kvk.pub/ --email you@example.com` спрашивает пароль скрыто, сохраняет в Keychain и печатает `logged in as <ник> (id N)`. Повторный запуск без `--email` берёт данные из Keychain. Флаг `--logout` удаляет их

## Заметки

Зависит от #010. Правило в `AGENTS.md` «только публичные страницы сайта» этой задачей нарушается: каталог доступен только после входа. Правку `AGENTS.md` выполняю только с согласия заказчика.

2026-11-14: вход и хранение данных готовы. 113 тестов `kino-api` и 23 `kino-core`, `fmt`/`clippy`/обе архитектуры зелёные. Живой вход и реальный Keychain НЕ проверялись: у агентов нет учётных данных, тесты идут через wiremock и `MemorySecretStore`.
- `login`, `ensure_logged_in`, `login_and_remember`; `SecretStore` с `KeychainSecretStore` и `MemorySecretStore`; `Password` с закрытым `Debug`, без `Display` и `Serialize`.
- Keychain: сервис `kinobox`, аккаунт `hdrezka-credentials`, значение JSON `{email, password}`. В Keychain данные пишутся только после успешного входа, неверный пароль туда не попадает. Куки в Keychain не пишутся.
- `kino-core::KinoError` получил `LoginRejected(String)` и `Keychain(String)`, оба не повторяются. `CRATE.md` обновлён.
- Пароль вырезается из текста причины отказа на случай, если сервер вернёт его в `message`.
- Зависимости: `keyring =4.2.0`, `rpassword =7.5.5` (только dev, для примера), у `tokio` добавлена feature `rt`.
- Не проверено живьём: форма ответа при ошибке (разбор по JS страницы входа), наличие `input#name` на главной, реальный Keychain, интерактивный скрытый ввод, принимает ли `standby-rezka.tv` вход напрямую.
- Ручная проверка заказчиком: `cargo run -p kino-api --example login`. Первый запуск спросит email и пароль, дальше данные берутся из Keychain, `--logout` их удаляет.
- В `ARCHITECTURE.md` уточнено, что хранится в Keychain и что в памяти.
- PR поверх ветки #011 (PR #10).
