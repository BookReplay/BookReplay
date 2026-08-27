mod features;

use std::time::Duration;

use axum::{Router, http::StatusCode};
use features::clippings::{ClippingsState, router as clippings_router};
use sqlx::postgres::PgPoolOptions;
use tower_http::services::ServeDir;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("rekindle_api=info")),
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
    info!("database is ready");

    let contact_email = std::env::var("OPEN_LIBRARY_CONTACT_EMAIL")
        .ok()
        .filter(|email| !email.trim().is_empty());
    let user_agent = contact_email.as_ref().map_or_else(
        || concat!("rekindle-api/", env!("CARGO_PKG_VERSION")).to_owned(),
        |email| {
            format!(
                "rekindle-api/{} ({})",
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

    let open_library = reqwest::Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(10))
        .build()?;
    rekindle_openlibrary::spawn_enrichment_worker(database_url, open_library);

    let api = Router::new()
        .nest("/clippings", clippings_router(ClippingsState { pool }))
        .fallback(not_found);
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new("app/web/build"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;

    info!(address = %listener.local_addr()?, "API is listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "route not found")
}
