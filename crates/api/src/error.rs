use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use conta_verde_application::{ApplicationError, AuthError, RepositoryError};
use serde::Serialize;

#[derive(Debug)]
pub(crate) enum ApiError {
    Application(ApplicationError),
    InvalidCursor,
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::InvalidCursor => (StatusCode::BAD_REQUEST, "invalid cursor".to_owned()),
            Self::Application(error) => describe(error),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

fn describe(error: &ApplicationError) -> (StatusCode, String) {
    match error {
        ApplicationError::Domain(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
        ApplicationError::Repository(
            error @ (RepositoryError::DuplicateDocument | RepositoryError::DuplicateEmail),
        ) => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::Auth(error) => {
            let status = match error {
                AuthError::InvalidCredentials | AuthError::Unauthenticated => {
                    StatusCode::UNAUTHORIZED
                }
                // Same answer for "not yours" and "does not exist", so ids cannot be probed.
                AuthError::Forbidden => StatusCode::NOT_FOUND,
            };
            (status, error.to_string())
        }
        ApplicationError::PartyNotFound
        | ApplicationError::Repository(RepositoryError::NotFound) => {
            (StatusCode::NOT_FOUND, "party not found".to_owned())
        }
        ApplicationError::LookupDisabled => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::RecheckTooSoon => (StatusCode::TOO_MANY_REQUESTS, error.to_string()),
        // Internal details can leak: log them, but show clients a generic message.
        ApplicationError::Repository(RepositoryError::Storage(detail)) => internal(detail),
        ApplicationError::Secret(error) => internal(&error.to_string()),
    }
}

fn internal(detail: &str) -> (StatusCode, String) {
    tracing::error!(%detail, "internal failure");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal error".to_owned(),
    )
}
