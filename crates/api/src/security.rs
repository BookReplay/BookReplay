use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

#[derive(Clone)]
pub(crate) struct Security {
    origin: HeaderValue,
    attempts: Arc<Mutex<VecDeque<Instant>>>,
}

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
            attempts: Arc::default(),
        })
    }

    fn allow_attempt(&self, now: Instant) -> bool {
        let Ok(mut attempts) = self.attempts.lock() else {
            return false;
        };
        while attempts
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
        {
            attempts.pop_front();
        }
        if attempts.len() >= 10 {
            return false;
        }
        attempts.push_back(now);
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
        if matches!(
            request.uri().path(),
            "/api/auth/login" | "/api/auth/register" | "/api/auth/password"
        ) && !security.allow_attempt(Instant::now())
        {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, "60")],
                "too many authentication attempts; try again in one minute",
            )
                .into_response();
        }
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
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
        assert_eq!(
            client
                .post(format!("http://{address}/api/auth/login"))
                .header("Origin", "https://books.example")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
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
    fn origin_configuration_rejects_unsafe_or_ambiguous_deployments() {
        for (origin, secure) in [
            ("https://books.example/", true),
            ("https://u:p@books.example", true),
            ("https://books.example?q=x", true),
            ("http://books.example", false),
            ("http://localhost:3000", true),
            ("null", false),
        ] {
            assert!(Security::new(origin, secure).is_err(), "{origin}");
        }
        assert!(Security::new("http://localhost:3000", false).is_ok());
        assert!(Security::new("https://books.example", true).is_ok());
    }

    #[test]
    fn rate_limit_has_a_bounded_sliding_window() {
        let security = Security::new("http://localhost:3000", false).unwrap();
        let now = Instant::now();
        for _ in 0..10 {
            assert!(security.allow_attempt(now));
        }
        assert!(!security.allow_attempt(now + Duration::from_secs(59)));
        assert!(security.allow_attempt(now + Duration::from_secs(60)));
    }
}
