mod features;
mod security;

use std::time::Duration;

use axum::{Router, http::StatusCode, middleware};
use axum_login::AuthManagerLayerBuilder;
use features::auth::{AuthBackend, access_guard};
use features::books::{BooksState, router as books_router};
use features::clippings::{ClippingsState, router as clippings_router};
use features::reviews::{ReviewsState, router as reviews_router};
use sqlx::postgres::PgPoolOptions;
use time::Duration as TimeDuration;
use tower_http::services::ServeDir;
use tower_sessions::{
    Expiry, SessionManagerLayer, cookie::SameSite, session_store::ExpiredDeletion,
};
use tower_sessions_sqlx_store::PostgresStore;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cookie_secure =
        std::env::var("SESSION_COOKIE_SECURE").map_or(Ok(false), |value| value.parse::<bool>())?;
    let origin = std::env::var("APP_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_owned());
    let security = security::Security::new(&origin, cookie_secure)?;
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
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    info!("running database migrations");
    sqlx::migrate!("../../migrations").run(&pool).await?;

    let session_store = PostgresStore::new(pool.clone());
    session_store.migrate().await?;
    let cleanup_store = session_store.clone();
    tokio::spawn(async move {
        if let Err(_error) = cleanup_store
            .continuously_delete_expired(Duration::from_secs(60))
            .await
        {
            error!("expired session cleanup stopped");
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
            "OPEN_LIBRARY_CONTACT_EMAIL is not configured; using conservative Open Library request pacing"
        );
    }

    let open_library_client = reqwest::Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(10))
        .build()?;
    let open_library = bookreplay_openlibrary::OpenLibrary::new(open_library_client);
    bookreplay_openlibrary::spawn_enrichment_worker(database_url, open_library.clone());

    let api = Router::new()
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
        .fallback(not_found);
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new("app/web/build"))
        .layer(middleware::from_fn_with_state(pool, access_guard))
        .layer(auth_layer)
        .layer(middleware::from_fn_with_state(security, security::guard));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;

    info!(address = %listener.local_addr()?, "API is listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Operator-only recovery: reads a new password from stdin, never arguments or logs.
pub async fn reset_owner_password() -> Result<(), Box<dyn std::error::Error>> {
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
    features::auth::reset_password(&pool, password).await
}

async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "route not found")
}
