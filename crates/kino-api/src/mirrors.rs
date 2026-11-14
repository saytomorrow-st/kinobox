//! Пул зеркал HDrezka: порядок проб, ротация, пометка мёртвых зеркал.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_util::future::join_all;
use kino_core::{KinoError, Result};
use url::Url;

use crate::client::{ClientConfig, HttpClient, Response};

const DEFAULT_STANDBY: &str = "https://standby-rezka.tv/";

/// Зеркала по умолчанию, в порядке проб. Хост подтверждён заказчиком;
/// другие домены (например `rezka.si`) добавлять только после его подтверждения.
const DEFAULT_KNOWN: [&str; 2] = ["https://kvk.pub/", "https://rezka.fi/"];

/// Признаки страницы входа: если тело ответа 2xx содержит любую из этих строк,
/// зеркало живое, но требует входа (`MirrorState::NeedsLogin`). Обновлять при поломке.
pub const LOGIN_STUB_MARKERS: [&str; 2] = ["id=\"loginForm\"", "Вход на HDrezka"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorConfig {
    /// Известные зеркала, пробуются по очереди.
    pub known: Vec<Url>,
    /// Адрес, который должен редиректить на актуальное зеркало.
    pub standby: Url,
    /// Лимит на одну пробу.
    pub probe_timeout: Duration,
    /// Сколько зеркало, помеченное `mark_dead`, пропускается.
    pub dead_ttl: Duration,
}

impl MirrorConfig {
    /// Конфиг по умолчанию: известные зеркала `https://kvk.pub/` и `https://rezka.fi/`
    /// (в таком порядке), standby `https://standby-rezka.tv/`.
    pub fn new() -> Result<Self> {
        let parse = |raw: &str| {
            Url::parse(raw)
                .map_err(|err| KinoError::Parse(format!("адрес по умолчанию {raw}: {err}")))
        };
        let known = DEFAULT_KNOWN
            .iter()
            .map(|raw| parse(raw))
            .collect::<Result<Vec<_>>>()?;
        let standby = parse(DEFAULT_STANDBY)?;
        Ok(Self {
            known,
            standby,
            probe_timeout: Duration::from_secs(5),
            dead_ttl: Duration::from_secs(60),
        })
    }
}

/// Состояние зеркала по результату пробы главной страницы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorState {
    /// 2xx, непустое тело, формы входа нет.
    Open,
    /// 2xx и страница входа. Зеркало живое, но сначала нужен вход.
    NeedsLogin,
    /// Защитный челлендж (Anubis). Пропускается, PoW не реализован.
    Challenge,
    /// Сеть, таймаут, не-2xx или пустое тело.
    Dead,
}

impl MirrorState {
    /// `true` для `Open` и `NeedsLogin`.
    pub fn is_usable(self) -> bool {
        matches!(self, Self::Open | Self::NeedsLogin)
    }
}

/// Результат пробы одного зеркала.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorStatus {
    pub url: Url,
    pub state: MirrorState,
    /// Пояснение к состоянию; у `Open` обычно `None`.
    pub reason: Option<String>,
    /// Время до получения ответа; `None`, если ответа не было.
    pub latency: Option<Duration>,
}

impl MirrorStatus {
    /// Зеркало можно использовать: `Open` или `NeedsLogin`.
    pub fn is_usable(&self) -> bool {
        self.state.is_usable()
    }
}

/// Выбранное зеркало и его состояние: по нему вызывающий знает, нужен ли вход.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMirror {
    pub url: Url,
    /// Всегда `Open` или `NeedsLogin`.
    pub state: MirrorState,
}

/// Хранилище активного зеркала. SQLite-реализация придёт из `kino-store` (M2).
pub trait MirrorStore: Send + Sync {
    fn active(&self) -> impl Future<Output = Option<Url>> + Send;
    fn set_active(&self, url: Url) -> impl Future<Output = ()> + Send;
    fn clear_active(&self) -> impl Future<Output = ()> + Send;
}

#[derive(Debug, Default)]
pub struct MemoryMirrorStore {
    active: Mutex<Option<Url>>,
}

impl MemoryMirrorStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_active(url: Url) -> Self {
        Self {
            active: Mutex::new(Some(url)),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Url>> {
        self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl MirrorStore for MemoryMirrorStore {
    async fn active(&self) -> Option<Url> {
        self.lock().clone()
    }

    async fn set_active(&self, url: Url) {
        *self.lock() = Some(url);
    }

    async fn clear_active(&self) {
        *self.lock() = None;
    }
}

struct Probe {
    status: MirrorStatus,
    /// Адрес ответа после редиректов; `None`, если ответа не было.
    final_url: Option<Url>,
    error: Option<KinoError>,
}

pub struct MirrorPool<S: MirrorStore> {
    probe_client: HttpClient,
    store: S,
    config: MirrorConfig,
    dead: Mutex<HashMap<String, Instant>>,
}

impl<S: MirrorStore> MirrorPool<S> {
    /// Пробы идут через отдельный клиент (без retry, с `probe_timeout`) с тем же `User-Agent`,
    /// что у `client`; cookie проб не смешиваются с основным клиентом.
    pub fn new(client: &HttpClient, store: S, config: MirrorConfig) -> Result<Self> {
        let probe_client = HttpClient::new(ClientConfig {
            timeout: config.probe_timeout,
            max_retries: 0,
            ..client.config().clone()
        })?;
        Ok(Self {
            probe_client,
            store,
            config,
            dead: Mutex::new(HashMap::new()),
        })
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn config(&self) -> &MirrorConfig {
        &self.config
    }

    /// Идёт по порядку: активное, известные, standby. Первое пригодное (`Open` или `NeedsLogin`)
    /// становится активным; `Challenge` и `Dead` пропускаются. Порядок `known` задаёт пользователь,
    /// поэтому `Open` позже `NeedsLogin` в списке не предпочитается.
    pub async fn resolve(&self) -> Result<ResolvedMirror> {
        let mut tried: Vec<String> = Vec::new();

        if let Some(active) = self.store.active().await {
            let key = mirror_key(&active);
            if !self.is_dead(&key) {
                tried.push(key);
                let status = self.run_probe(&active).await.status;
                if status.is_usable() {
                    return Ok(ResolvedMirror {
                        url: active,
                        state: status.state,
                    });
                }
            }
        }

        for url in &self.config.known {
            let key = mirror_key(url);
            if tried.contains(&key) || self.is_dead(&key) {
                continue;
            }
            tried.push(key);
            let status = self.run_probe(url).await.status;
            if status.is_usable() {
                self.store.set_active(url.clone()).await;
                return Ok(ResolvedMirror {
                    url: url.clone(),
                    state: status.state,
                });
            }
        }

        let (standby, redirected) = self.probe_standby_inner().await;
        if redirected && standby.is_usable() && !self.is_dead(&mirror_key(&standby.url)) {
            self.store.set_active(standby.url.clone()).await;
            return Ok(ResolvedMirror {
                url: standby.url,
                state: standby.state,
            });
        }

        Err(KinoError::AllMirrorsUnavailable)
    }

    /// Параллельно пробует все известные зеркала; порядок результата совпадает с `known`.
    pub async fn health_check(&self) -> Vec<MirrorStatus> {
        let probes = join_all(self.config.known.iter().map(|url| self.run_probe(url))).await;
        probes.into_iter().map(|probe| probe.status).collect()
    }

    /// Пробует standby. Зеркалом он считается, только если редиректит на другой хост;
    /// тогда `url` в статусе — корень актуального зеркала. Без редиректа `url` — сам standby,
    /// `state` отражает ответ (например `NeedsLogin`), но `resolve` такой standby не использует.
    pub async fn probe_standby(&self) -> MirrorStatus {
        self.probe_standby_inner().await.0
    }

    /// Статус standby и признак того, что он редиректит на другой хост.
    async fn probe_standby_inner(&self) -> (MirrorStatus, bool) {
        let probe = self.run_probe(&self.config.standby).await;
        let Some(final_url) = probe.final_url else {
            return (probe.status, false);
        };
        if same_authority(&final_url, &self.config.standby) {
            let reason = match (probe.status.state, probe.status.reason) {
                (MirrorState::Open | MirrorState::NeedsLogin, _) | (_, None) => {
                    "standby без редиректа на зеркало".to_string()
                }
                (_, Some(inner)) => format!("standby без редиректа на зеркало ({inner})"),
            };
            let status = MirrorStatus {
                url: self.config.standby.clone(),
                state: probe.status.state,
                reason: Some(reason),
                latency: probe.status.latency,
            };
            return (status, false);
        }
        let status = MirrorStatus {
            url: mirror_root(&final_url),
            ..probe.status
        };
        (status, true)
    }

    /// Пробует адрес, введённый пользователем. Пригодный (`Open` или `NeedsLogin`) становится
    /// активным. `Challenge` даёт `Err(KinoError::Challenge)`, прочее непригодное даёт ошибку
    /// пробы; активное при этом не меняется.
    pub async fn add_manual(&self, url: Url) -> Result<ResolvedMirror> {
        let probe = self.run_probe(&url).await;
        if !probe.status.is_usable() {
            return Err(probe.error.unwrap_or(KinoError::AllMirrorsUnavailable));
        }
        self.lock_dead().remove(&mirror_key(&url));
        self.store.set_active(url.clone()).await;
        Ok(ResolvedMirror {
            url,
            state: probe.status.state,
        })
    }

    /// Помечает зеркало мёртвым на `dead_ttl`. Если оно было активным, активное сбрасывается,
    /// и следующий `resolve` ротирует на другое зеркало.
    pub async fn mark_dead(&self, url: &Url) {
        let key = mirror_key(url);
        self.lock_dead()
            .insert(key.clone(), Instant::now() + self.config.dead_ttl);
        if let Some(active) = self.store.active().await {
            if mirror_key(&active) == key {
                self.store.clear_active().await;
            }
        }
    }

    fn lock_dead(&self) -> std::sync::MutexGuard<'_, HashMap<String, Instant>> {
        self.dead.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_dead(&self, key: &str) -> bool {
        let mut dead = self.lock_dead();
        match dead.get(key) {
            Some(until) if Instant::now() < *until => true,
            Some(_) => {
                dead.remove(key);
                false
            }
            None => false,
        }
    }

    async fn run_probe(&self, url: &Url) -> Probe {
        let started = Instant::now();
        let outcome = self.probe_client.get(url).await;
        let elapsed = started.elapsed();

        let response = match outcome {
            Ok(response) => response,
            Err(err) => {
                let state = if err == KinoError::Challenge {
                    MirrorState::Challenge
                } else {
                    MirrorState::Dead
                };
                return Probe::failed(url, state, describe_error(&err), err);
            }
        };

        let final_url = response.final_url.clone();
        let (state, reason) = judge(&response);
        let error = match (state, &reason) {
            (MirrorState::Dead, Some(reason)) => Some(KinoError::Parse(reason.clone())),
            _ => None,
        };
        Probe {
            status: MirrorStatus {
                url: url.clone(),
                state,
                reason,
                latency: Some(elapsed),
            },
            final_url: Some(final_url),
            error,
        }
    }
}

impl Probe {
    fn failed(url: &Url, state: MirrorState, reason: String, error: KinoError) -> Self {
        Self {
            status: MirrorStatus {
                url: url.clone(),
                state,
                reason: Some(reason),
                latency: None,
            },
            final_url: None,
            error: Some(error),
        }
    }
}

fn describe_error(err: &KinoError) -> String {
    match err {
        KinoError::Challenge => "защитный челлендж (Anubis), зеркало пропущено".to_string(),
        KinoError::Http { status } => format!("HTTP {status}"),
        other => other.to_string(),
    }
}

/// Состояние по полученному ответу и пояснение. `Challenge` сюда не доходит:
/// его определяет `HttpClient` и возвращает как ошибку.
fn judge(response: &Response) -> (MirrorState, Option<String>) {
    if !(200..300).contains(&response.status) {
        return (MirrorState::Dead, Some(format!("HTTP {}", response.status)));
    }
    let body = response.text();
    if body.trim().is_empty() {
        return (MirrorState::Dead, Some("пустой ответ".to_string()));
    }
    if LOGIN_STUB_MARKERS
        .iter()
        .any(|marker| body.contains(marker))
    {
        return (MirrorState::NeedsLogin, Some("требуется вход".to_string()));
    }
    (MirrorState::Open, None)
}

fn mirror_key(url: &Url) -> String {
    format!(
        "{}://{}:{}",
        url.scheme(),
        url.host_str().unwrap_or_default(),
        url.port_or_known_default().unwrap_or_default()
    )
}

fn same_authority(a: &Url, b: &Url) -> bool {
    a.host_str() == b.host_str() && a.port() == b.port()
}

fn mirror_root(url: &Url) -> Url {
    let mut root = url.clone();
    root.set_path("/");
    root.set_query(None);
    root.set_fragment(None);
    root
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const LIVE_BODY: &str = "<html><body>главная страница</body></html>";
    const STUB_BODY: &str = "<form id=\"loginForm\" action=\"/ajax/login/\"></form>";
    const ANUBIS_COOKIE: &str = "techaro.lol-anubis-auth-d82da6a5=; Path=/";
    const ANUBIS_TITLE: &str = "<html><head><title>Проверяем, что вы не бот!</title></head></html>";

    async fn server_with(template: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&server)
            .await;
        server
    }

    async fn live() -> MockServer {
        server_with(ResponseTemplate::new(200).set_body_string(LIVE_BODY)).await
    }

    async fn failing() -> MockServer {
        server_with(ResponseTemplate::new(503)).await
    }

    async fn login_stub() -> MockServer {
        server_with(ResponseTemplate::new(200).set_body_string(STUB_BODY)).await
    }

    async fn anubis_cookie() -> MockServer {
        server_with(
            ResponseTemplate::new(200)
                .insert_header("Set-Cookie", ANUBIS_COOKIE)
                .set_body_string(LIVE_BODY),
        )
        .await
    }

    async fn anubis_title() -> MockServer {
        server_with(ResponseTemplate::new(200).set_body_string(ANUBIS_TITLE)).await
    }

    async fn redirecting_to(target: &MockServer) -> MockServer {
        server_with(
            ResponseTemplate::new(301).insert_header("Location", format!("{}/", target.uri())),
        )
        .await
    }

    fn base(server: &MockServer) -> Url {
        Url::parse(&format!("{}/", server.uri())).expect("корректный url")
    }

    fn resolved(server: &MockServer, state: MirrorState) -> ResolvedMirror {
        ResolvedMirror {
            url: base(server),
            state,
        }
    }

    async fn hits(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .map(|requests| requests.len())
            .unwrap_or_default()
    }

    fn config(known: &[&MockServer], standby: &MockServer) -> MirrorConfig {
        MirrorConfig {
            known: known.iter().map(|server| base(server)).collect(),
            standby: base(standby),
            probe_timeout: Duration::from_secs(2),
            dead_ttl: Duration::from_millis(400),
        }
    }

    fn pool(store: MemoryMirrorStore, config: MirrorConfig) -> MirrorPool<MemoryMirrorStore> {
        let client = HttpClient::new(ClientConfig::default()).expect("клиент");
        MirrorPool::new(&client, store, config).expect("пул")
    }

    async fn state_of(server: &MockServer) -> MirrorState {
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[server], &standby));
        pool.health_check().await[0].state
    }

    #[test]
    fn default_config_matches_contract() {
        let config = MirrorConfig::new().expect("конфиг");
        let known: Vec<&str> = config.known.iter().map(Url::as_str).collect();
        assert_eq!(known, vec!["https://kvk.pub/", "https://rezka.fi/"]);
        assert_eq!(config.standby.as_str(), DEFAULT_STANDBY);
        assert_eq!(config.probe_timeout, Duration::from_secs(5));
        assert_eq!(config.dead_ttl, Duration::from_secs(60));
    }

    #[test]
    fn usability_follows_state() {
        assert!(MirrorState::Open.is_usable());
        assert!(MirrorState::NeedsLogin.is_usable());
        assert!(!MirrorState::Challenge.is_usable());
        assert!(!MirrorState::Dead.is_usable());
    }

    #[tokio::test]
    async fn plain_page_is_open() {
        let server = live().await;
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[&server], &standby));

        let statuses = pool.health_check().await;
        assert_eq!(statuses[0].state, MirrorState::Open);
        assert!(statuses[0].is_usable());
        assert!(statuses[0].reason.is_none());
        assert!(statuses[0].latency.is_some());
    }

    #[tokio::test]
    async fn login_page_is_needs_login_and_resolve_accepts_it() {
        let stub = login_stub().await;
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[&stub], &standby));

        let statuses = pool.health_check().await;
        assert_eq!(statuses[0].state, MirrorState::NeedsLogin);
        assert!(statuses[0].is_usable());
        assert_eq!(statuses[0].reason.as_deref(), Some("требуется вход"));

        assert_eq!(
            pool.resolve().await,
            Ok(resolved(&stub, MirrorState::NeedsLogin))
        );
        assert_eq!(pool.store().active().await, Some(base(&stub)));
    }

    #[tokio::test]
    async fn login_title_marker_is_needs_login() {
        let server = server_with(
            ResponseTemplate::new(200)
                .set_body_string("<title>Вход на HDrezka</title><p>войдите</p>"),
        )
        .await;
        assert_eq!(state_of(&server).await, MirrorState::NeedsLogin);
    }

    #[tokio::test]
    async fn anubis_cookie_is_challenge_and_resolve_skips_it() {
        let guarded = anubis_cookie().await;
        let good = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::new(),
            config(&[&guarded, &good], &standby),
        );

        let statuses = pool.health_check().await;
        assert_eq!(statuses[0].state, MirrorState::Challenge);
        assert!(!statuses[0].is_usable());
        assert!(statuses[0].latency.is_none());
        let reason = statuses[0].reason.clone().expect("причина");
        assert!(reason.contains("челлендж"), "{reason}");

        assert_eq!(pool.resolve().await, Ok(resolved(&good, MirrorState::Open)));
        assert_eq!(pool.store().active().await, Some(base(&good)));
    }

    #[tokio::test]
    async fn anubis_title_is_challenge() {
        let server = anubis_title().await;
        assert_eq!(state_of(&server).await, MirrorState::Challenge);
    }

    #[tokio::test]
    async fn anubis_in_403_is_challenge() {
        let server = server_with(
            ResponseTemplate::new(403)
                .set_body_string("<title>Making sure you're not a bot!</title>"),
        )
        .await;
        assert_eq!(state_of(&server).await, MirrorState::Challenge);
    }

    #[tokio::test]
    async fn http_503_is_dead() {
        let server = failing().await;
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[&server], &standby));

        let statuses = pool.health_check().await;
        assert_eq!(statuses[0].state, MirrorState::Dead);
        assert_eq!(statuses[0].reason.as_deref(), Some("HTTP 503"));
    }

    #[tokio::test]
    async fn timeout_is_dead_with_timeout_reason() {
        let slow =
            server_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(800))).await;
        let standby = failing().await;
        let mut config = config(&[&slow], &standby);
        config.probe_timeout = Duration::from_millis(100);
        let pool = pool(MemoryMirrorStore::new(), config);

        let statuses = pool.health_check().await;
        assert_eq!(statuses[0].state, MirrorState::Dead);
        let reason = statuses[0].reason.clone().expect("причина");
        assert!(reason.contains("таймаут"), "{reason}");
    }

    #[tokio::test]
    async fn empty_body_is_dead() {
        let empty = server_with(ResponseTemplate::new(200)).await;
        assert_eq!(state_of(&empty).await, MirrorState::Dead);
    }

    #[tokio::test]
    async fn live_active_is_returned_without_walking_list() {
        let active = live().await;
        let other = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&active)),
            config(&[&other], &standby),
        );

        assert_eq!(
            pool.resolve().await,
            Ok(resolved(&active, MirrorState::Open))
        );
        assert_eq!(hits(&other).await, 0);
        assert_eq!(hits(&standby).await, 0);
    }

    #[tokio::test]
    async fn active_needing_login_keeps_its_state() {
        let active = login_stub().await;
        let other = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&active)),
            config(&[&other], &standby),
        );

        assert_eq!(
            pool.resolve().await,
            Ok(resolved(&active, MirrorState::NeedsLogin))
        );
        assert_eq!(hits(&other).await, 0);
    }

    #[tokio::test]
    async fn dead_active_falls_back_to_first_usable_known() {
        let active = failing().await;
        let dead_known = failing().await;
        let challenged = anubis_cookie().await;
        let good = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&active)),
            config(&[&dead_known, &challenged, &good], &standby),
        );

        assert_eq!(pool.resolve().await, Ok(resolved(&good, MirrorState::Open)));
        assert_eq!(pool.store().active().await, Some(base(&good)));
        assert_eq!(hits(&standby).await, 0);
    }

    #[tokio::test]
    async fn known_order_wins_over_better_state() {
        let login = login_stub().await;
        let open = live().await;
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[&login, &open], &standby));

        assert_eq!(
            pool.resolve().await,
            Ok(resolved(&login, MirrorState::NeedsLogin))
        );
        assert_eq!(pool.store().active().await, Some(base(&login)));
        assert_eq!(hits(&open).await, 0);
    }

    #[tokio::test]
    async fn standby_redirect_to_live_mirror_becomes_active() {
        let mirror = live().await;
        let standby = redirecting_to(&mirror).await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(
            pool.resolve().await,
            Ok(resolved(&mirror, MirrorState::Open))
        );
        assert_eq!(pool.store().active().await, Some(base(&mirror)));
    }

    #[tokio::test]
    async fn standby_redirect_to_login_page_is_needs_login_mirror() {
        let stub = login_stub().await;
        let standby = redirecting_to(&stub).await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(
            pool.resolve().await,
            Ok(resolved(&stub, MirrorState::NeedsLogin))
        );
        assert_eq!(pool.store().active().await, Some(base(&stub)));
    }

    #[tokio::test]
    async fn standby_redirect_to_challenge_is_skipped() {
        let guarded = anubis_cookie().await;
        let standby = redirecting_to(&guarded).await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
        assert_eq!(pool.store().active().await, None);
    }

    #[tokio::test]
    async fn standby_without_redirect_login_page_is_not_a_mirror() {
        let standby = login_stub().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        let status = pool.probe_standby().await;
        assert_eq!(status.state, MirrorState::NeedsLogin);
        assert_eq!(status.url, base(&standby));
        assert_eq!(
            status.reason.as_deref(),
            Some("standby без редиректа на зеркало")
        );

        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
        assert_eq!(pool.store().active().await, None);
    }

    #[tokio::test]
    async fn standby_without_redirect_is_skipped_even_with_open_page() {
        let standby = live().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
        assert_eq!(pool.store().active().await, None);
    }

    #[tokio::test]
    async fn empty_known_and_unavailable_standby() {
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
    }

    #[tokio::test]
    async fn mark_dead_rotates_to_next_mirror() {
        let first = live().await;
        let second = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&first)),
            config(&[&first, &second], &standby),
        );

        pool.mark_dead(&base(&first)).await;
        assert_eq!(pool.store().active().await, None);
        assert_eq!(
            pool.resolve().await,
            Ok(resolved(&second, MirrorState::Open))
        );
        assert_eq!(hits(&first).await, 0);
    }

    #[tokio::test]
    async fn dead_mirror_is_skipped_until_ttl_then_probed_again() {
        let only = live().await;
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[&only], &standby));

        pool.mark_dead(&base(&only)).await;
        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
        assert_eq!(hits(&only).await, 0);

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(pool.resolve().await, Ok(resolved(&only, MirrorState::Open)));
        assert_eq!(hits(&only).await, 1);
    }

    #[tokio::test]
    async fn mark_dead_keeps_other_active() {
        let first = live().await;
        let second = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&second)),
            config(&[&first, &second], &standby),
        );

        pool.mark_dead(&base(&first)).await;
        assert_eq!(pool.store().active().await, Some(base(&second)));
    }

    #[tokio::test]
    async fn add_manual_open_becomes_active() {
        let previous = live().await;
        let manual = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&previous)),
            config(&[], &standby),
        );

        assert_eq!(
            pool.add_manual(base(&manual)).await,
            Ok(resolved(&manual, MirrorState::Open))
        );
        assert_eq!(pool.store().active().await, Some(base(&manual)));
    }

    #[tokio::test]
    async fn add_manual_needs_login_is_accepted() {
        let previous = live().await;
        let manual = login_stub().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&previous)),
            config(&[], &standby),
        );

        assert_eq!(
            pool.add_manual(base(&manual)).await,
            Ok(resolved(&manual, MirrorState::NeedsLogin))
        );
        assert_eq!(pool.store().active().await, Some(base(&manual)));
    }

    #[tokio::test]
    async fn add_manual_challenge_keeps_active() {
        let previous = live().await;
        let by_cookie = anubis_cookie().await;
        let by_title = anubis_title().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&previous)),
            config(&[], &standby),
        );

        assert_eq!(
            pool.add_manual(base(&by_cookie)).await,
            Err(KinoError::Challenge)
        );
        assert_eq!(
            pool.add_manual(base(&by_title)).await,
            Err(KinoError::Challenge)
        );
        assert_eq!(pool.store().active().await, Some(base(&previous)));
    }

    #[tokio::test]
    async fn add_manual_dead_keeps_active_and_returns_error() {
        let previous = live().await;
        let broken = failing().await;
        let empty = server_with(ResponseTemplate::new(200)).await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&previous)),
            config(&[], &standby),
        );

        assert_eq!(
            pool.add_manual(base(&broken)).await,
            Err(KinoError::Http { status: 503 })
        );
        assert!(matches!(
            pool.add_manual(base(&empty)).await,
            Err(KinoError::Parse(reason)) if reason.contains("пустой")
        ));
        assert_eq!(pool.store().active().await, Some(base(&previous)));
    }

    #[tokio::test]
    async fn health_check_reports_every_state_in_order() {
        let first = live().await;
        let second = login_stub().await;
        let third = anubis_cookie().await;
        let fourth = failing().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::new(),
            config(&[&first, &second, &third, &fourth], &standby),
        );

        let statuses = pool.health_check().await;
        let urls: Vec<Url> = statuses.iter().map(|status| status.url.clone()).collect();
        assert_eq!(
            urls,
            vec![base(&first), base(&second), base(&third), base(&fourth)]
        );
        let states: Vec<MirrorState> = statuses.iter().map(|status| status.state).collect();
        assert_eq!(
            states,
            vec![
                MirrorState::Open,
                MirrorState::NeedsLogin,
                MirrorState::Challenge,
                MirrorState::Dead
            ]
        );
        assert!(statuses[0].latency.is_some());
        assert!(statuses[0].reason.is_none());
        assert_eq!(statuses[3].reason.as_deref(), Some("HTTP 503"));
        assert_eq!(hits(&standby).await, 0);
        assert_eq!(pool.store().active().await, None);
    }
}
