//! Вход в HDrezka и хранение учётных данных в macOS Keychain.
//!
//! Пароль живёт только в [`Password`] (закрытый `Debug`, нет `Display` и `Serialize`) и в Keychain.
//! Куки сессии остаются в cookie-хранилище [`HttpClient`] в памяти и на диск не пишутся.

use std::future::Future;
use std::sync::{Mutex, PoisonError};

use kino_core::{KinoError, Result, UserSession};
use serde_json::Value;
use url::Url;

use crate::client::HttpClient;

const KEYCHAIN_SERVICE: &str = "kinobox";
const KEYCHAIN_ACCOUNT: &str = "hdrezka-credentials";
const LOGIN_PATH: &str = "/ajax/login/";
const LOGIN_FORM_MARKER: &str = "id=\"loginForm\"";
const USER_ID_COOKIE: &str = "dle_user_id";
const REJECT_KEYS: [&str; 4] = ["email", "password", "login_name", "login_password"];
const REDACTED: &str = "***";
const UNKNOWN_REASON: &str = "причина не указана сайтом";
const FALLBACK_USERNAME: &str = "user";
const MAX_ENTITY_LEN: usize = 10;

/// Пароль. `Debug` печатает `Password(***)`; `Display` и `Serialize` нет намеренно.
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Открытое значение: только для отправки на сайт и записи в Keychain.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(***)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub email: String,
    pub password: Password,
}

/// Хранилище учётных данных. Реализации: [`KeychainSecretStore`] и [`MemorySecretStore`] (тесты).
pub trait SecretStore: Send + Sync {
    /// `Ok(None)`, если данных нет.
    fn load(&self) -> impl Future<Output = Result<Option<Credentials>>> + Send;
    fn save(&self, credentials: &Credentials) -> impl Future<Output = Result<()>> + Send;
    /// Удаление отсутствующей записи не ошибка.
    fn clear(&self) -> impl Future<Output = Result<()>> + Send;
}

/// Одна запись Keychain (сервис `kinobox`) с email и паролем. Куки сюда не пишутся.
#[derive(Debug, Clone)]
pub struct KeychainSecretStore {
    service: String,
    account: String,
}

impl KeychainSecretStore {
    pub fn new() -> Self {
        Self {
            service: KEYCHAIN_SERVICE.to_string(),
            account: KEYCHAIN_ACCOUNT.to_string(),
        }
    }
}

impl Default for KeychainSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for KeychainSecretStore {
    async fn load(&self) -> Result<Option<Credentials>> {
        let (service, account) = (self.service.clone(), self.account.clone());
        run_blocking(move || keychain_load(&service, &account)).await
    }

    async fn save(&self, credentials: &Credentials) -> Result<()> {
        let (service, account) = (self.service.clone(), self.account.clone());
        let payload = encode_credentials(credentials);
        run_blocking(move || keychain_save(&service, &account, &payload)).await
    }

    async fn clear(&self) -> Result<()> {
        let (service, account) = (self.service.clone(), self.account.clone());
        run_blocking(move || keychain_clear(&service, &account)).await
    }
}

/// Хранилище в памяти для тестов.
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    inner: Mutex<Option<Credentials>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_credentials(credentials: Credentials) -> Self {
        Self {
            inner: Mutex::new(Some(credentials)),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Credentials>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SecretStore for MemorySecretStore {
    async fn load(&self) -> Result<Option<Credentials>> {
        Ok(self.lock().clone())
    }

    async fn save(&self, credentials: &Credentials) -> Result<()> {
        *self.lock() = Some(credentials.clone());
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        *self.lock() = None;
        Ok(())
    }
}

/// Блокирующие вызовы Keychain выполняются вне async-потока.
async fn run_blocking<T, F>(job: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|_| KinoError::Keychain("задача Keychain прервана".to_string()))?
}

fn keychain_error(err: keyring::Error) -> KinoError {
    KinoError::Keychain(err.to_string())
}

fn keychain_entry(service: &str, account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(service, account).map_err(keychain_error)
}

fn keychain_load(service: &str, account: &str) -> Result<Option<Credentials>> {
    match keychain_entry(service, account)?.get_password() {
        Ok(raw) => decode_credentials(&raw).map(Some),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(keychain_error(err)),
    }
}

fn keychain_save(service: &str, account: &str, payload: &str) -> Result<()> {
    keychain_entry(service, account)?
        .set_password(payload)
        .map_err(keychain_error)
}

fn keychain_clear(service: &str, account: &str) -> Result<()> {
    match keychain_entry(service, account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(keychain_error(err)),
    }
}

fn encode_credentials(credentials: &Credentials) -> String {
    serde_json::json!({
        "email": credentials.email,
        "password": credentials.password.expose(),
    })
    .to_string()
}

fn decode_credentials(raw: &str) -> Result<Credentials> {
    let broken = || KinoError::Keychain("запись с учётными данными повреждена".to_string());
    let value: Value = serde_json::from_str(raw).map_err(|_| broken())?;
    let email = value
        .get("email")
        .and_then(Value::as_str)
        .ok_or_else(broken)?;
    let password = value
        .get("password")
        .and_then(Value::as_str)
        .ok_or_else(broken)?;
    Ok(Credentials {
        email: email.to_string(),
        password: Password::new(password),
    })
}

/// Вход на зеркало. Куки остаются в cookie-хранилище `client`.
///
/// Порядок: `POST /ajax/login/`, разбор ответа, `dle_user_id` из `Set-Cookie`, GET главной
/// (проверка, что формы входа больше нет, и поиск `input#name`).
pub async fn login(
    client: &HttpClient,
    mirror: &Url,
    credentials: &Credentials,
) -> Result<UserSession> {
    let base = site_root(mirror);
    let login_url = base
        .join(LOGIN_PATH)
        .map_err(|err| KinoError::Parse(format!("адрес входа: {err}")))?;
    let form = [
        ("login_name", credentials.email.as_str()),
        ("login_password", credentials.password.expose()),
        ("login_not_save", "0"),
    ];
    let headers = [
        ("X-Requested-With", "XMLHttpRequest"),
        ("Referer", base.as_str()),
    ];

    let reply = client
        .post_form_with_headers(&login_url, &form, &headers)
        .await?;
    check_login_reply(&reply.body, &credentials.password)?;
    let user_id = extract_user_id(&reply.headers)?;

    let home = client.get(&base).await?.text();
    if home.contains(LOGIN_FORM_MARKER) {
        return Err(KinoError::LoginRejected(
            "вход не засчитан: сайт по-прежнему показывает форму входа".to_string(),
        ));
    }
    let username = extract_username(&home)
        .unwrap_or_else(|| username_from_email(&credentials.email).to_string());
    Ok(UserSession { user_id, username })
}

/// Вход сохранёнными данными. Без данных в `store` возвращает `AuthRequired`.
pub async fn ensure_logged_in<S: SecretStore>(
    client: &HttpClient,
    mirror: &Url,
    store: &S,
) -> Result<UserSession> {
    let credentials = store.load().await?.ok_or(KinoError::AuthRequired)?;
    login(client, mirror, &credentials).await
}

/// Вход новыми данными; в `store` они попадают только после успешного входа.
pub async fn login_and_remember<S: SecretStore>(
    client: &HttpClient,
    mirror: &Url,
    store: &S,
    credentials: &Credentials,
) -> Result<UserSession> {
    let session = login(client, mirror, credentials).await?;
    store.save(credentials).await?;
    Ok(session)
}

fn site_root(mirror: &Url) -> Url {
    let mut base = mirror.clone();
    base.set_path("/");
    base.set_query(None);
    base.set_fragment(None);
    base
}

/// Разбирает только `success`, `message` и `logged_email`; всё остальное — `Parse`.
fn check_login_reply(body: &[u8], password: &Password) -> Result<()> {
    let parse = |text: &str| KinoError::Parse(text.to_string());
    let value: Value = serde_json::from_slice(body).map_err(|_| parse("ответ входа не JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| parse("ответ входа не объект JSON"))?;
    let success = match object.get("success") {
        Some(Value::Bool(flag)) => *flag,
        Some(_) => return Err(parse("поле success в ответе входа не булево")),
        None => return Err(parse("в ответе входа нет поля success")),
    };
    if success {
        return Ok(());
    }

    if let Some(email) = object
        .get("logged_email")
        .and_then(Value::as_str)
        .filter(|email| !email.trim().is_empty())
    {
        return Err(KinoError::LoginRejected(format!(
            "уже выполнен вход под другим аккаунтом: {}",
            sanitize(email, password)
        )));
    }

    match object.get("message") {
        Some(Value::String(text)) => {
            let reason = sanitize(text, password);
            let reason = if reason.is_empty() {
                UNKNOWN_REASON.to_string()
            } else {
                reason
            };
            Err(KinoError::LoginRejected(reason))
        }
        Some(Value::Object(fields)) => {
            let reasons: Vec<String> = REJECT_KEYS
                .iter()
                .filter_map(|key| fields.get(*key))
                .filter_map(Value::as_str)
                .map(|text| sanitize(text, password))
                .filter(|text| !text.is_empty())
                .collect();
            if reasons.is_empty() {
                return Err(parse("в message ответа входа нет известных полей"));
            }
            Err(KinoError::LoginRejected(reasons.join("; ")))
        }
        _ => Err(parse(
            "в ответе входа с success=false нет message и logged_email",
        )),
    }
}

/// Пароль вычищается до и после удаления тегов: он может содержать `<` и `>`.
fn sanitize(raw: &str, password: &Password) -> String {
    redact(&clean_message(&redact(raw, password)), password)
}

fn redact(text: &str, password: &Password) -> String {
    if password.is_empty() {
        return text.to_string();
    }
    text.replace(password.expose(), REDACTED)
}

/// Текст сообщения без HTML: теги заменяются пробелом, сущности раскрываются, пробелы схлопываются.
fn clean_message(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(open) = rest.find('<') {
        let after = rest.get(open + 1..).unwrap_or_default();
        let is_tag = after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!');
        match (is_tag, after.find('>')) {
            (true, Some(close)) => {
                text.push_str(rest.get(..open).unwrap_or_default());
                text.push(' ');
                rest = after.get(close + 1..).unwrap_or_default();
            }
            (true, None) => {
                text.push_str(rest.get(..open).unwrap_or_default());
                rest = "";
                break;
            }
            (false, _) => {
                text.push_str(rest.get(..=open).unwrap_or_default());
                rest = after;
            }
        }
    }
    text.push_str(rest);
    decode_entities(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(rest.get(..amp).unwrap_or_default());
        let after = rest.get(amp + 1..).unwrap_or_default();
        let decoded = after
            .find(';')
            .filter(|&semi| semi <= MAX_ENTITY_LEN)
            .and_then(|semi| entity_char(after.get(..semi)?).map(|c| (c, semi)));
        match decoded {
            Some((c, semi)) => {
                out.push(c);
                rest = after.get(semi + 1..).unwrap_or_default();
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity_char(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// `dle_user_id` из заголовков `Set-Cookie`; если кука приходит несколько раз, берётся последняя.
fn extract_user_id(headers: &[(String, String)]) -> Result<u64> {
    let raw = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, value)| cookie_pair(value))
        .filter(|(name, _)| *name == USER_ID_COOKIE)
        .map(|(_, value)| value)
        .next_back()
        .ok_or_else(|| KinoError::Parse("в ответе входа нет куки dle_user_id".to_string()))?;
    raw.parse::<u64>()
        .map_err(|_| KinoError::Parse("кука dle_user_id не число".to_string()))
}

fn cookie_pair(set_cookie: &str) -> Option<(&str, &str)> {
    let first = set_cookie.split(';').next()?;
    let (name, value) = first.split_once('=')?;
    Some((name.trim(), value.trim()))
}

/// Значение `<input id="name" value="...">` с главной страницы.
fn extract_username(html: &str) -> Option<String> {
    const TAG: &str = "<input";
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(offset) = lower.get(from..).and_then(|tail| tail.find(TAG)) {
        let start = from + offset + TAG.len();
        from = start;
        let rest = html.get(start..)?;
        if !rest
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '/' || c == '>')
        {
            continue;
        }
        let attrs = parse_attrs(rest);
        let attr = |name: &str| {
            attrs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        if attr("id") != Some("name") {
            continue;
        }
        if let Some(value) = attr("value") {
            let decoded = decode_entities(value);
            let trimmed = decoded.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Атрибуты тега до закрывающего `>`; имена в нижнем регистре, значения как есть.
fn parse_attrs(rest: &str) -> Vec<(String, String)> {
    let mut chars = rest.chars().peekable();
    let mut attrs = Vec::new();
    loop {
        while chars.next_if(|c| c.is_whitespace() || *c == '/').is_some() {}
        if matches!(chars.peek(), None | Some('>')) {
            break;
        }
        let mut name = String::new();
        while let Some(c) = chars.next_if(|c| !c.is_whitespace() && !matches!(c, '=' | '>' | '/')) {
            name.push(c.to_ascii_lowercase());
        }
        if name.is_empty() {
            chars.next();
            continue;
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut value = String::new();
        if chars.next_if_eq(&'=').is_some() {
            while chars.next_if(|c| c.is_whitespace()).is_some() {}
            match chars.peek().copied() {
                Some(quote @ ('"' | '\'')) => {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == quote {
                            break;
                        }
                        value.push(c);
                    }
                }
                _ => {
                    while let Some(c) = chars.next_if(|c| !c.is_whitespace() && *c != '>') {
                        value.push(c);
                    }
                }
            }
        }
        attrs.push((name, value));
    }
    attrs
}

fn username_from_email(email: &str) -> &str {
    email
        .split('@')
        .next()
        .map(str::trim)
        .filter(|local| !local.is_empty())
        .unwrap_or(FALLBACK_USERNAME)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wiremock::matchers::{body_string_contains, header, header_regex, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::client::ClientConfig;

    const PASSWORD: &str = "Secret-pw_9";
    const CATALOG: &str = "<html><body><h1>Каталог</h1></body></html>";

    fn creds() -> Credentials {
        Credentials {
            email: "user@example.com".to_string(),
            password: Password::new(PASSWORD),
        }
    }

    fn http() -> HttpClient {
        HttpClient::new(ClientConfig {
            timeout: Duration::from_secs(5),
            max_retries: 0,
            base_delay: Duration::from_millis(1),
            user_agent: "kinobox-test".to_string(),
        })
        .expect("клиент должен собираться")
    }

    fn base(server: &MockServer) -> Url {
        Url::parse(&server.uri()).expect("корректный url")
    }

    fn login_ok() -> ResponseTemplate {
        ResponseTemplate::new(200)
            .set_body_string(r#"{"success":true}"#)
            .append_header("Set-Cookie", "dle_user_id=123; Path=/")
            .append_header("Set-Cookie", "dle_password=abc; Path=/")
    }

    fn json(body: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_string(body)
    }

    async fn mount_login(server: &MockServer, template: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/ajax/login/"))
            .respond_with(template)
            .mount(server)
            .await;
    }

    async fn mount_home(server: &MockServer, body: &str) {
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(server)
            .await;
    }

    async fn run_login(template: ResponseTemplate, home: &str) -> Result<UserSession> {
        let server = MockServer::start().await;
        mount_login(&server, template).await;
        mount_home(&server, home).await;
        login(&http(), &base(&server), &creds()).await
    }

    fn assert_no_password(err: &KinoError) {
        assert!(!err.to_string().contains(PASSWORD), "{err}");
        assert!(!format!("{err:?}").contains(PASSWORD), "{err:?}");
    }

    #[tokio::test]
    async fn successful_login_sends_expected_request_and_returns_session() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ajax/login/"))
            .and(header("x-requested-with", "XMLHttpRequest"))
            .and(header("referer", format!("{}/", server.uri()).as_str()))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string_contains("login_name=user%40example.com"))
            .and(body_string_contains(
                format!("login_password={PASSWORD}").as_str(),
            ))
            .and(body_string_contains("login_not_save=0"))
            .respond_with(login_ok())
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/"))
            .and(header_regex("cookie", "dle_user_id=123"))
            .respond_with(ResponseTemplate::new(200).set_body_string(CATALOG))
            .expect(1)
            .mount(&server)
            .await;

        let session = login(&http(), &base(&server), &creds())
            .await
            .expect("ожидали успешный вход");
        assert_eq!(
            session,
            UserSession {
                user_id: 123,
                username: "user".to_string()
            }
        );
    }

    #[tokio::test]
    async fn username_comes_from_input_name_on_home_page() {
        let home = r#"<form><input type="hidden" name="n" id="name" value="kinolover"></form>"#;
        let session = run_login(login_ok(), home).await.expect("успех");
        assert_eq!(session.username, "kinolover");
    }

    #[tokio::test]
    async fn username_falls_back_to_email_local_part() {
        let session = run_login(login_ok(), CATALOG).await.expect("успех");
        assert_eq!(session.username, "user");
    }

    #[tokio::test]
    async fn rejected_with_html_string_message_is_cleaned() {
        let err = run_login(
            json(r#"{"success":false,"message":"<ul><li>Неверный <b>пароль</b></li></ul>"}"#),
            CATALOG,
        )
        .await
        .expect_err("ожидали отказ");
        assert_eq!(err, KinoError::LoginRejected("Неверный пароль".to_string()));
    }

    #[tokio::test]
    async fn rejected_with_object_message_collects_known_keys() {
        let err = run_login(
            json(r#"{"success":false,"message":{"password":"Неверный пароль","other":"x"}}"#),
            CATALOG,
        )
        .await
        .expect_err("ожидали отказ");
        assert_eq!(err, KinoError::LoginRejected("Неверный пароль".to_string()));

        let err = run_login(
            json(r#"{"success":false,"message":{"email":"<li>Плохой email</li>","password":"Пусто"}}"#),
            CATALOG,
        )
        .await
        .expect_err("ожидали отказ");
        assert_eq!(
            err,
            KinoError::LoginRejected("Плохой email; Пусто".to_string())
        );
    }

    #[tokio::test]
    async fn message_object_without_known_keys_is_parse_error() {
        let err = run_login(json(r#"{"success":false,"message":{"zzz":"x"}}"#), CATALOG)
            .await
            .expect_err("ожидали ошибку");
        assert!(matches!(err, KinoError::Parse(_)), "{err:?}");
    }

    #[tokio::test]
    async fn empty_string_message_gives_generic_reason() {
        let err = run_login(
            json(r#"{"success":false,"message":" <li></li> "}"#),
            CATALOG,
        )
        .await
        .expect_err("ожидали отказ");
        assert_eq!(err, KinoError::LoginRejected(UNKNOWN_REASON.to_string()));
    }

    #[tokio::test]
    async fn logged_email_reports_other_account() {
        let err = run_login(
            json(r#"{"success":false,"logged_email":"other@example.com"}"#),
            CATALOG,
        )
        .await
        .expect_err("ожидали отказ");
        assert_eq!(
            err,
            KinoError::LoginRejected(
                "уже выполнен вход под другим аккаунтом: other@example.com".to_string()
            )
        );
    }

    #[tokio::test]
    async fn malformed_replies_are_parse_errors() {
        for body in [
            "",
            "<html>не json</html>",
            r#"{"ok":1}"#,
            "[]",
            r#"{"success":"yes"}"#,
            r#"{"success":false}"#,
        ] {
            let err = run_login(json(body), CATALOG)
                .await
                .expect_err("ожидали ошибку");
            assert!(matches!(err, KinoError::Parse(_)), "{body:?}: {err:?}");
        }
    }

    #[tokio::test]
    async fn missing_or_non_numeric_user_id_cookie_is_parse_error() {
        let no_cookie = json(r#"{"success":true}"#);
        let err = run_login(no_cookie, CATALOG).await.expect_err("нет куки");
        assert!(matches!(err, KinoError::Parse(_)), "{err:?}");

        let bad =
            json(r#"{"success":true}"#).append_header("Set-Cookie", "dle_user_id=abc; Path=/");
        let err = run_login(bad, CATALOG).await.expect_err("не число");
        assert!(matches!(err, KinoError::Parse(_)), "{err:?}");
    }

    #[tokio::test]
    async fn last_user_id_cookie_wins() {
        let reply = json(r#"{"success":true}"#)
            .append_header("Set-Cookie", "dle_user_id=deleted; Path=/")
            .append_header("Set-Cookie", "dle_user_id=77; Path=/");
        let session = run_login(reply, CATALOG).await.expect("успех");
        assert_eq!(session.user_id, 77);
    }

    #[tokio::test]
    async fn login_form_still_present_means_login_not_counted() {
        let err = run_login(login_ok(), r#"<form id="loginForm"></form>"#)
            .await
            .expect_err("ожидали ошибку");
        match err {
            KinoError::LoginRejected(reason) => assert!(reason.contains("не засчитан"), "{reason}"),
            other => panic!("неожиданная ошибка: {other:?}"),
        }
    }

    #[tokio::test]
    async fn challenge_on_login_is_propagated() {
        let err = run_login(
            json("<html><title>Проверяем, что вы не бот!</title></html>"),
            CATALOG,
        )
        .await
        .expect_err("ожидали челлендж");
        assert_eq!(err, KinoError::Challenge);
    }

    #[tokio::test]
    async fn challenge_on_home_page_is_propagated() {
        let err = run_login(
            login_ok(),
            "<html><title>Making sure you're not a bot!</title></html>",
        )
        .await
        .expect_err("ожидали челлендж");
        assert_eq!(err, KinoError::Challenge);
    }

    #[tokio::test]
    async fn ensure_logged_in_without_credentials_is_auth_required() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(login_ok())
            .expect(0)
            .mount(&server)
            .await;

        let store = MemorySecretStore::new();
        let err = ensure_logged_in(&http(), &base(&server), &store)
            .await
            .expect_err("ожидали AuthRequired");
        assert_eq!(err, KinoError::AuthRequired);
    }

    #[tokio::test]
    async fn ensure_logged_in_uses_stored_credentials() {
        let server = MockServer::start().await;
        mount_login(&server, login_ok()).await;
        mount_home(&server, CATALOG).await;

        let store = MemorySecretStore::with_credentials(creds());
        let session = ensure_logged_in(&http(), &base(&server), &store)
            .await
            .expect("успех");
        assert_eq!(session.user_id, 123);
    }

    #[tokio::test]
    async fn rejected_password_is_not_saved() {
        let server = MockServer::start().await;
        mount_login(
            &server,
            json(r#"{"success":false,"message":"Неверный пароль"}"#),
        )
        .await;
        mount_home(&server, CATALOG).await;

        let store = MemorySecretStore::new();
        let err = login_and_remember(&http(), &base(&server), &store, &creds())
            .await
            .expect_err("ожидали отказ");
        assert!(matches!(err, KinoError::LoginRejected(_)), "{err:?}");
        assert_eq!(store.load().await, Ok(None));
    }

    #[tokio::test]
    async fn successful_login_is_saved() {
        let server = MockServer::start().await;
        mount_login(&server, login_ok()).await;
        mount_home(&server, CATALOG).await;

        let store = MemorySecretStore::new();
        login_and_remember(&http(), &base(&server), &store, &creds())
            .await
            .expect("успех");
        assert_eq!(store.load().await, Ok(Some(creds())));
    }

    #[tokio::test]
    async fn memory_store_save_load_clear() {
        let store = MemorySecretStore::new();
        assert_eq!(store.load().await, Ok(None));
        store.save(&creds()).await.expect("save");
        assert_eq!(store.load().await, Ok(Some(creds())));
        store.clear().await.expect("clear");
        assert_eq!(store.load().await, Ok(None));
        store.clear().await.expect("повторный clear");
    }

    #[test]
    fn password_and_credentials_debug_hide_secret() {
        let password = Password::new(PASSWORD);
        assert_eq!(format!("{password:?}"), "Password(***)");
        let debug = format!("{:?}", creds());
        assert!(!debug.contains(PASSWORD), "{debug}");
        assert!(debug.contains("Password(***)"), "{debug}");
        let store = MemorySecretStore::with_credentials(creds());
        assert!(!format!("{store:?}").contains(PASSWORD));
    }

    #[test]
    fn keychain_payload_roundtrip_and_corruption() {
        let payload = encode_credentials(&creds());
        assert_eq!(decode_credentials(&payload), Ok(creds()));
        for broken in ["", "not json", r#"{"email":"a@b"}"#, r#"{"password":"x"}"#] {
            let err = decode_credentials(broken).expect_err("ожидали ошибку");
            assert!(matches!(err, KinoError::Keychain(_)), "{err:?}");
            assert_no_password(&err);
        }
    }

    #[tokio::test]
    async fn no_error_text_contains_password_even_if_server_echoes_it() {
        let bodies = [
            format!(r#"{{"success":false,"message":"<li>Пароль {PASSWORD} неверный</li>"}}"#),
            format!(r#"{{"success":false,"message":{{"password":"bad {PASSWORD}"}}}}"#),
            format!(r#"{{"success":false,"logged_email":"{PASSWORD}"}}"#),
            format!("эхо {PASSWORD}"),
            format!(r#"{{"success":"{PASSWORD}"}}"#),
            format!(r#"{{"success":false,"message":{{"zzz":"{PASSWORD}"}}}}"#),
        ];
        for body in &bodies {
            let err = run_login(json(body), CATALOG)
                .await
                .expect_err("ожидали ошибку");
            assert_no_password(&err);
        }

        let err = run_login(json(&bodies[0]), CATALOG)
            .await
            .expect_err("отказ");
        assert_eq!(
            err,
            KinoError::LoginRejected("Пароль *** неверный".to_string())
        );
    }

    #[test]
    fn password_with_markup_characters_is_redacted_before_cleaning() {
        let password = Password::new("x<y>z");
        let body = br#"{"success":false,"message":"bad x<y>z end"}"#;
        let err = check_login_reply(body, &password).expect_err("отказ");
        assert_eq!(err, KinoError::LoginRejected("bad *** end".to_string()));
    }

    #[test]
    fn clean_message_strips_tags_and_entities() {
        assert_eq!(clean_message("<li>x</li>"), "x");
        assert_eq!(
            clean_message("<ul><li>a</li><li><b>b</b> c</li></ul>"),
            "a b c"
        );
        assert_eq!(clean_message(""), "");
        assert_eq!(clean_message("   "), "");
        assert_eq!(clean_message("<li></li>"), "");
        assert_eq!(
            clean_message("a &amp; b &lt;c&gt; &#33;&#x21;"),
            "a & b <c> !!"
        );
        assert_eq!(clean_message("a < b"), "a < b");
        assert_eq!(clean_message("текст <b"), "текст");
        assert_eq!(clean_message("a&nbsp;b &unknown; &"), "a b &unknown; &");
    }

    #[test]
    fn extract_username_handles_attribute_variants() {
        let name = |html: &str| extract_username(html);
        assert_eq!(
            name(r#"<input id="name" value="kino">"#),
            Some("kino".to_string())
        );
        assert_eq!(
            name(r#"<INPUT VALUE='a &amp; b' ID='name' />"#),
            Some("a & b".to_string())
        );
        assert_eq!(
            name(r#"<input type=hidden id=name value=plain>"#),
            Some("plain".to_string())
        );
        assert_eq!(name(r#"<input data-id="name" value="no">"#), None);
        assert_eq!(name(r#"<input id="name2" value="no">"#), None);
        assert_eq!(name(r#"<input id="name" value="  ">"#), None);
        assert_eq!(name(r#"<input id="name">"#), None);
        assert_eq!(name(r#"<inputx id="name" value="no">"#), None);
        assert_eq!(
            name(r#"<input id="other" value="a"><input id="name" value="b">"#),
            Some("b".to_string())
        );
        assert_eq!(
            name("<input id=\"name\" value=\"обрыв"),
            Some("обрыв".to_string())
        );
        assert_eq!(name(""), None);
    }

    #[test]
    fn username_from_email_variants() {
        assert_eq!(username_from_email("user@example.com"), "user");
        assert_eq!(username_from_email("plain"), "plain");
        assert_eq!(username_from_email("@example.com"), FALLBACK_USERNAME);
        assert_eq!(username_from_email(""), FALLBACK_USERNAME);
    }

    #[test]
    fn site_root_drops_path_query_and_fragment() {
        let url = Url::parse("https://kvk.pub/films/x?a=1#f").expect("url");
        assert_eq!(site_root(&url).as_str(), "https://kvk.pub/");
    }
}
