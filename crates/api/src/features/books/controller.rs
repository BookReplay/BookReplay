use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};
use bookreplay_openlibrary::BookCandidate;
use serde::Deserialize;
use tracing::error;

use super::{BooksState, model};
use crate::{
    error::{ApiError, database_cause, user_id},
    features::auth::AuthSession,
};

#[derive(Deserialize)]
pub struct SearchQuery {
    q: Option<String>,
}

pub async fn get_all(
    State(state): State<BooksState>,
    auth_session: AuthSession,
) -> Result<Json<Vec<model::BookSummary>>, ApiError> {
    let user_id = user_id(auth_session)?;
    model::all(&state.pool, user_id)
        .await
        .map(Json)
        .map_err(|error| {
            error!(cause = database_cause(&error), "failed to fetch books");
            ApiError::internal("failed to load books")
        })
}

pub async fn clippings(
    State(state): State<BooksState>,
    auth_session: AuthSession,
    Path(book_id): Path<i64>,
) -> Result<Json<Vec<model::BookClipping>>, ApiError> {
    let user_id = user_id(auth_session)?;
    model::clippings(&state.pool, user_id, book_id)
        .await
        .map(Json)
        .map_err(|error| {
            error!(
                book_id,
                cause = database_cause(&error),
                "failed to fetch book highlights"
            );
            ApiError::internal("failed to load highlights")
        })
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
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "search query must not be empty"))?;
    let open_library = state.open_library.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            "metadata lookup is turned off on this instance",
        )
    })?;
    open_library
        .search(query)
        .await
        .map(Json)
        .map_err(|_error| {
            error!("Book metadata search failed");
            ApiError::new(StatusCode::BAD_GATEWAY, "Book metadata search failed")
        })
}

pub async fn identify(
    State(state): State<BooksState>,
    auth_session: AuthSession,
    Path(book_id): Path<i64>,
    candidate: Result<Json<BookCandidate>, JsonRejection>,
) -> Result<Json<model::BookSummary>, ApiError> {
    let Json(candidate) = candidate
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid book identification"))?;
    validate_candidate(&candidate)?;

    let user_id = user_id(auth_session)?;
    model::identify(&state.pool, user_id, book_id, &candidate)
        .await
        .map_err(|error| {
            error!(
                book_id,
                cause = database_cause(&error),
                "failed to identify book"
            );
            ApiError::internal("failed to identify book")
        })?
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "book not found"))
}

fn validate_candidate(candidate: &BookCandidate) -> Result<(), ApiError> {
    if !candidate.validate() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid book candidate",
        ));
    }
    Ok(())
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

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn candidate_rejects_blank_title() {
        let error = validate_candidate(&candidate("/works/OL123W", "  "))
            .expect_err("blank title should be rejected");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }
}
