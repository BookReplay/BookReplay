mod controller;
mod model;

use axum::{
    Router,
    routing::{get, put},
};
use rekindle_openlibrary::OpenLibrary;
use sqlx::PgPool;

#[derive(Clone)]
pub struct BooksState {
    pub pool: PgPool,
    pub open_library: OpenLibrary,
}

pub fn router(state: BooksState) -> Router {
    Router::new()
        .route("/", get(controller::get_all))
        .route("/search", get(controller::search))
        .route("/{book_id}/identification", put(controller::identify))
        .with_state(state)
}
