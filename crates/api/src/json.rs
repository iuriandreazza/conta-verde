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

pub(crate) fn describe(rejection: &JsonRejection) -> (axum::http::StatusCode, String) {
    (rejection.status(), rejection.body_text())
}
