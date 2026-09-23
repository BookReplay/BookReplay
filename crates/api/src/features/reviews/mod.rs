mod controller;
mod model;
mod scheduler;

use axum::{
    Router,
    routing::{get, post},
};
use sqlx::PgPool;

#[derive(Clone)]
pub struct ReviewsState {
    pub pool: PgPool,
}

pub fn router(state: ReviewsState) -> Router {
    Router::new()
        .route("/highlights/session", get(controller::session))
        .route("/highlights/{highlight_id}", post(controller::review))
        .route("/streak", get(controller::streak))
        .with_state(state)
}
