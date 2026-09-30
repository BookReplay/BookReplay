mod controller;
pub(super) mod model;
mod view;

use axum::{
    Router,
    routing::{get, patch, post},
};
use sqlx::PgPool;

#[derive(Clone)]
pub struct ClippingsState {
    pub pool: PgPool,
}

pub fn router(state: ClippingsState) -> Router {
    Router::new()
        .route("/", get(controller::get_all))
        .route("/import", post(controller::import))
        .route("/{clipping_id}", patch(controller::update))
        .with_state(state)
}
