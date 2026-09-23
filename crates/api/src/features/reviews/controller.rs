use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tracing::error;

use super::{
    ReviewsState, model,
    scheduler::{ReviewRating, schedule_review},
};
use crate::features::auth::AuthSession;

#[derive(Deserialize)]
pub struct SessionQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct ReviewRequest {
    rating: ReviewRating,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    message: &'static str,
}

type ApiError = (StatusCode, Json<ErrorResponse>);

pub async fn session(
    State(state): State<ReviewsState>,
    auth_session: AuthSession,
    Query(query): Query<SessionQuery>,
) -> Result<Json<Vec<model::SessionHighlight>>, ApiError> {
    let limit = query.limit.unwrap_or(10).clamp(1, 100);
    let user_id = authenticated_user_id(auth_session)?;
    let highlights = model::session(&state.pool, user_id, limit, OffsetDateTime::now_utc())
        .await
        .map_err(internal_error)?;
    Ok(Json(highlights))
}

pub async fn review(
    State(state): State<ReviewsState>,
    auth_session: AuthSession,
    Path(highlight_id): Path<i64>,
    Json(request): Json<ReviewRequest>,
) -> Result<Json<model::ReviewResponse>, ApiError> {
    let user_id = authenticated_user_id(auth_session)?;
    let mut transaction = state.pool.begin().await.map_err(internal_error)?;
    let state = model::find_for_update(&mut transaction, user_id, highlight_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "highlight not found"))?;

    if state.archived_at.is_some() {
        return Err(api_error(
            StatusCode::CONFLICT,
            "highlight is already archived",
        ));
    }

    let now = OffsetDateTime::now_utc();
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
    let user_id = authenticated_user_id(auth_session)?;
    let streak = model::streak(&state.pool, user_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(streak))
}

fn internal_error(_error: sqlx::Error) -> ApiError {
    error!("highlight review request failed");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, "review request failed")
}

fn api_error(status: StatusCode, message: &'static str) -> ApiError {
    (status, Json(ErrorResponse { message }))
}

fn authenticated_user_id(auth_session: AuthSession) -> Result<i16, ApiError> {
    auth_session
        .user
        .map(|user| user.id)
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "authentication required"))
}
