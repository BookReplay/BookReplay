use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};
use serde::Deserialize;
use time::OffsetDateTime;
use tracing::error;

use super::{
    ReviewsState, model,
    scheduler::{ReviewRating, schedule_review},
};
use crate::{
    error::{ApiError, database_cause, owner_id},
    features::auth::AuthSession,
};

#[derive(Deserialize)]
pub struct SessionQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct ReviewRequest {
    rating: ReviewRating,
}

pub async fn session(
    State(state): State<ReviewsState>,
    auth_session: AuthSession,
    Query(query): Query<SessionQuery>,
) -> Result<Json<Vec<model::SessionHighlight>>, ApiError> {
    let limit = query.limit.unwrap_or(10).clamp(1, 100);
    let user_id = owner_id(auth_session)?;
    let highlights = model::session(&state.pool, user_id, limit, OffsetDateTime::now_utc())
        .await
        .map_err(internal_error)?;
    Ok(Json(highlights))
}

pub async fn review(
    State(state): State<ReviewsState>,
    auth_session: AuthSession,
    Path(highlight_id): Path<i64>,
    request: Result<Json<ReviewRequest>, JsonRejection>,
) -> Result<Json<model::ReviewResponse>, ApiError> {
    let Json(request) =
        request.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid review rating"))?;
    let user_id = owner_id(auth_session)?;
    let mut transaction = state.pool.begin().await.map_err(internal_error)?;
    let state = model::find_for_update(&mut transaction, user_id, highlight_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "highlight not found"))?;

    if state.archived_at.is_some() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "highlight is already archived",
        ));
    }
    let now = OffsetDateTime::now_utc();
    // A repeated submission must not grow the interval or count toward the daily goal twice.
    if state.next_review_at.is_some_and(|date| date > now) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "highlight is not due for review",
        ));
    }

    let schedule = schedule_review(
        now,
        state.current_interval_days,
        state.review_count,
        request.rating,
    );
    let response = model::save_review(
        &mut transaction,
        user_id,
        highlight_id,
        &state,
        request.rating,
        &schedule,
        now,
    )
    .await
    .map_err(internal_error)?;
    transaction.commit().await.map_err(internal_error)?;

    Ok(Json(response))
}

pub async fn streak(
    State(state): State<ReviewsState>,
    auth_session: AuthSession,
) -> Result<Json<model::StreakResponse>, ApiError> {
    let user_id = owner_id(auth_session)?;
    let streak = model::streak(&state.pool, user_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(streak))
}

fn internal_error(error: sqlx::Error) -> ApiError {
    error!(
        cause = database_cause(&error),
        "highlight review request failed"
    );
    ApiError::internal("review request failed")
}
