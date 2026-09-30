use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};
use bookreplay_openlibrary::BookCandidate;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use super::{BooksState, model};
use crate::features::auth::AuthSession;

#[derive(Deserialize)]
pub struct SearchQuery {
    q: Option<String>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    error: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);

pub async fn get_all(
    State(state): State<BooksState>,
    auth_session: AuthSession,
) -> Result<Json<Vec<model::BookSummary>>, StatusCode> {
    info!("fetching all books");
    let user_id = auth_session
        .user
        .map(|user| user.id)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let books = model::all(&state.pool, user_id).await.map_err(|_error| {
        error!("failed to fetch books from database");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    info!(books = books.len(), "books fetched");
    Ok(Json(books))
}

pub async fn search(
    State(state): State<BooksState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<BookCandidate>>, ApiError> {
    let query = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "search query must not be empty"))?;
    state
        .open_library
        .search(query)
        .await
        .map(Json)
        .map_err(|_error| {
            error!("Book metadata search failed");
            api_error(StatusCode::BAD_GATEWAY, "Book metadata search failed")
        })
}

pub async fn identify(
    State(state): State<BooksState>,
    auth_session: AuthSession,
    Path(book_id): Path<i64>,
    candidate: Result<Json<BookCandidate>, JsonRejection>,
) -> Result<Json<model::BookSummary>, ApiError> {
    let Json(candidate) =
        candidate.map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid book identification"))?;
    validate_candidate(&candidate)?;

    let user_id = auth_session
        .user
        .map(|user| user.id)
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "authentication required"))?;
    model::identify(&state.pool, user_id, book_id, &candidate)
        .await
        .map_err(|_error| {
            error!(book_id, "failed to identify book");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "failed to identify book")
        })?
        .map(Json)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "book not found"))
}

fn validate_candidate(candidate: &BookCandidate) -> Result<(), ApiError> {
    if !candidate.validate() {
        return Err(api_error(StatusCode::BAD_REQUEST, "invalid book candidate"));
    }
    Ok(())
}

fn api_error(status: StatusCode, message: &str) -> ApiError {
    (
        status,
        Json(ErrorResponse {
            error: message.to_owned(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use bookreplay_openlibrary::BookCandidate;

    use super::validate_candidate;

    fn candidate(key: &str, title: &str) -> BookCandidate {
        BookCandidate {
            provider: bookreplay_openlibrary::Provider::OpenLibrary,
            provider_id: key.into(),
            title: title.into(),
            authors: vec![],
            cover_url: None,
            first_publish_year: None,
            edition_count: None,
            isbns: vec![],
        }
    }

    #[test]
    fn candidate_accepts_open_library_work_key_and_title() {
        assert!(validate_candidate(&candidate("/works/OL123W", "A Book")).is_ok());
    }

    #[test]
    fn candidate_rejects_non_work_key() {
        let error = validate_candidate(&candidate("/authors/OL123A", "A Book"))
            .expect_err("author key should be rejected");

        assert_eq!(error.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn candidate_rejects_blank_title() {
        let error = validate_candidate(&candidate("/works/OL123W", "  "))
            .expect_err("blank title should be rejected");

        assert_eq!(error.0, StatusCode::BAD_REQUEST);
    }
}
