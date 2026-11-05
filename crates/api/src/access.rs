use std::sync::Arc;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use conta_verde_application::{
    AuthError, CnpjRegistry, IdentityRepository, PartyRepository, SecretService,
};
use conta_verde_domain::CompanyId;
use uuid::Uuid;

use crate::AppState;
use crate::error::ApiError;
use crate::session::require_token;

/// Proof that the caller's session belongs to a member of the `{company_id}` in the route.
///
/// Declare it as a handler argument before any body or query extractor: it runs first, so
/// unauthenticated callers are rejected before anything else is parsed, and a company route
/// cannot be written without choosing whether to check access.
pub(crate) struct CompanyAccess(pub CompanyId);

impl<R, C, I, S> FromRequestParts<Arc<AppState<R, C, I, S>>> for CompanyAccess
where
    R: PartyRepository + 'static,
    C: CnpjRegistry + 'static,
    I: IdentityRepository + 'static,
    S: SecretService + 'static,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState<R, C, I, S>>,
    ) -> Result<Self, ApiError> {
        // Owned, because extracting the path below borrows `parts` mutably.
        let token = require_token(&parts.headers)?.to_owned();
        // A malformed id is indistinguishable from a company the caller cannot see.
        let Path(raw_id) = Path::<Uuid>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::Application(AuthError::Forbidden.into()))?;
        let company_id = CompanyId::from_uuid(raw_id);
        state.auth.authorize(&token, company_id).await?;
        Ok(Self(company_id))
    }
}
