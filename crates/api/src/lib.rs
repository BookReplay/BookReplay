mod error;
mod features;
mod security;

use std::{net::SocketAddr, time::Duration};

use axum::{Router, extract::State, http::StatusCode, middleware, routing::get};
use axum_login::AuthManagerLayerBuilder;
use features::auth::{AccessGuard, AuthBackend, access_guard};
use features::books::{BooksState, router as books_router};
use features::clippings::{ClippingsState, router as clippings_router};
use features::reviews::{ReviewsState, router as reviews_router};
use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};
use time::Duration as TimeDuration;
use tower_http::services::ServeDir;
use tower_sessions::{
    Expiry, SessionManagerLayer, cookie::SameSite, session_store::ExpiredDeletion,
};
use tower_sessions_sqlx_store::PostgresStore;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

pub use error::{ApiError, user_id};
pub use features::auth::{
    AuthError, AuthSession, Registration, create_user, hash_password, verify_password,
};

/// What extension routes are built from.
#[derive(Clone)]
pub struct Core {
    pub pool: PgPool,
}

/// Additions a build embedding this crate makes to the single-owner application.
/// The default adds nothing.
#[derive(Default)]
pub struct Extension {
    /// Nested under `/api`, behind the origin check, the session and the access guard.
    pub routes: Option<fn(Core) -> Router>,
    /// Paths served without a session; each also covers what lies beneath it.
    pub public_paths: &'static [&'static str],
    /// Unsafe-method paths counted against the authentication attempt limits.
    pub rate_limited_paths: &'static [&'static str],
    pub limits: Limits,
    /// Run after the built-in migrations, against the same database.
    pub migrator: Option<Migrator>,
}

pub struct Limits {
    /// Authentication attempts per client address per minute.
    pub client_attempts: usize,
    /// Authentication attempts across all clients per minute.
    pub total_attempts: usize,
    /// Password hashes computed at once; further requests are told to retry.
    pub password_jobs: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            client_attempts: security::CLIENT_ATTEMPTS,
            total_attempts: security::TOTAL_ATTEMPTS,
            password_jobs: features::auth::DEFAULT_PASSWORD_JOBS,
        }
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_with(Extension::default()).await
}

pub async fn run_with(extension: Extension) -> Result<(), Box<dyn std::error::Error>> {
    let cookie_secure = std::env::var("SESSION_COOKIE_SECURE")
        .map_or(Ok(false), |value| value.parse::<bool>())
        .map_err(|_| "SESSION_COOKIE_SECURE must be exactly true or false")?;
    let bind_address = bind_address()?;
    let origin = std::env::var("APP_ORIGIN").unwrap_or_else(|_| "http://localhost:2665".to_owned());
    let client_ip_header = std::env::var("CLIENT_IP_HEADER")
        .ok()
        .filter(|name| !name.trim().is_empty());
    let security = security::Security::new(&origin, cookie_secure)?
        .with_client_ip_header(client_ip_header.as_deref().map(str::trim))?
        .with_attempt_limits(
            extension.limits.client_attempts,
            extension.limits.total_attempts,
            extension.rate_limited_paths,
        );
    features::auth::allow_password_jobs(extension.limits.password_jobs);
    let enrichment = std::env::var("METADATA_ENRICHMENT")
        .map_or(Ok(true), |value| value.parse::<bool>())
        .map_err(|_| "METADATA_ENRICHMENT must be exactly true or false")?;
    let setup_secret = std::env::var("SETUP_SECRET")
        .ok()
        .filter(|secret| !secret.is_empty());
    if setup_secret
        .as_ref()
        .is_some_and(|secret| !(32..=128).contains(&secret.len()))
    {
        return Err(
            "SETUP_SECRET must be 32–128 bytes; generate it with openssl rand -hex 32".into(),
        );
    }
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("bookreplay_api=info")),
        )
        .with(
            tracing_subscriber::fmt::layer().with_filter(tracing_subscriber::filter::filter_fn(
                |metadata| metadata.target().starts_with("bookreplay_"),
            )),
        )
        .init();

    info!("connecting to database");
    let database_url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required")?;
    let pool = connect_when_ready(&database_url).await?;

    info!("running database migrations");
    let mut migrator = sqlx::migrate!("../../migrations");
    // Each set of migrations finds the other's versions already applied.
    migrator.set_ignore_missing(extension.migrator.is_some());
    migrator
        .run(&pool)
        .await
        .map_err(|error| format!("database migration failed: {error}"))?;
    if let Some(mut migrator) = extension.migrator {
        migrator.set_ignore_missing(true);
        migrator
            .run(&pool)
            .await
            .map_err(|error| format!("extension database migration failed: {error}"))?;
    }

    let session_store = PostgresStore::new(pool.clone());
    session_store.migrate().await.map_err(|error| {
        format!(
            "session table setup failed ({})",
            error::database_cause(&error)
        )
    })?;
    let cleanup_store = session_store.clone();
    tokio::spawn(async move {
        // Keep cleaning after a failed pass, such as a database restart.
        loop {
            if let Err(_error) = cleanup_store
                .clone()
                .continuously_delete_expired(Duration::from_secs(60 * 60))
                .await
            {
                error!("expired session cleanup failed; retrying in one minute");
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("bookreplay.sid")
        .with_http_only(true)
        .with_same_site(SameSite::Strict)
        .with_path("/")
        .with_secure(cookie_secure)
        .with_expiry(Expiry::OnInactivity(TimeDuration::days(30)))
        .with_always_save(true);
    let auth_backend = AuthBackend::new(pool.clone(), setup_secret).await?;
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend.clone(), session_layer).build();
    info!("database is ready");

    let open_library = if enrichment {
        let contact_email = std::env::var("OPEN_LIBRARY_CONTACT_EMAIL")
            .ok()
            .filter(|email| !email.trim().is_empty());
        let user_agent = contact_email.as_ref().map_or_else(
            || concat!("bookreplay-api/", env!("CARGO_PKG_VERSION")).to_owned(),
            |email| {
                format!(
                    "bookreplay-api/{} ({})",
                    env!("CARGO_PKG_VERSION"),
                    email.trim()
                )
            },
        );
        if contact_email.is_none() {
            warn!(
                "OPEN_LIBRARY_CONTACT_EMAIL is not configured; metadata requests carry no contact address"
            );
        }
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(10))
            .build()?;
        let open_library = bookreplay_openlibrary::OpenLibrary::new(client)
            .with_google_key(std::env::var("GOOGLE_BOOKS_API_KEY").ok());
        bookreplay_openlibrary::spawn_enrichment_worker(database_url, open_library.clone());
        Some(open_library)
    } else {
        info!("metadata enrichment is disabled; no book titles leave this instance");
        None
    };

    let api = Router::new()
        .route("/version", get(|| async { env!("CARGO_PKG_VERSION") }))
        .nest("/auth", features::auth::router(auth_backend))
        .nest(
            "/books",
            books_router(BooksState {
                pool: pool.clone(),
                open_library,
            }),
        )
        .nest(
            "/clippings",
            clippings_router(ClippingsState { pool: pool.clone() }),
        )
        .nest(
            "/reviews",
            reviews_router(ReviewsState { pool: pool.clone() }),
        )
        .merge(
            extension
                .routes
                .map_or_else(Router::new, |routes| routes(Core { pool: pool.clone() })),
        )
        .fallback(not_found);
    let site = ServeDir::new("app/web/build")
        .precompressed_br()
        .precompressed_gzip();
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(site)
        .layer(middleware::from_fn_with_state(
            AccessGuard {
                pool: pool.clone(),
                public_paths: extension.public_paths,
            },
            access_guard,
        ))
        .layer(auth_layer)
        // Added after the session layer: build assets and health probes are public and need no database session.
        .nest_service(
            "/_app",
            ServeDir::new("app/web/build/_app")
                .precompressed_br()
                .precompressed_gzip(),
        )
        .route("/healthz", get(health).with_state(pool))
        .layer(middleware::from_fn_with_state(security, security::guard));

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .map_err(|error| format!("could not listen on {bind_address}: {error}"))?;

    info!(address = %listener.local_addr()?, "API is listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    info!("shutdown complete");
    Ok(())
}

/// Waits for PostgreSQL to accept connections, as when both containers restart together.
/// Rejected credentials and other non-network failures are reported at once.
async fn connect_when_ready(database_url: &str) -> Result<PgPool, String> {
    const ATTEMPTS: u32 = 30;
    let mut attempt = 1;
    loop {
        match PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(sqlx::Error::Io(_)) if attempt < ATTEMPTS => {
                warn!(
                    attempt,
                    "database is not accepting connections yet; retrying in one second"
                );
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(error) => {
                return Err(format!(
                    "could not connect to the database ({}); check DATABASE_URL and that PostgreSQL is running",
                    error::database_cause(&error)
                ));
            }
        }
    }
}

fn bind_address() -> Result<SocketAddr, &'static str> {
    std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:2665".to_owned())
        .parse()
        .map_err(|_| "BIND_ADDR must be an IP address and port, such as 127.0.0.1:2665")
}

async fn health(State(pool): State<PgPool>) -> StatusCode {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_error) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            info!("shutdown signal received; finishing open requests");
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received; finishing open requests");
}

/// Container health probe: succeeds when the running server answers `/healthz`.
pub async fn healthcheck() -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut address = bind_address()?;
    if address.ip().is_unspecified() {
        address.set_ip(std::net::Ipv4Addr::LOCALHOST.into());
    }
    let probe = async {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await?;
        let mut status = [0_u8; 12];
        stream.read_exact(&mut status).await?;
        Ok::<_, std::io::Error>(status)
    };
    match tokio::time::timeout(Duration::from_secs(3), probe).await {
        Ok(Ok(status)) if &status == b"HTTP/1.1 204" => Ok(()),
        _ => Err("server is not healthy".into()),
    }
}

/// Operator-only recovery: reads a new password from stdin, never arguments or logs.
/// `email` picks the account when the instance has more than one.
pub async fn reset_owner_password(email: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut password = String::new();
    std::io::stdin()
        .take(130)
        .read_to_string(&mut password)
        .map_err(|_| "could not read password from stdin")?;
    if password.ends_with('\n') {
        password.pop();
        if password.ends_with('\r') {
            password.pop();
        }
    }
    let database_url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required")?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .map_err(|_| "could not connect to database")?;
    features::auth::reset_password(&pool, email.as_deref(), password).await
}

async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "route not found")
}
