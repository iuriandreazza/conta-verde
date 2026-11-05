use axum::Json;
use axum::extract::{Path, State};
use conta_verde_application::PostalCodeEntry;
use serde::Serialize;
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::{Ports, SharedState};

/// What a postal code stands for, to fill an address form in. Only the shape of the address: the
/// number and the complement are the user's to say.
#[derive(Serialize)]
pub(crate) struct PostalCodeResponse {
    street: Option<String>,
    district: Option<String>,
    city: String,
    state: String,
}

impl From<PostalCodeEntry> for PostalCodeResponse {
    fn from(entry: PostalCodeEntry) -> Self {
        Self {
            street: entry.street,
            district: entry.district,
            city: entry.city,
            state: entry.state,
        }
    }
}

/// Finds the address of a postal code, written with or without the hyphen. It needs a session and
/// a company like every other route, so the installation is not an open proxy to a third party.
/// Nothing is stored or recorded.
pub(crate) async fn find_postal_code<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(..): CompanyAccess,
    Path((_, code)): Path<(Uuid, String)>,
) -> Result<Json<PostalCodeResponse>, ApiError> {
    let entry = state.postal_codes.lookup(&code).await?;
    Ok(Json(PostalCodeResponse::from(entry)))
}
