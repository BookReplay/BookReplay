use super::{ClippingsState, model, view::ImportResponse};
use crate::features::auth::AuthSession;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use bookreplay_core::EnrichedClipping;
use bookreplay_kindle::parse_clippings;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

pub async fn get_all(
    State(state): State<ClippingsState>,
    auth_session: AuthSession,
) -> Result<Json<Vec<EnrichedClipping>>, (StatusCode, Json<ImportResponse>)> {
    info!("fetching all clippings");
    let user_id = authenticated_user_id(auth_session)?;
    let clippings = model::all(&state.pool, user_id).await.map_err(|_error| {
        error!("failed to fetch clippings from database");
        internal_error("failed to get clippings")
    })?;
    info!(clippings = clippings.len(), "clippings fetched");
    Ok(Json(clippings))
}

pub async fn import(
    State(state): State<ClippingsState>,
    auth_session: AuthSession,
    content: String,
) -> Result<(StatusCode, Json<ImportResponse>), (StatusCode, Json<ImportResponse>)> {
    info!(content_bytes = content.len(), "clippings import started");
    let clippings = parse_clippings(&content);
    let parsed = clippings.len();
    let preview = clippings
        .iter()
        .find(|clipping| !clipping.content.trim().is_empty())
        .map(|clipping| clipping.content.clone().into_boxed_str());
    info!(parsed, "clippings file parsed");

    let user_id = authenticated_user_id(auth_session)?;
    let result = model::insert(&state.pool, user_id, &clippings)
        .await
        .map_err(|_error| {
            error!(parsed, "failed to store parsed clippings");
            internal_error("clippings import failed")
        })?;
    info!(
        parsed,
        clippings_inserted = result.clippings,
        duplicates = parsed - result.clippings,
        books_inserted = result.books,
        "clippings stored"
    );

    info!(
        parsed,
        clippings_inserted = result.clippings,
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
    Json(request): Json<UpdateClippingRequest>,
) -> Result<Json<UpdateClippingResponse>, (StatusCode, Json<ImportResponse>)> {
    if request.content.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ImportResponse::error("highlight text must not be empty")),
        ));
    }

    let user_id = authenticated_user_id(auth_session)?;
    let content = model::update_content(&state.pool, user_id, clipping_id, &request.content)
        .await
        .map_err(|_error| {
            error!(clipping_id, "failed to update clipping");
            internal_error("failed to update highlight")
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ImportResponse::error("highlight not found")),
            )
        })?;

    Ok(Json(UpdateClippingResponse { content }))
}

fn internal_error(message: &'static str) -> (StatusCode, Json<ImportResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ImportResponse::error(message)),
    )
}

fn authenticated_user_id(
    auth_session: AuthSession,
) -> Result<i16, (StatusCode, Json<ImportResponse>)> {
    auth_session.user.map(|user| user.id).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ImportResponse::error("authentication required")),
        )
    })
}
