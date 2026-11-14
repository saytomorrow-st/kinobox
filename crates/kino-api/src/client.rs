//! HTTP-клиент: таймаут, cookie, retry с экспоненциальным backoff и jitter.

use std::hash::{BuildHasher, RandomState};
use std::time::Duration;

use kino_core::{KinoError, Result};
use url::Url;

const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const MAX_BACKOFF_SHIFT: u32 = 16;
const MAX_REDIRECTS: usize = 10;
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
const CHALLENGE_MARKERS: [&str; 3] = [
    "anubis",
    "making sure you're not a bot",
    "making sure you\u{2019}re not a bot",
];
const ANUBIS_COOKIE_PREFIX: &str = "techaro.lol-anubis";
const CHALLENGE_TITLES: [&str; 3] = [
    "проверяем, что вы не бот",
    "making sure you're not a bot",
    "making sure you\u{2019}re not a bot",
];
const TITLE_SCAN_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// Лимит на один запрос целиком (соединение, заголовки, тело).
    pub timeout: Duration,
    /// Сколько повторов делать после первой неудачной попытки.
    pub max_retries: u32,
    /// База экспоненциальной задержки: `base_delay * 2^попытка`.
    pub base_delay: Duration,
    pub user_agent: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
            max_retries: 3,
            base_delay: Duration::from_millis(300),
            user_agent: DEFAULT_USER_AGENT.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Сколько запросов отправлено, включая успешный (минимум 1).
    pub attempts: u32,
    /// Адрес, с которого получен ответ, после всех редиректов.
    pub final_url: Url,
}

impl Response {
    /// Тело как UTF-8; некорректные байты заменяются на `U+FFFD`.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Значение заголовка без учёта регистра имени (первое, если их несколько).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Clone)]
pub struct HttpClient {
    inner: wreq::Client,
    config: ClientConfig,
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClient")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

struct Failure {
    error: KinoError,
    retry_after: Option<Duration>,
}

impl From<KinoError> for Failure {
    fn from(error: KinoError) -> Self {
        Self {
            error,
            retry_after: None,
        }
    }
}

impl HttpClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let inner = wreq::Client::builder()
            .timeout(config.timeout)
            .user_agent(config.user_agent.as_str())
            .cookie_store(true)
            .redirect(wreq::redirect::Policy::limited(MAX_REDIRECTS))
            .retry(wreq::retry::Policy::never())
            .build()
            .map_err(|err| KinoError::Network(format!("не удалось создать HTTP-клиент: {err}")))?;
        Ok(Self { inner, config })
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub async fn get(&self, url: &Url) -> Result<Response> {
        self.execute(url, None).await
    }

    pub async fn post_form(&self, url: &Url, form: &[(&str, &str)]) -> Result<Response> {
        self.execute(url, Some(form)).await
    }

    async fn execute(&self, url: &Url, form: Option<&[(&str, &str)]>) -> Result<Response> {
        let mut attempt: u32 = 0;
        loop {
            match self.send_once(url, form).await {
                Ok(mut response) => {
                    response.attempts = attempt.saturating_add(1);
                    return Ok(response);
                }
                Err(failure) => {
                    if !failure.error.is_retryable() || attempt >= self.config.max_retries {
                        return Err(failure.error);
                    }
                    let delay = failure
                        .retry_after
                        .unwrap_or_else(|| backoff(self.config.base_delay, attempt));
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn send_once(
        &self,
        url: &Url,
        form: Option<&[(&str, &str)]>,
    ) -> std::result::Result<Response, Failure> {
        let request = match form {
            Some(fields) => self.inner.post(url.as_str()).form(fields),
            None => self.inner.get(url.as_str()),
        };
        let response = request.send().await.map_err(map_transport_error)?;

        let status = response.status().as_u16();
        let final_url = Url::parse(&response.uri().to_string()).unwrap_or_else(|_| url.clone());
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(map_transport_error)?
            .to_vec();

        let result = Response {
            status,
            headers,
            body,
            attempts: 0,
            final_url,
        };

        if looks_like_challenge(&result) {
            return Err(KinoError::Challenge.into());
        }
        if status < 400 {
            return Ok(result);
        }
        let retry_after = (status == 429)
            .then(|| parse_retry_after(&result))
            .flatten();
        let error = match status {
            404 => KinoError::NotFound,
            _ => KinoError::Http { status },
        };
        Err(Failure { error, retry_after })
    }
}

fn map_transport_error(err: wreq::Error) -> Failure {
    let message = if err.is_timeout() {
        format!("таймаут запроса: {err}")
    } else {
        err.to_string()
    };
    KinoError::Network(message).into()
}

/// Anubis отвечает и 200, и 403/503, поэтому куки и `<title>` проверяются при любом статусе,
/// а широкий поиск маркеров — только при 403 и 503.
fn looks_like_challenge(response: &Response) -> bool {
    if has_anubis_cookie(&response.headers) || title_is_challenge(&response.body) {
        return true;
    }
    if !matches!(response.status, 403 | 503) {
        return false;
    }
    let contains_marker = |text: &str| {
        let lower = text.to_lowercase();
        CHALLENGE_MARKERS
            .iter()
            .any(|marker| lower.contains(marker))
    };
    response
        .headers
        .iter()
        .any(|(name, value)| contains_marker(name) || contains_marker(value))
        || contains_marker(&response.text())
}

/// Есть ли среди заголовков `Set-Cookie` кука, имя которой начинается с `techaro.lol-anubis`.
fn has_anubis_cookie(headers: &[(String, String)]) -> bool {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, value)| cookie_name(value))
        .any(|name| starts_with_ignore_ascii_case(name, ANUBIS_COOKIE_PREFIX))
}

/// Имя куки из значения `Set-Cookie`: всё до первого `=`.
fn cookie_name(set_cookie: &str) -> Option<&str> {
    let (name, _) = set_cookie.split_once('=')?;
    Some(name.trim())
}

fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Содержит ли какой-либо элемент `<title>` в начале тела фразу страницы-челленджа.
fn title_is_challenge(body: &[u8]) -> bool {
    let head = body.get(..TITLE_SCAN_LIMIT).unwrap_or(body);
    let lower = String::from_utf8_lossy(head).to_lowercase();
    let mut rest = lower.as_str();
    while let Some(open) = rest.find("<title") {
        let Some(after_open) = rest.get(open..) else {
            return false;
        };
        let Some(tag_end) = after_open.find('>') else {
            return false;
        };
        let Some(content) = after_open.get(tag_end + 1..) else {
            return false;
        };
        let (title, next) = match content.find("</title") {
            Some(close) => (
                content.get(..close).unwrap_or_default(),
                content.get(close..).unwrap_or_default(),
            ),
            None => (content, ""),
        };
        if CHALLENGE_TITLES.iter().any(|phrase| title.contains(phrase)) {
            return true;
        }
        rest = next;
    }
    false
}

fn parse_retry_after(response: &Response) -> Option<Duration> {
    let seconds: u64 = response.header("retry-after")?.trim().parse().ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

/// `base * 2^attempt`, затем половина задержки фиксирована, вторая половина случайная.
fn backoff(base: Duration, attempt: u32) -> Duration {
    let factor = 1u32 << attempt.min(MAX_BACKOFF_SHIFT);
    let delay = base.saturating_mul(factor).min(MAX_BACKOFF);
    let half = delay / 2;
    let random = RandomState::new().hash_one(attempt);
    let fraction = random as f64 / u64::MAX as f64;
    half + half.mul_f64(fraction)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn test_config() -> ClientConfig {
        ClientConfig {
            timeout: Duration::from_secs(5),
            max_retries: 3,
            base_delay: Duration::from_millis(2),
            user_agent: "kinobox-test".to_string(),
        }
    }

    fn client(config: ClientConfig) -> HttpClient {
        HttpClient::new(config).expect("клиент должен собираться")
    }

    fn url(server: &MockServer, route: &str) -> Url {
        Url::parse(&format!("{}{route}", server.uri())).expect("корректный url")
    }

    #[test]
    fn default_config_matches_contract() {
        let config = ClientConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(15));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay, Duration::from_millis(300));
        assert!(!config.user_agent.is_empty());
    }

    #[test]
    fn backoff_stays_within_jitter_bounds() {
        let base = Duration::from_millis(100);
        for attempt in 0..5 {
            let full = base * (1 << attempt);
            for _ in 0..50 {
                let delay = backoff(base, attempt);
                assert!(delay >= full / 2 && delay <= full, "{attempt}: {delay:?}");
            }
        }
    }

    #[test]
    fn backoff_is_capped() {
        let delay = backoff(Duration::from_secs(10), 40);
        assert!(delay <= MAX_BACKOFF);
    }

    #[tokio::test]
    async fn success_on_first_attempt() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ok"))
            .and(header("user-agent", "kinobox-test"))
            .respond_with(ResponseTemplate::new(200).set_body_string("привет"))
            .expect(1)
            .mount(&server)
            .await;

        let response = client(test_config()).get(&url(&server, "/ok")).await;
        let response = response.expect("ожидали успех");
        assert_eq!(response.status, 200);
        assert_eq!(response.attempts, 1);
        assert_eq!(response.text(), "привет");
    }

    #[tokio::test]
    async fn retries_503_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;

        let response = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect("ожидали успех после ретрая");
        assert_eq!(response.status, 200);
        assert_eq!(response.attempts, 2);
    }

    #[tokio::test]
    async fn persistent_503_exhausts_retries() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .expect(4)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::Http { status: 503 });
    }

    #[tokio::test]
    async fn not_found_is_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/missing"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::NotFound);
    }

    #[tokio::test]
    async fn plain_403_is_http_error_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .expect(1)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::Http { status: 403 });
    }

    #[tokio::test]
    async fn anubis_body_is_challenge_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_string("<html><title>Making sure you're not a bot!</title></html>"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::Challenge);
    }

    #[tokio::test]
    async fn anubis_marker_in_503_is_challenge_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503).set_body_string("protected by ANUBIS"))
            .expect(1)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::Challenge);
    }

    /// gzip без сжатия (один stored-блок deflate), чтобы не тянуть зависимость ради теста.
    fn gzip_stored(data: &[u8]) -> Vec<u8> {
        let mut crc = u32::MAX;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        let len = u16::try_from(data.len()).expect("тестовые данные короче 64 КиБ");
        let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff, 1];
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(data);
        out.extend_from_slice(&(!crc).to_le_bytes());
        out.extend_from_slice(&u32::from(len).to_le_bytes());
        out
    }

    fn header_pair(name: &str, value: &str) -> (String, String) {
        (name.to_string(), value.to_string())
    }

    #[test]
    fn cookie_name_is_taken_before_first_equals() {
        assert_eq!(
            cookie_name("techaro.lol-anubis-auth-d82da6a5=; Path=/"),
            Some("techaro.lol-anubis-auth-d82da6a5")
        );
        assert_eq!(cookie_name(" sid =abc"), Some("sid"));
        assert_eq!(cookie_name("no-equals"), None);
    }

    #[test]
    fn anubis_cookie_is_detected_among_several_set_cookie_headers() {
        let headers = vec![
            header_pair("set-cookie", "sid=abc; Path=/"),
            header_pair(
                "set-cookie",
                "techaro.lol-anubis-cookie-verification-d82da6a5=1b2c; Path=/",
            ),
        ];
        assert!(has_anubis_cookie(&headers));
    }

    #[test]
    fn anubis_cookie_requires_name_prefix_in_set_cookie() {
        let by_value = vec![header_pair("set-cookie", "sid=techaro.lol-anubis-auth")];
        assert!(!has_anubis_cookie(&by_value));
        let other_header = vec![header_pair("x-note", "techaro.lol-anubis-auth=1")];
        assert!(!has_anubis_cookie(&other_header));
        let suffix = vec![header_pair("set-cookie", "x-techaro.lol-anubis=1")];
        assert!(!has_anubis_cookie(&suffix));
    }

    #[test]
    fn anubis_cookie_comparison_ignores_case() {
        let headers = vec![header_pair(
            "Set-Cookie",
            "TECHARO.lol-Anubis-Auth-AB12=; Path=/",
        )];
        assert!(has_anubis_cookie(&headers));
    }

    #[test]
    fn starts_with_ignore_ascii_case_handles_short_and_multibyte_text() {
        assert!(starts_with_ignore_ascii_case("Techaro.lol", "techaro"));
        assert!(!starts_with_ignore_ascii_case("tech", "techaro"));
        assert!(!starts_with_ignore_ascii_case("пример", "techaro"));
    }

    #[test]
    fn title_detects_russian_and_english_challenge() {
        assert!(title_is_challenge(
            "<html><head><title>Проверяем, что вы не бот!</title></head></html>".as_bytes()
        ));
        assert!(title_is_challenge(
            b"<TITLE>Making sure you're not a bot!</TITLE>"
        ));
        assert!(title_is_challenge(
            "<title>Making sure you\u{2019}re not a bot!</title>".as_bytes()
        ));
        assert!(title_is_challenge(
            b"<title lang=\"en\">MAKING SURE YOU'RE NOT A BOT</title>"
        ));
    }

    #[test]
    fn title_ignores_phrase_outside_title_and_ordinary_titles() {
        assert!(!title_is_challenge(
            b"<title>Home</title><p>Making sure you're not a bot</p>"
        ));
        assert!(!title_is_challenge(
            "<title>Вход на HDrezka</title>".as_bytes()
        ));
        assert!(!title_is_challenge(b"<p>Anubis</p>"));
        assert!(!title_is_challenge(b""));
        assert!(!title_is_challenge(b"<title"));
    }

    #[test]
    fn title_finds_challenge_in_second_title_element() {
        assert!(title_is_challenge(
            b"<svg><title>logo</title></svg><title>Making sure you're not a bot!</title>"
        ));
    }

    #[test]
    fn title_survives_invalid_utf8_and_unclosed_tag() {
        let mut body = b"<title>Making sure you're not a bot".to_vec();
        body.extend_from_slice(&[0xff, 0xfe]);
        assert!(title_is_challenge(&body));
        assert!(!title_is_challenge(&[0xff, 0xfe, b'<', b't']));
    }

    #[tokio::test]
    async fn anubis_cookie_on_200_is_challenge_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Set-Cookie", "techaro.lol-anubis-auth-d82da6a5=; Path=/")
                    .set_body_string("<html><body>что угодно</body></html>"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::Challenge);
    }

    #[tokio::test]
    async fn russian_title_on_200_is_challenge() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<html><title>Проверяем, что вы не бот!</title></html>"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::Challenge);
    }

    #[tokio::test]
    async fn english_title_on_200_is_challenge() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<html><title>Making sure you're not a bot!</title></html>"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::Challenge);
    }

    #[tokio::test]
    async fn anubis_word_in_ordinary_200_body_is_not_challenge() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "<html><title>Фильм</title><body><h1>Anubis: Страж</h1></body></html>",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let response = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect("обычная страница не челлендж");
        assert_eq!(response.status, 200);
    }

    #[tokio::test]
    async fn login_page_title_on_200_is_not_challenge() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<html><title>Вход на HDrezka</title></html>"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let response = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect("страница входа не челлендж");
        assert_eq!(response.status, 200);
    }

    #[tokio::test]
    async fn gzip_body_is_decoded_before_detection() {
        let compressed = gzip_stored("<title>Проверяем, что вы не бот!</title>".as_bytes());

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Encoding", "gzip")
                    .set_body_bytes(compressed),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect_err("ожидали ошибку");
        assert_eq!(err, KinoError::Challenge);
    }

    #[tokio::test]
    async fn timeout_is_network_error_with_retries() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(600)))
            .expect(3)
            .mount(&server)
            .await;

        let config = ClientConfig {
            timeout: Duration::from_millis(100),
            max_retries: 2,
            ..test_config()
        };
        let err = client(config)
            .get(&url(&server, "/slow"))
            .await
            .expect_err("ожидали таймаут");
        assert!(matches!(err, KinoError::Network(_)), "{err:?}");
    }

    #[tokio::test]
    async fn honors_retry_after_on_429() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let started = Instant::now();
        let response = client(test_config())
            .get(&url(&server, "/"))
            .await
            .expect("ожидали успех после ожидания");
        assert!(started.elapsed() >= Duration::from_millis(950));
        assert_eq!(response.attempts, 2);
    }

    #[tokio::test]
    async fn post_form_sends_urlencoded_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ajax/search/"))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string("q=%D0%BA%D0%B8%D0%BD%D0%BE&page=1"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&server)
            .await;

        let response = client(test_config())
            .post_form(
                &url(&server, "/ajax/search/"),
                &[("q", "кино"), ("page", "1")],
            )
            .await
            .expect("ожидали успех");
        assert_eq!(response.status, 200);
        assert_eq!(response.text(), "{}");
    }

    #[tokio::test]
    async fn cookies_are_stored_between_requests() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("cookie", "sid=abc"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let http = client(test_config());
        http.get(&url(&server, "/login")).await.expect("login");
        http.get(&url(&server, "/me"))
            .await
            .expect("cookie должен уйти");
    }

    #[tokio::test]
    async fn follows_redirects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/old"))
            .respond_with(ResponseTemplate::new(301).insert_header("Location", "/new"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/new"))
            .respond_with(ResponseTemplate::new(200).set_body_string("moved"))
            .expect(1)
            .mount(&server)
            .await;

        let response = client(test_config())
            .get(&url(&server, "/old"))
            .await
            .expect("ожидали успех после редиректа");
        assert_eq!(response.status, 200);
        assert_eq!(response.text(), "moved");
        assert_eq!(response.attempts, 1);
        assert_eq!(response.final_url, url(&server, "/new"));
    }

    #[tokio::test]
    async fn connection_refused_is_network_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("свободный порт");
        let port = listener.local_addr().expect("адрес сокета").port();
        drop(listener);
        let dead = Url::parse(&format!("http://127.0.0.1:{port}/")).expect("корректный url");

        let config = ClientConfig {
            max_retries: 1,
            ..test_config()
        };
        let err = client(config).get(&dead).await.expect_err("ожидали ошибку");
        assert!(matches!(err, KinoError::Network(_)), "{err:?}");
    }
}
