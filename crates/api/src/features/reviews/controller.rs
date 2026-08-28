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
    Query(query): Query<SessionQuery>,
) -> Result<Json<Vec<model::SessionHighlight>>, ApiError> {
    let limit = query.limit.unwrap_or(10).clamp(1, 100);
    let highlights = model::session(&state.pool, limit, OffsetDateTime::now_utc())
        .await
        .map_err(internal_error)?;
    Ok(Json(highlights))
}

pub async fn review(
    State(state): State<ReviewsState>,
    Path(highlight_id): Path<i64>,
    Json(request): Json<ReviewRequest>,
) -> Result<Json<model::ReviewResponse>, ApiError> {
    let mut transaction = state.pool.begin().await.map_err(internal_error)?;
    let state = model::find_for_update(&mut transaction, highlight_id)
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

fn internal_error(error: sqlx::Error) -> ApiError {
    error!(error = %error, "highlight review request failed");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, "review request failed")
}

fn api_error(status: StatusCode, message: &'static str) -> ApiError {
    (status, Json(ErrorResponse { message }))
}
