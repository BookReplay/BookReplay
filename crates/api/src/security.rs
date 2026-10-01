use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

#[derive(Clone)]
pub(crate) struct Security {
    origin: HeaderValue,
    secure: bool,
    client_ip_header: Option<HeaderName>,
    client_attempts: usize,
    total_attempts: usize,
    /// Limited in addition to the built-in authentication routes.
    limited_paths: &'static [&'static str],
    attempts: Arc<Mutex<Attempts>>,
}

const ATTEMPT_WINDOW: Duration = Duration::from_secs(60);
/// Authentication attempts allowed per client address in one window.
pub(crate) const CLIENT_ATTEMPTS: usize = 10;
/// Ceiling across all clients, so many addresses cannot multiply password guesses.
pub(crate) const TOTAL_ATTEMPTS: usize = 60;

#[derive(Default)]
struct Attempts {
    total: VecDeque<Instant>,
    clients: HashMap<Option<IpAddr>, VecDeque<Instant>>,
}

// Scripts are restricted by the hash policy SvelteKit writes into each prerendered page.
// Open Library cover URLs redirect to archive.org, and redirects must be allowed too.
const CONTENT_SECURITY_POLICY: &str = "frame-ancestors 'none'; base-uri 'none'; form-action 'self'; object-src 'none'; \
     connect-src 'self'; \
     img-src 'self' data: https://covers.openlibrary.org https://archive.org https://*.archive.org https://books.google.com https://books.googleusercontent.com";

impl Security {
    pub(crate) fn new(origin: &str, secure: bool) -> Result<Self, &'static str> {
        let url = reqwest::Url::parse(origin).map_err(|_| "invalid APP_ORIGIN")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.origin().ascii_serialization() != origin
            || (secure && url.scheme() != "https")
            || (!secure && !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
        {
            return Err(
                "APP_ORIGIN must be an exact origin; remote access requires HTTPS and SESSION_COOKIE_SECURE=true",
            );
        }
        Ok(Self {
            origin: HeaderValue::from_str(origin).map_err(|_| "invalid APP_ORIGIN")?,
            secure,
            client_ip_header: None,
            client_attempts: CLIENT_ATTEMPTS,
            total_attempts: TOTAL_ATTEMPTS,
            limited_paths: &[],
            attempts: Arc::default(),
        })
    }

    pub(crate) fn with_attempt_limits(
        mut self,
        client_attempts: usize,
        total_attempts: usize,
        limited_paths: &'static [&'static str],
    ) -> Self {
        self.client_attempts = client_attempts;
        self.total_attempts = total_attempts;
        self.limited_paths = limited_paths;
        self
    }

    /// Names the header in which a trusted reverse proxy reports the client address.
    pub(crate) fn with_client_ip_header(
        mut self,
        name: Option<&str>,
    ) -> Result<Self, &'static str> {
        self.client_ip_header = name
            .map(|name| {
                name.parse()
                    .map_err(|_| "CLIENT_IP_HEADER must be an HTTP header name")
            })
            .transpose()?;
        Ok(self)
    }

    /// The address attempts are counted against: the proxy-reported client when
    /// configured, otherwise the connecting peer.
    fn client(&self, headers: &HeaderMap, peer: Option<IpAddr>) -> Option<IpAddr> {
        let Some(name) = &self.client_ip_header else {
            return peer;
        };
        // The last entry is the one the nearest proxy appended; earlier ones are client-supplied.
        headers
            .get_all(name)
            .iter()
            .next_back()
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit(',').next())
            .and_then(|value| value.trim().parse().ok())
            .or(peer)
    }

    fn allow_attempt(&self, client: Option<IpAddr>, now: Instant) -> bool {
        let Ok(mut attempts) = self.attempts.lock() else {
            return false;
        };
        let expired = |at: &Instant| now.duration_since(*at) >= ATTEMPT_WINDOW;
        while attempts.total.front().is_some_and(expired) {
            attempts.total.pop_front();
        }
        attempts.clients.retain(|_, recent| {
            while recent.front().is_some_and(expired) {
                recent.pop_front();
            }
            !recent.is_empty()
        });
        if attempts.total.len() >= self.total_attempts
            || attempts
                .clients
                .get(&client)
                .is_some_and(|recent| recent.len() >= self.client_attempts)
        {
            return false;
        }
        attempts.total.push_back(now);
        attempts.clients.entry(client).or_default().push_back(now);
        true
    }
}

pub(crate) async fn guard(
    State(security): State<Security>,
    request: Request,
    next: Next,
) -> Response {
    if !request.method().is_safe() {
        let mut origins = request.headers().get_all(header::ORIGIN).iter();
        if origins.next() != Some(&security.origin) || origins.next().is_some() {
            return (StatusCode::FORBIDDEN, "request origin is not allowed").into_response();
        }
        let path = request.uri().path();
        if (matches!(
            path,
            "/api/auth/login" | "/api/auth/register" | "/api/auth/password"
        ) || security.limited_paths.contains(&path))
            && !security.allow_attempt(
                security.client(
                    request.headers(),
                    request
                        .extensions()
                        .get::<ConnectInfo<SocketAddr>>()
                        .map(|peer| peer.ip()),
                ),
                Instant::now(),
            )
        {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, "60")],
                "too many authentication attempts; try again in one minute",
            )
                .into_response();
        }
    }
    // Hashed build output never changes; everything else may hold private library data.
    let immutable = request.uri().path().starts_with("/_app/immutable/");
    let mut response = next.run(request).await;
    let cache_control = if immutable && response.status().is_success() {
        "public, max-age=31536000, immutable"
    } else {
        "no-store"
    };
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    if security.secure {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn all_unsafe_methods_require_one_exact_origin_even_with_spoofed_proxy_headers() {
        let app = axum::Router::new()
            .fallback(|| async { StatusCode::NO_CONTENT })
            .layer(axum::middleware::from_fn_with_state(
                Security::new("https://books.example", true).unwrap(),
                guard,
            ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::new();
        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            for origin in [
                None,
                Some("null"),
                Some("https://evil.example"),
                Some("https://books.example.evil"),
                Some("http://books.example"),
            ] {
                let mut request = client
                    .request(
                        method.parse().unwrap(),
                        format!("http://{address}/api/clippings/import"),
                    )
                    .header("X-Forwarded-Host", "books.example")
                    .header("X-Forwarded-Proto", "https");
                if let Some(origin) = origin {
                    request = request.header("Origin", origin);
                }
                assert_eq!(
                    request.send().await.unwrap().status(),
                    StatusCode::FORBIDDEN
                );
            }
        }
        let allowed = client
            .post(format!("http://{address}/api/auth/login"))
            .header("Origin", "https://books.example")
            .send()
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::NO_CONTENT);
        for (name, value) in [
            ("cache-control", "no-store"),
            ("x-content-type-options", "nosniff"),
            ("x-frame-options", "DENY"),
            ("referrer-policy", "no-referrer"),
            ("strict-transport-security", "max-age=31536000"),
        ] {
            assert_eq!(allowed.headers()[name], value, "{name}");
        }
        assert!(
            allowed.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .starts_with("frame-ancestors 'none';")
        );
        let asset = client
            .get(format!("http://{address}/_app/immutable/chunk.js"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            asset.headers()["cache-control"],
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            client
                .post(format!("http://{address}/api/auth/login"))
                .header("Origin", "https://books.example")
                .header("Origin", "https://evil.example")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        task.abort();
    }

    #[test]
    fn attempts_are_limited_per_client_with_a_shared_ceiling() {
        let security = Security::new("https://books.example", true).unwrap();
        let now = Instant::now();
        let client = |last: u8| Some(IpAddr::from([192, 0, 2, last]));

        for _ in 0..CLIENT_ATTEMPTS {
            assert!(security.allow_attempt(client(1), now));
        }
        assert!(!security.allow_attempt(client(1), now));
        // One client being locked out leaves the owner's own address usable.
        assert!(security.allow_attempt(client(2), now));
        for last in 3..=u8::try_from(TOTAL_ATTEMPTS - CLIENT_ATTEMPTS + 1).unwrap() {
            assert!(security.allow_attempt(client(last), now));
        }
        assert!(!security.allow_attempt(client(200), now));
        assert!(security.allow_attempt(client(1), now + ATTEMPT_WINDOW));
    }

    #[test]
    fn attempt_limits_can_be_raised() {
        let security = Security::new("https://books.example", true)
            .unwrap()
            .with_attempt_limits(CLIENT_ATTEMPTS + 1, TOTAL_ATTEMPTS, &[]);
        let now = Instant::now();
        let client = Some(IpAddr::from([192, 0, 2, 1]));

        for _ in 0..=CLIENT_ATTEMPTS {
            assert!(security.allow_attempt(client, now));
        }
        assert!(!security.allow_attempt(client, now));
    }

    #[test]
    fn client_address_comes_from_the_proxy_only_when_configured() {
        let peer = Some(IpAddr::from([127, 0, 0, 1]));
        let mut headers = HeaderMap::new();
        headers.append(
            "x-forwarded-for",
            "203.0.113.9, 198.51.100.7".parse().unwrap(),
        );
        let direct = Security::new("https://books.example", true).unwrap();
        let proxied = Security::new("https://books.example", true)
            .unwrap()
            .with_client_ip_header(Some("X-Forwarded-For"))
            .unwrap();

        assert_eq!(direct.client(&headers, peer), peer);
        // The spoofable first entry is ignored in favour of the proxy's own.
        assert_eq!(
            proxied.client(&headers, peer),
            Some(IpAddr::from([198, 51, 100, 7]))
        );
        assert_eq!(proxied.client(&HeaderMap::new(), peer), peer);
        assert!(direct.with_client_ip_header(Some("not a header")).is_err());
    }

    #[test]
    fn origin_configuration_rejects_unsafe_or_ambiguous_deployments() {
        for (origin, secure) in [
            ("https://books.example/", true),
            ("https://u:p@books.example", true),
            ("https://books.example?q=x", true),
            ("http://books.example", false),
            ("http://localhost:2665", true),
            ("null", false),
        ] {
            assert!(Security::new(origin, secure).is_err(), "{origin}");
        }
        assert!(Security::new("http://localhost:2665", false).is_ok());
        assert!(Security::new("https://books.example", true).is_ok());
    }

    #[test]
    fn rate_limit_has_a_bounded_sliding_window() {
        let security = Security::new("http://localhost:2665", false).unwrap();
        let now = Instant::now();
        for _ in 0..10 {
            assert!(security.allow_attempt(None, now));
        }
        assert!(!security.allow_attempt(None, now + Duration::from_secs(59)));
        assert!(security.allow_attempt(None, now + Duration::from_secs(60)));
    }
}
