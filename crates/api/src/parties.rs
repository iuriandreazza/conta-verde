use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use conta_verde_application::{IdentityRepository, PartyCursor, PartyRepository, SecretService};
use conta_verde_domain::{Party, PartyDraft, PartyId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::access::CompanyAccess;
use crate::error::ApiError;

#[derive(Deserialize)]
pub(crate) struct RegisterPartyRequest {
    name: String,
    document: Option<String>,
    email: Option<String>,
    is_customer: bool,
    is_supplier: bool,
}

impl From<RegisterPartyRequest> for PartyDraft {
    fn from(request: RegisterPartyRequest) -> Self {
        Self {
            name: request.name,
            document: request.document,
            email: request.email,
            is_customer: request.is_customer,
            is_supplier: request.is_supplier,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct PartyResponse {
    id: Uuid,
    company_id: Uuid,
    name: String,
    document: Option<String>,
    email: Option<String>,
    is_customer: bool,
    is_supplier: bool,
}

impl From<&Party> for PartyResponse {
    fn from(party: &Party) -> Self {
        Self {
            id: party.id().as_uuid(),
            company_id: party.company_id().as_uuid(),
            name: party.name().to_owned(),
            document: party
                .document()
                .map(|document| document.digits().to_owned()),
            email: party.email().map(|email| email.as_str().to_owned()),
            is_customer: party.is_customer(),
            is_supplier: party.is_supplier(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PartyPageResponse {
    items: Vec<PartyResponse>,
    next_cursor: Option<String>,
}

pub(crate) async fn register_party<R, I, S>(
    State(state): State<Arc<AppState<R, I, S>>>,
    CompanyAccess(company_id): CompanyAccess,
    Json(request): Json<RegisterPartyRequest>,
) -> Result<(StatusCode, Json<PartyResponse>), ApiError>
where
    R: PartyRepository,
    I: IdentityRepository,
    S: SecretService,
{
    let party = state.parties.register(company_id, request.into()).await?;
    Ok((StatusCode::CREATED, Json(PartyResponse::from(&party))))
}

pub(crate) async fn list_parties<R, I, S>(
    State(state): State<Arc<AppState<R, I, S>>>,
    CompanyAccess(company_id): CompanyAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<PartyPageResponse>, ApiError>
where
    R: PartyRepository,
    I: IdentityRepository,
    S: SecretService,
{
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .parties
        .list(company_id, after.as_ref(), query.limit)
        .await?;
    Ok(Json(PartyPageResponse {
        items: page.items.iter().map(PartyResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

// Clients treat the cursor as opaque. The id comes first because it has a fixed shape,
// so a name containing the separator cannot confuse parsing.
fn encode_cursor(cursor: &PartyCursor) -> String {
    format!("{}|{}", cursor.id.as_uuid(), cursor.name)
}

fn decode_cursor(raw: &str) -> Result<PartyCursor, ApiError> {
    let (id, name) = raw.split_once('|').ok_or(ApiError::InvalidCursor)?;
    let id = Uuid::parse_str(id).map_err(|_| ApiError::InvalidCursor)?;
    Ok(PartyCursor {
        name: name.to_owned(),
        id: PartyId::from_uuid(id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_survives_separators_in_names() {
        let original = PartyCursor {
            name: "Ana | Filhos".into(),
            id: PartyId::generate(),
        };
        let decoded = decode_cursor(&encode_cursor(&original)).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn garbage_cursors_are_rejected() {
        assert!(decode_cursor("nonsense").is_err());
        assert!(decode_cursor("not-a-uuid|Ana").is_err());
    }
}
