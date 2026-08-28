mod controller;
mod model;

use axum::{Router, routing::get};
use sqlx::PgPool;

#[derive(Clone)]
pub struct BooksState {
    pub pool: PgPool,
}

pub fn router(state: BooksState) -> Router {
    Router::new()
        .route("/", get(controller::get_all))
        .with_state(state)
}
