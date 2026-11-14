---
id: 012
type: feature
status: open
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
