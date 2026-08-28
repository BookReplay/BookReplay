use axum::{Json, extract::State, http::StatusCode};
use tracing::{error, info};

use super::{BooksState, model};

pub async fn get_all(
    State(state): State<BooksState>,
) -> Result<Json<Vec<model::BookSummary>>, StatusCode> {
    info!("fetching all books");
    let books = model::all(&state.pool).await.map_err(|error| {
        error!(error = %error, "failed to fetch books from database");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    info!(books = books.len(), "books fetched");
    Ok(Json(books))
}
