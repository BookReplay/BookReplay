use super::{ClippingsState, model, view::ImportResponse};
use crate::{
    error::{ApiError, database_cause, owner_id},
    features::auth::AuthSession,
};
use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
};
use bookreplay_kindle::parse_clippings;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

pub async fn import(
    State(state): State<ClippingsState>,
    auth_session: AuthSession,
    content: String,
) -> Result<(StatusCode, Json<ImportResponse>), ApiError> {
    info!(content_bytes = content.len(), "clippings import started");
    let clippings = parse_clippings(&content);
    let parsed = clippings.len();
    let preview = clippings
        .iter()
        .find(|clipping| !clipping.content.trim().is_empty())
        .map(|clipping| clipping.content.clone().into_boxed_str());
    info!(parsed, "clippings file parsed");

    let user_id = owner_id(auth_session)?;
    let result = model::insert(&state.pool, user_id, &clippings)
        .await
        .map_err(|error| {
            error!(
                parsed,
                cause = database_cause(&error),
                "failed to store parsed clippings"
            );
            ApiError::internal("clippings import failed")
        })?;
    info!(
        parsed,
        clippings_inserted = result.clippings,
        duplicates = parsed - result.clippings,
        books_inserted = result.books,
        books_queued = result.books,
        "clippings import completed"
    );

    Ok((
        StatusCode::CREATED,
        Json(ImportResponse::success(
            parsed,
            result.clippings,
            result.books,
            preview,
        )),
    ))
}

#[derive(Deserialize)]
pub struct UpdateClippingRequest {
    content: String,
}

#[derive(Serialize)]
pub struct UpdateClippingResponse {
    content: String,
}

pub async fn update(
    State(state): State<ClippingsState>,
    auth_session: AuthSession,
    Path(clipping_id): Path<i64>,
    request: Result<Json<UpdateClippingRequest>, JsonRejection>,
) -> Result<Json<UpdateClippingResponse>, ApiError> {
    let Json(request) =
        request.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid highlight update"))?;
    if request.content.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "highlight text must not be empty",
        ));
    }

    let user_id = owner_id(auth_session)?;
    let content = model::update_content(&state.pool, user_id, clipping_id, &request.content)
        .await
        .map_err(|error| {
            error!(
                clipping_id,
                cause = database_cause(&error),
                "failed to update clipping"
            );
            ApiError::internal("failed to update highlight")
        })?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "highlight not found"))?;

    Ok(Json(UpdateClippingResponse { content }))
}
