use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;

use crate::error::ApiError;

/// A JSON body, like axum's `Json`, whose rejection is the `{"error": ...}` body every other
/// failure of this API has, instead of plain text. A price sent as a JSON number, for instance, is
/// refused this way: decimals travel as strings.
pub(crate) struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(ApiError::InvalidBody(rejection)),
        }
    }
}

/// Query parameters, like axum's `Query`, refused in the API's own error shape.
pub(crate) struct ApiQuery<T>(pub T);

impl<S, T> axum::extract::FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, ApiError> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(Self(value)),
            Err(rejection) => Err(ApiError::InvalidParameters(rejection)),
        }
    }
}

pub(crate) fn describe(rejection: &JsonRejection) -> (axum::http::StatusCode, String) {
    (rejection.status(), rejection.body_text())
}

/// The `active` of a body that registers or replaces. A missing one means `default` when
/// registering, and is refused when replacing (`default` is `None`): a replacement without it
/// would switch a deactivated record back on.
pub(crate) fn active_or(
    active: Option<bool>,
    default: Option<bool>,
    message: &'static str,
) -> Result<bool, ApiError> {
    active.or(default).ok_or(ApiError::InvalidField(message))
}
