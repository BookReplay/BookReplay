use super::{ClippingsState, model, view::ImportResponse};
use axum::{Json, extract::State, http::StatusCode};
use rekindle_core::EnrichedClipping;
use rekindle_kindle::parse_clippings;
use tracing::{error, info};

pub async fn get_all(
    State(state): State<ClippingsState>,
) -> Result<Json<Vec<EnrichedClipping>>, (StatusCode, Json<ImportResponse>)> {
    info!("fetching all clippings");
    let clippings = model::all(&state.pool).await.map_err(|error| {
        error!(error = %error, "failed to fetch clippings from database");
        internal_error("failed to get clippings")
    })?;
    info!(clippings = clippings.len(), "clippings fetched");
    Ok(Json(clippings))
}

pub async fn import(
    State(state): State<ClippingsState>,
    content: String,
) -> Result<(StatusCode, Json<ImportResponse>), (StatusCode, Json<ImportResponse>)> {
    info!(content_bytes = content.len(), "clippings import started");
    let clippings = parse_clippings(&content);
    let parsed = clippings.len();
    info!(parsed, "clippings file parsed");

    let result = model::insert(&state.pool, &clippings)
        .await
        .map_err(|error| {
            error!(parsed, error = %error, "failed to store parsed clippings");
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
        )),
    ))
}

fn internal_error(message: &'static str) -> (StatusCode, Json<ImportResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ImportResponse::error(message)),
    )
}
