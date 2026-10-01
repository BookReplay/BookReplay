mod controller;
mod model;

use axum::{
    Router,
    routing::{get, put},
};
use bookreplay_openlibrary::OpenLibrary;
use sqlx::PgPool;

#[derive(Clone)]
pub struct BooksState {
    pub pool: PgPool,
    /// `None` when the operator turned metadata enrichment off.
    pub open_library: Option<OpenLibrary>,
}

pub fn router(state: BooksState) -> Router {
    Router::new()
        .route("/", get(controller::get_all))
        .route("/search", get(controller::search))
        .route("/{book_id}/clippings", get(controller::clippings))
        .route("/{book_id}/identification", put(controller::identify))
        .with_state(state)
}
