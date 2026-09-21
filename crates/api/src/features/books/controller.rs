use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};
use bookreplay_openlibrary::OpenLibraryBook;
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
) -> Result<Json<Vec<OpenLibraryBook>>, ApiError> {
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
            error!("Open Library search failed");
            api_error(StatusCode::BAD_GATEWAY, "Open Library search failed")
        })
}

pub async fn identify(
    State(state): State<BooksState>,
    auth_session: AuthSession,
    Path(book_id): Path<i64>,
    candidate: Result<Json<OpenLibraryBook>, JsonRejection>,
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

fn validate_candidate(candidate: &OpenLibraryBook) -> Result<(), ApiError> {
    let valid_key = candidate
        .open_library_key
        .strip_prefix("/works/OL")
        .and_then(|key| key.strip_suffix('W'))
        .is_some_and(|digits| {
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        });
    if !valid_key {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid Open Library work key",
        ));
    }
    if candidate.title.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "book title must not be empty",
        ));
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
    use bookreplay_openlibrary::OpenLibraryBook;

    use super::validate_candidate;

    fn candidate(key: &str, title: &str) -> OpenLibraryBook {
        OpenLibraryBook {
            open_library_key: key.into(),
            title: title.into(),
            authors: vec![],
            cover_id: None,
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
