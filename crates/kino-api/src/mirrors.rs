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

/// Признаки страницы-заглушки входа: если тело ответа содержит любую из этих строк,
/// зеркало считается мёртвым, даже при статусе 200. Обновлять при поломке.
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
    /// Конфиг по умолчанию: без известных зеркал, standby `https://standby-rezka.tv/`.
    pub fn new() -> Result<Self> {
        let standby = Url::parse(DEFAULT_STANDBY)
            .map_err(|err| KinoError::Parse(format!("standby по умолчанию: {err}")))?;
        Ok(Self {
            known: Vec::new(),
            standby,
            probe_timeout: Duration::from_secs(5),
            dead_ttl: Duration::from_secs(60),
        })
    }
}

/// Результат пробы одного зеркала.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorStatus {
    pub url: Url,
    pub alive: bool,
    /// Почему зеркало признано мёртвым; у живого `None`.
    pub reason: Option<String>,
    /// Время до получения ответа; `None`, если ответа не было.
    pub latency: Option<Duration>,
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

    /// Идёт по порядку: активное, известные, standby. Первое живое становится активным.
    pub async fn resolve(&self) -> Result<Url> {
        let mut tried: Vec<String> = Vec::new();

        if let Some(active) = self.store.active().await {
            let key = mirror_key(&active);
            if !self.is_dead(&key) {
                tried.push(key);
                if self.run_probe(&active).await.status.alive {
                    return Ok(active);
                }
            }
        }

        for url in &self.config.known {
            let key = mirror_key(url);
            if tried.contains(&key) || self.is_dead(&key) {
                continue;
            }
            tried.push(key);
            if self.run_probe(url).await.status.alive {
                self.store.set_active(url.clone()).await;
                return Ok(url.clone());
            }
        }

        let standby = self.probe_standby().await;
        if standby.alive && !self.is_dead(&mirror_key(&standby.url)) {
            self.store.set_active(standby.url.clone()).await;
            return Ok(standby.url);
        }

        Err(KinoError::AllMirrorsUnavailable)
    }

    /// Параллельно пробует все известные зеркала; порядок результата совпадает с `known`.
    pub async fn health_check(&self) -> Vec<MirrorStatus> {
        let probes = join_all(self.config.known.iter().map(|url| self.run_probe(url))).await;
        probes.into_iter().map(|probe| probe.status).collect()
    }

    /// Пробует standby. Зеркалом он считается, только если редиректит на другой хост;
    /// тогда `url` в статусе — корень актуального зеркала.
    pub async fn probe_standby(&self) -> MirrorStatus {
        let probe = self.run_probe(&self.config.standby).await;
        let Some(final_url) = probe.final_url else {
            return probe.status;
        };
        if same_authority(&final_url, &self.config.standby) {
            let reason = match probe.status.reason {
                Some(inner) => format!("standby без редиректа на зеркало ({inner})"),
                None => "standby без редиректа на зеркало".to_string(),
            };
            return MirrorStatus {
                url: self.config.standby.clone(),
                alive: false,
                reason: Some(reason),
                latency: probe.status.latency,
            };
        }
        MirrorStatus {
            url: mirror_root(&final_url),
            ..probe.status
        }
    }

    /// Пробует адрес, введённый пользователем. Живой становится активным;
    /// недоступный возвращает ошибку и не меняет активное.
    pub async fn add_manual(&self, url: Url) -> Result<Url> {
        let probe = self.run_probe(&url).await;
        if !probe.status.alive {
            return Err(probe.error.unwrap_or(KinoError::AllMirrorsUnavailable));
        }
        self.lock_dead().remove(&mirror_key(&url));
        self.store.set_active(url.clone()).await;
        Ok(url)
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
            Err(err) => return Probe::failed(url, describe_error(&err), err),
        };

        let final_url = response.final_url.clone();
        let (alive, reason, error) = match judge(&response) {
            Some(reason) => (false, Some(reason.clone()), Some(KinoError::Parse(reason))),
            None => (true, None, None),
        };
        Probe {
            status: MirrorStatus {
                url: url.clone(),
                alive,
                reason,
                latency: Some(elapsed),
            },
            final_url: Some(final_url),
            error,
        }
    }
}

impl Probe {
    fn failed(url: &Url, reason: String, error: KinoError) -> Self {
        Self {
            status: MirrorStatus {
                url: url.clone(),
                alive: false,
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
        KinoError::Challenge => "защитный челлендж (Anubis), зеркало недоступно".to_string(),
        KinoError::Http { status } => format!("HTTP {status}"),
        other => other.to_string(),
    }
}

/// `None`, если ответ похож на живое зеркало; иначе причина.
fn judge(response: &Response) -> Option<String> {
    if !(200..300).contains(&response.status) {
        return Some(format!("HTTP {}", response.status));
    }
    let body = response.text();
    if body.trim().is_empty() {
        return Some("пустой ответ".to_string());
    }
    if LOGIN_STUB_MARKERS
        .iter()
        .any(|marker| body.contains(marker))
    {
        return Some("страница-заглушка входа".to_string());
    }
    None
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

    async fn redirecting_to(target: &MockServer) -> MockServer {
        server_with(
            ResponseTemplate::new(301).insert_header("Location", format!("{}/", target.uri())),
        )
        .await
    }

    fn base(server: &MockServer) -> Url {
        Url::parse(&format!("{}/", server.uri())).expect("корректный url")
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

    #[test]
    fn default_config_matches_contract() {
        let config = MirrorConfig::new().expect("конфиг");
        assert!(config.known.is_empty());
        assert_eq!(config.standby.as_str(), DEFAULT_STANDBY);
        assert_eq!(config.probe_timeout, Duration::from_secs(5));
        assert_eq!(config.dead_ttl, Duration::from_secs(60));
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

        assert_eq!(pool.resolve().await, Ok(base(&active)));
        assert_eq!(hits(&other).await, 0);
        assert_eq!(hits(&standby).await, 0);
    }

    #[tokio::test]
    async fn dead_active_falls_back_to_first_live_known() {
        let active = failing().await;
        let dead_known = failing().await;
        let good = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&active)),
            config(&[&dead_known, &good], &standby),
        );

        assert_eq!(pool.resolve().await, Ok(base(&good)));
        assert_eq!(pool.store().active().await, Some(base(&good)));
        assert_eq!(hits(&standby).await, 0);
    }

    #[tokio::test]
    async fn login_stub_is_dead_and_next_mirror_is_taken() {
        let stub = login_stub().await;
        let good = live().await;
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[&stub, &good], &standby));

        assert_eq!(pool.resolve().await, Ok(base(&good)));
        let statuses = pool.health_check().await;
        assert!(!statuses[0].alive);
        assert!(statuses[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("заглушка")));
    }

    #[tokio::test]
    async fn standby_redirect_to_live_mirror_becomes_active() {
        let mirror = live().await;
        let standby = redirecting_to(&mirror).await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(pool.resolve().await, Ok(base(&mirror)));
        assert_eq!(pool.store().active().await, Some(base(&mirror)));
    }

    #[tokio::test]
    async fn standby_redirect_to_login_stub_is_not_a_mirror() {
        let stub = login_stub().await;
        let standby = redirecting_to(&stub).await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
        assert_eq!(pool.store().active().await, None);
    }

    #[tokio::test]
    async fn standby_without_redirect_is_skipped() {
        let standby = login_stub().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
        assert_eq!(pool.store().active().await, None);

        let status = pool.probe_standby().await;
        assert!(!status.alive);
        assert_eq!(status.url, base(&standby));
        let reason = status.reason.expect("причина");
        assert!(reason.contains("без редиректа"), "{reason}");
        assert!(reason.contains("заглушка"), "{reason}");
    }

    #[tokio::test]
    async fn standby_without_redirect_is_skipped_even_with_live_page() {
        let standby = live().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
    }

    #[tokio::test]
    async fn empty_known_and_unavailable_standby() {
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[], &standby));

        assert_eq!(pool.resolve().await, Err(KinoError::AllMirrorsUnavailable));
    }

    #[tokio::test]
    async fn slow_mirror_is_dead_with_timeout_reason() {
        let slow =
            server_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(800))).await;
        let standby = failing().await;
        let mut config = config(&[&slow], &standby);
        config.probe_timeout = Duration::from_millis(100);
        let pool = pool(MemoryMirrorStore::new(), config);

        let statuses = pool.health_check().await;
        assert!(!statuses[0].alive);
        let reason = statuses[0].reason.clone().expect("причина");
        assert!(reason.contains("таймаут"), "{reason}");
    }

    #[tokio::test]
    async fn challenge_is_dead_with_reason() {
        let guarded = server_with(
            ResponseTemplate::new(403)
                .set_body_string("<title>Making sure you're not a bot!</title>"),
        )
        .await;
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[&guarded], &standby));

        let statuses = pool.health_check().await;
        assert!(!statuses[0].alive);
        let reason = statuses[0].reason.clone().expect("причина");
        assert!(reason.contains("челлендж"), "{reason}");
        assert_eq!(
            pool.add_manual(base(&guarded)).await,
            Err(KinoError::Challenge)
        );
    }

    #[tokio::test]
    async fn empty_body_is_dead() {
        let empty = server_with(ResponseTemplate::new(200)).await;
        let standby = failing().await;
        let pool = pool(MemoryMirrorStore::new(), config(&[&empty], &standby));

        let statuses = pool.health_check().await;
        assert!(!statuses[0].alive);
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
        assert_eq!(pool.resolve().await, Ok(base(&second)));
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
        assert_eq!(pool.resolve().await, Ok(base(&only)));
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
    async fn add_manual_live_becomes_active() {
        let previous = live().await;
        let manual = live().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::with_active(base(&previous)),
            config(&[], &standby),
        );

        assert_eq!(pool.add_manual(base(&manual)).await, Ok(base(&manual)));
        assert_eq!(pool.store().active().await, Some(base(&manual)));
    }

    #[tokio::test]
    async fn add_manual_dead_keeps_active_and_returns_error() {
        let previous = live().await;
        let broken = failing().await;
        let stub = login_stub().await;
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
            pool.add_manual(base(&stub)).await,
            Err(KinoError::Parse(reason)) if reason.contains("заглушка")
        ));
        assert_eq!(pool.store().active().await, Some(base(&previous)));
    }

    #[tokio::test]
    async fn health_check_reports_every_known_in_order() {
        let first = live().await;
        let second = failing().await;
        let third = login_stub().await;
        let standby = failing().await;
        let pool = pool(
            MemoryMirrorStore::new(),
            config(&[&first, &second, &third], &standby),
        );

        let statuses = pool.health_check().await;
        let urls: Vec<Url> = statuses.iter().map(|status| status.url.clone()).collect();
        assert_eq!(urls, vec![base(&first), base(&second), base(&third)]);
        let alive: Vec<bool> = statuses.iter().map(|status| status.alive).collect();
        assert_eq!(alive, vec![true, false, false]);
        assert!(statuses[0].latency.is_some());
        assert!(statuses[0].reason.is_none());
        assert_eq!(statuses[1].reason.as_deref(), Some("HTTP 503"));
        assert_eq!(hits(&standby).await, 0);
    }
}
