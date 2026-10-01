use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::features::auth::AuthSession;

#[derive(Debug)]
pub(crate) struct ApiError {
    pub(crate) status: StatusCode,
    message: &'static str,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: &'static str,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, message: &'static str) -> Self {
        Self { status, message }
    }

    pub(crate) fn internal(message: &'static str) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

pub(crate) fn owner_id(auth_session: AuthSession) -> Result<i16, ApiError> {
    auth_session
        .user
        .map(|user| user.id)
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "authentication required"))
}

/// Names a database failure for the logs without the statement, parameters or row data.
pub(crate) fn database_cause(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Database(error) => match (error.code(), error.constraint()) {
            (Some(code), Some(constraint)) => format!("SQLSTATE {code} on {constraint}"),
            (Some(code), None) => format!("SQLSTATE {code}"),
            _ => "database rejected the statement".to_owned(),
        },
        sqlx::Error::Io(error) => format!("connection I/O error: {}", error.kind()),
        sqlx::Error::PoolTimedOut => "connection pool timed out".to_owned(),
        sqlx::Error::PoolClosed => "connection pool closed".to_owned(),
        sqlx::Error::Tls(_) => "TLS error".to_owned(),
        sqlx::Error::RowNotFound => "row not found".to_owned(),
        sqlx::Error::Encode(_) => "parameter could not be encoded".to_owned(),
        sqlx::Error::Decode(_) | sqlx::Error::ColumnDecode { .. } => {
            "row could not be decoded".to_owned()
        }
        _ => "unexpected database driver error".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_share_one_json_shape() {
        let response = ApiError::new(StatusCode::NOT_FOUND, "book not found").into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn database_cause_never_includes_driver_details() {
        assert_eq!(
            database_cause(&sqlx::Error::PoolTimedOut),
            "connection pool timed out"
        );
        assert_eq!(
            database_cause(&sqlx::Error::Protocol("private text".into())),
            "unexpected database driver error"
        );
    }
}
