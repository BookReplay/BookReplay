mod controller;
mod model;
mod view;

use axum::{
    Router,
    routing::{get, post},
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
        .with_state(state)
}
