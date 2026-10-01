mod controller;
pub(super) mod model;
mod view;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{patch, post},
};
use sqlx::PgPool;

/// Largest accepted `My Clippings.txt`; keep deploy/Caddyfile in step.
pub const MAX_IMPORT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct ClippingsState {
    pub pool: PgPool,
}

pub fn router(state: ClippingsState) -> Router {
    Router::new()
        .route(
            "/import",
            post(controller::import).layer(DefaultBodyLimit::max(MAX_IMPORT_BYTES)),
        )
        .route("/{clipping_id}", patch(controller::update))
        .with_state(state)
}
