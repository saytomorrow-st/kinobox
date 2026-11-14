use std::fmt::Debug;
use std::time::Duration;

use kino_api::{
    ClientConfig, Credentials, MemoryMirrorStore, MemorySecretStore, MirrorConfig, MirrorStore,
    Password, SecretStore,
};
use serde::Serialize;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::{AppService, BootState, IpcError, IpcErrorKind, MirrorDto, MirrorStateDto, UserDto};

const PASSWORD: &str = "Zq9!pw-secret";
const EMAIL: &str = "user@example.com";
const HOME: &str = "<html><body>главная страница</body></html>";
const STUB: &str = "<form id=\"loginForm\" action=\"/ajax/login/\"></form>";
const ANUBIS_TITLE: &str = "<html><head><title>Проверяем, что вы не бот!</title></head></html>";

type Service = AppService<MemorySecretStore, MemoryMirrorStore>;

fn creds() -> Credentials {
    Credentials {
        email: EMAIL.to_string(),
        password: Password::new(PASSWORD),
    }
}

fn login_ok() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(r#"{"success":true}"#)
        .append_header("Set-Cookie", "dle_user_id=123; Path=/")
}

fn login_failed(message: &str) -> ResponseTemplate {
    let body = serde_json::json!({"success": false, "message": message});
    ResponseTemplate::new(200).set_body_string(body.to_string())
}

/// Зеркало с входом: `POST /ajax/login/` отвечает `login`, главная отдаёт каталог.
async fn site(login: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ajax/login/"))
        .respond_with(login)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HOME))
        .mount(&server)
        .await;
    server
}

async fn answering(template: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

async fn dead() -> MockServer {
    answering(ResponseTemplate::new(503)).await
}

async fn hits(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .map(|requests| requests.len())
        .unwrap_or_default()
}

fn base(server: &MockServer) -> Url {
    Url::parse(&format!("{}/", server.uri())).expect("корректный url")
}

fn build(
    secrets: MemorySecretStore,
    store: MemoryMirrorStore,
    known: &[&MockServer],
    standby: &MockServer,
) -> Service {
    build_with_standby(secrets, store, known, base(standby))
}

/// Standby без mock-сервера: адрес закрытого порта. Сервер, созданный внутри помощника и
/// сразу уничтоженный, возвращается в пул `wiremock`, и чужой тест получает тот же порт.
fn unreachable_standby() -> Url {
    Url::parse("http://127.0.0.1:1/").expect("корректный url")
}

fn build_with_standby(
    secrets: MemorySecretStore,
    store: MemoryMirrorStore,
    known: &[&MockServer],
    standby: Url,
) -> Service {
    let client = ClientConfig {
        timeout: Duration::from_secs(5),
        max_retries: 0,
        base_delay: Duration::from_millis(1),
        user_agent: "kinobox-test".to_string(),
    };
    let mirrors = MirrorConfig {
        known: known.iter().map(|server| base(server)).collect(),
        standby,
        probe_timeout: Duration::from_secs(2),
        dead_ttl: Duration::from_millis(400),
    };
    AppService::new(client, secrets, store, mirrors).expect("сервис")
}

async fn service(known: &[&MockServer]) -> Service {
    build_with_standby(
        MemorySecretStore::new(),
        MemoryMirrorStore::new(),
        known,
        unreachable_standby(),
    )
}

async fn saved_service(known: &[&MockServer]) -> Service {
    build_with_standby(
        MemorySecretStore::with_credentials(creds()),
        MemoryMirrorStore::new(),
        known,
        unreachable_standby(),
    )
}

fn assert_clean<T: Serialize + Debug>(value: &T) {
    let json = serde_json::to_string(value).expect("json");
    assert!(!json.contains(PASSWORD), "пароль в JSON: {json}");
    let debug = format!("{value:?}");
    assert!(!debug.contains(PASSWORD), "пароль в Debug: {debug}");
}

fn user() -> UserDto {
    UserDto {
        user_id: 123,
        username: "user".to_string(),
    }
}

#[tokio::test]
async fn boot_without_credentials_needs_login() {
    let server = site(login_ok()).await;
    let service = service(&[&server]).await;

    assert_eq!(service.boot().await, BootState::NeedsLogin { email: None });
    assert_eq!(hits(&server).await, 0);
    assert_eq!(service.session(), None);
}

#[tokio::test]
async fn boot_with_valid_credentials_signs_in() {
    let server = site(login_ok()).await;
    let service = saved_service(&[&server]).await;

    assert_eq!(service.boot().await, BootState::SignedIn { user: user() });
    assert_eq!(service.session(), Some(user()));
}

#[tokio::test]
async fn boot_with_rejected_credentials_keeps_them_and_returns_email() {
    let server = site(login_failed("Неверный пароль")).await;
    let service = saved_service(&[&server]).await;

    assert_eq!(
        service.boot().await,
        BootState::NeedsLogin {
            email: Some(EMAIL.to_string())
        }
    );
    assert_eq!(service.session(), None);
    assert_eq!(service.secrets.load().await, Ok(Some(creds())));
}

#[tokio::test]
async fn boot_without_working_mirrors_is_unavailable_and_keeps_credentials() {
    let broken = dead().await;
    let service = saved_service(&[&broken]).await;

    let BootState::Unavailable { message } = service.boot().await else {
        panic!("ожидали Unavailable");
    };
    assert!(!message.is_empty());
    assert_eq!(service.secrets.load().await, Ok(Some(creds())));
}

#[tokio::test]
async fn boot_with_challenge_on_every_mirror_explains_it() {
    let anubis = answering(ResponseTemplate::new(200).set_body_string(ANUBIS_TITLE)).await;
    let service = saved_service(&[&anubis]).await;

    let BootState::Unavailable { message } = service.boot().await else {
        panic!("ожидали Unavailable");
    };
    assert!(message.contains("Anubis"), "{message}");
    assert_eq!(service.secrets.load().await, Ok(Some(creds())));
}

#[tokio::test]
async fn boot_with_site_error_is_unavailable_and_keeps_credentials() {
    let server = site(ResponseTemplate::new(500)).await;
    let service = saved_service(&[&server]).await;

    let BootState::Unavailable { message } = service.boot().await else {
        panic!("ожидали Unavailable");
    };
    assert!(message.contains("500"), "{message}");
    assert_eq!(service.secrets.load().await, Ok(Some(creds())));
}

#[tokio::test]
async fn login_success_saves_credentials_and_session() {
    let server = site(login_ok()).await;
    let service = service(&[&server]).await;
    assert_eq!(service.secrets.load().await, Ok(None));

    let result = service.login(EMAIL.to_string(), PASSWORD.to_string()).await;

    assert_eq!(result, Ok(user()));
    assert_eq!(service.session(), Some(user()));
    assert_eq!(service.secrets.load().await, Ok(Some(creds())));
}

#[tokio::test]
async fn login_with_wrong_password_is_rejected_and_nothing_saved() {
    let server = site(login_failed("Неверный пароль")).await;
    let service = service(&[&server]).await;

    let err = service
        .login(EMAIL.to_string(), PASSWORD.to_string())
        .await
        .expect_err("ожидали отказ");

    assert_eq!(err.kind, IpcErrorKind::LoginRejected);
    assert!(err.message.contains("Неверный пароль"), "{err}");
    assert_eq!(service.secrets.load().await, Ok(None));
    assert_eq!(service.session(), None);
}

#[tokio::test]
async fn login_with_empty_fields_makes_no_requests() {
    let server = site(login_ok()).await;
    let service = service(&[&server]).await;

    for (email, password) in [("", PASSWORD), ("   ", PASSWORD), (EMAIL, ""), ("", "")] {
        let err = service
            .login(email.to_string(), password.to_string())
            .await
            .expect_err("ожидали InvalidInput");
        assert_eq!(err.kind, IpcErrorKind::InvalidInput);
    }

    assert_eq!(hits(&server).await, 0);
    assert_eq!(service.secrets.load().await, Ok(None));
}

#[tokio::test]
async fn login_without_mirrors_reports_no_mirrors() {
    let broken = dead().await;
    let service = service(&[&broken]).await;

    let err = service
        .login(EMAIL.to_string(), PASSWORD.to_string())
        .await
        .expect_err("ожидали ошибку");

    assert_eq!(err.kind, IpcErrorKind::NoMirrors);
    assert_eq!(service.secrets.load().await, Ok(None));
}

#[tokio::test]
async fn logout_clears_store_session_and_cookies() {
    let server = site(login_ok()).await;
    let service = service(&[&server]).await;
    service
        .login(EMAIL.to_string(), PASSWORD.to_string())
        .await
        .expect("вход");
    let url = base(&server);

    service.client().get(&url).await.expect("запрос");
    assert!(last_request_has_cookie(&server).await);

    assert_eq!(service.logout().await, Ok(()));

    assert_eq!(service.secrets.load().await, Ok(None));
    assert_eq!(service.session(), None);
    service.client().get(&url).await.expect("запрос");
    assert!(!last_request_has_cookie(&server).await);
}

async fn last_request_has_cookie(server: &MockServer) -> bool {
    server
        .received_requests()
        .await
        .and_then(|requests| requests.last().map(|r| r.headers.contains_key("cookie")))
        .unwrap_or_default()
}

#[tokio::test]
async fn mirrors_lists_known_without_requests() {
    let first = site(login_ok()).await;
    let second = site(login_ok()).await;
    let standby = dead().await;
    let service = build(
        MemorySecretStore::new(),
        MemoryMirrorStore::with_active(base(&second)),
        &[&first, &second],
        &standby,
    );

    let list = service.mirrors().await;

    assert_eq!(list.len(), 2);
    assert_eq!(list[0].url, base(&first).to_string());
    assert!(list.iter().all(|m| m.state == MirrorStateDto::Unknown));
    assert!(list.iter().all(|m| m.latency_ms.is_none() && !m.standby));
    assert_eq!(
        list.iter().map(|m| m.active).collect::<Vec<_>>(),
        vec![false, true]
    );
    assert_eq!(
        hits(&first).await + hits(&second).await + hits(&standby).await,
        0
    );
}

#[tokio::test]
async fn check_mirrors_maps_every_state() {
    let open = answering(ResponseTemplate::new(200).set_body_string(HOME)).await;
    let needs_login = answering(ResponseTemplate::new(200).set_body_string(STUB)).await;
    let challenge = answering(ResponseTemplate::new(200).set_body_string(ANUBIS_TITLE)).await;
    let broken = dead().await;
    let standby = dead().await;
    let service = build(
        MemorySecretStore::new(),
        MemoryMirrorStore::with_active(base(&needs_login)),
        &[&open, &needs_login, &challenge, &broken],
        &standby,
    );

    let list = service.check_mirrors().await;

    let states: Vec<_> = list.iter().map(|m| m.state).collect();
    assert_eq!(
        states,
        vec![
            MirrorStateDto::Open,
            MirrorStateDto::NeedsLogin,
            MirrorStateDto::Challenge,
            MirrorStateDto::Dead,
            MirrorStateDto::Dead,
        ]
    );
    assert_eq!(
        list.iter().map(|m| m.active).collect::<Vec<_>>(),
        vec![false, true, false, false, false]
    );
    assert_eq!(
        list.iter().map(|m| m.standby).collect::<Vec<_>>(),
        vec![false, false, false, false, true]
    );
    assert!(list[0].latency_ms.is_some());
    assert!(list[1].latency_ms.is_some());
    assert_eq!(list[1].reason.as_deref(), Some("требуется вход"));
    assert!(list[2].latency_ms.is_none());
    assert!(list[2].reason.is_some());
    assert!(list[3].latency_ms.is_none());
    assert!(list[3].reason.as_deref().is_some_and(|r| r.contains("503")));
}

#[tokio::test]
async fn check_mirrors_shows_manual_active_mirror_outside_known() {
    let known = answering(ResponseTemplate::new(200).set_body_string(HOME)).await;
    let manual = answering(ResponseTemplate::new(200).set_body_string(STUB)).await;
    let service = service(&[&known]).await;
    service.add_mirror(manual.uri()).await.expect("зеркало");

    let list = service.check_mirrors().await;

    let active: Vec<_> = list.iter().filter(|m| m.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].url, base(&manual).to_string());
    assert_eq!(active[0].state, MirrorStateDto::NeedsLogin);
}

#[tokio::test]
async fn add_mirror_rejects_bad_urls_without_requests() {
    let server = site(login_ok()).await;
    let service = service(&[&server]).await;

    for raw in [
        "",
        "   ",
        "not a url",
        "http://",
        "ftp://example.com/",
        "file:///tmp/x",
        "mailto:a@b.c",
        "https://user:secret@example.com/",
    ] {
        let err = service
            .add_mirror(raw.to_string())
            .await
            .expect_err("ожидали InvalidInput");
        assert_eq!(err.kind, IpcErrorKind::InvalidInput, "{raw}");
        assert!(!err.message.contains("secret"), "{raw}");
    }
    assert_eq!(hits(&server).await, 0);
}

#[tokio::test]
async fn add_mirror_reports_challenge() {
    let anubis = answering(ResponseTemplate::new(200).set_body_string(ANUBIS_TITLE)).await;
    let service = service(&[]).await;

    let err = service
        .add_mirror(anubis.uri())
        .await
        .expect_err("ожидали Challenge");

    assert_eq!(err.kind, IpcErrorKind::Challenge);
    assert!(!err.message.is_empty());
    assert!(service.mirrors().await.is_empty());
}

#[tokio::test]
async fn add_mirror_reports_unreachable_mirror() {
    let broken = dead().await;
    let service = service(&[]).await;

    let err = service
        .add_mirror(broken.uri())
        .await
        .expect_err("ожидали ошибку");

    assert_eq!(err.kind, IpcErrorKind::Network);
}

#[tokio::test]
async fn add_mirror_activates_root_of_valid_url() {
    let server = answering(ResponseTemplate::new(200).set_body_string(STUB)).await;
    let service = service(&[]).await;

    let mirror = service
        .add_mirror(format!("{}/some/page?x=1#frag", server.uri()))
        .await
        .expect("зеркало");

    assert_eq!(mirror.url, base(&server).to_string());
    assert_eq!(mirror.state, MirrorStateDto::NeedsLogin);
    assert!(mirror.active);
    assert!(!mirror.standby);
    assert_eq!(service.pool.store().active().await, Some(base(&server)));
    assert_eq!(
        service.mirrors().await,
        vec![MirrorDto::unknown(&base(&server), true)]
    );
}

#[tokio::test]
async fn select_mirror_switches_active_and_drops_session() {
    let first = site(login_ok()).await;
    let second = site(login_ok()).await;
    let service = service(&[&first]).await;
    service
        .login(EMAIL.to_string(), PASSWORD.to_string())
        .await
        .expect("вход");
    assert_eq!(service.session(), Some(user()));

    let mirror = service.select_mirror(second.uri()).await.expect("зеркало");

    assert!(mirror.active);
    assert_eq!(mirror.url, base(&second).to_string());
    assert_eq!(service.session(), None);
    assert_eq!(service.pool.store().active().await, Some(base(&second)));
    assert!(!last_request_has_cookie(&second).await);
}

#[tokio::test]
async fn failed_select_mirror_keeps_session() {
    let server = site(login_ok()).await;
    let broken = dead().await;
    let service = service(&[&server]).await;
    service
        .login(EMAIL.to_string(), PASSWORD.to_string())
        .await
        .expect("вход");

    let err = service
        .select_mirror(broken.uri())
        .await
        .expect_err("ожидали ошибку");

    assert_eq!(err.kind, IpcErrorKind::Network);
    assert_eq!(service.session(), Some(user()));
    assert_eq!(service.pool.store().active().await, Some(base(&server)));
}

#[tokio::test]
async fn password_never_reaches_dto_or_errors() {
    let ok = site(login_ok()).await;
    let service = service(&[&ok]).await;
    let signed_in = service
        .login(EMAIL.to_string(), PASSWORD.to_string())
        .await
        .expect("вход");
    assert_clean(&signed_in);
    assert_clean(&service.session());
    assert_clean(&service.mirrors().await);
    assert_clean(&service.check_mirrors().await);
    assert_clean(&BootState::SignedIn { user: signed_in });

    let echo = site(login_failed(&format!("Пароль {PASSWORD} неверный"))).await;
    let service = service_for_echo(&echo).await;
    let err = service
        .login(EMAIL.to_string(), PASSWORD.to_string())
        .await
        .expect_err("ожидали отказ");
    assert_eq!(err.kind, IpcErrorKind::LoginRejected);
    assert_clean(&err);
    assert!(!err.to_string().contains(PASSWORD));

    let boot = service_for_echo(&echo).await;
    boot.secrets.save(&creds()).await.expect("сохранение");
    let state = boot.boot().await;
    assert_clean(&state);
}

async fn service_for_echo(server: &MockServer) -> Service {
    service(&[server]).await
}

#[test]
fn ipc_error_is_an_error_and_displays_message() {
    let err = IpcError::new(IpcErrorKind::Internal, "сбой");
    let boxed: Box<dyn std::error::Error> = Box::new(err);
    assert_eq!(boxed.to_string(), "сбой");
}
