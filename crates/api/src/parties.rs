use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use conta_verde_application::{
    ApplicationError, CnpjRegistry, IdentityRepository, PartyCursor, PartyRepository, SecretService,
};
use conta_verde_domain::{DocumentCheck, Party, PartyDraft, PartyId};
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
    document_check: DocumentCheckResponse,
}

/// What the federal registry said about a CNPJ. `needs_attention` is true when the registry does
/// not know the company, was unreachable, or the company is not active, so the UI never has to
/// know the registry's wording.
#[derive(Serialize)]
struct DocumentCheckResponse {
    status: &'static str,
    legal_name: Option<String>,
    situation: Option<String>,
    needs_attention: bool,
}

impl From<&DocumentCheck> for DocumentCheckResponse {
    fn from(check: &DocumentCheck) -> Self {
        let (status, legal_name, situation) = match check {
            DocumentCheck::NotApplicable => ("not_applicable", None, None),
            DocumentCheck::NotFound => ("not_found", None, None),
            DocumentCheck::Unverified => ("unverified", None, None),
            DocumentCheck::Verified {
                legal_name,
                situation,
            } => (
                "verified",
                Some(legal_name.clone()),
                Some(situation.clone()),
            ),
        };
        Self {
            status,
            legal_name,
            situation,
            needs_attention: check.needs_attention(),
        }
    }
}

impl From<&Party> for PartyResponse {
    fn from(party: &Party) -> Self {
        Self {
            id: party.id().as_uuid(),
            company_id: party.company_id().as_uuid(),
            name: party.name().to_owned(),
            document: party
                .document()
                .map(|document| document.as_str().to_owned()),
            email: party.email().map(|email| email.as_str().to_owned()),
            is_customer: party.is_customer(),
            is_supplier: party.is_supplier(),
            document_check: DocumentCheckResponse::from(party.document_check()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PartyPageResponse {
    items: Vec<PartyResponse>,
    next_cursor: Option<String>,
}

pub(crate) async fn register_party<R, C, I, S>(
    State(state): State<Arc<AppState<R, C, I, S>>>,
    CompanyAccess(company_id): CompanyAccess,
    Json(request): Json<RegisterPartyRequest>,
) -> Result<(StatusCode, Json<PartyResponse>), ApiError>
where
    R: PartyRepository,
    C: CnpjRegistry,
    I: IdentityRepository,
    S: SecretService,
{
    let party = state.parties.register(company_id, request.into()).await?;
    Ok((StatusCode::CREATED, Json(PartyResponse::from(&party))))
}

/// Asks the registry again about a party's CNPJ and returns the party with the new answer.
pub(crate) async fn verify_document<R, C, I, S>(
    State(state): State<Arc<AppState<R, C, I, S>>>,
    CompanyAccess(company_id): CompanyAccess,
    // A string, parsed below: axum's own rejection for a malformed id is plain text, unlike the
    // JSON errors of the rest of the API. The company id was already validated by CompanyAccess.
    Path((_, party_id)): Path<(Uuid, String)>,
) -> Result<Json<PartyResponse>, ApiError>
where
    R: PartyRepository,
    C: CnpjRegistry,
    I: IdentityRepository,
    S: SecretService,
{
    let party_id = Uuid::parse_str(&party_id)
        .map(PartyId::from_uuid)
        .map_err(|_| ApplicationError::PartyNotFound)?;
    let party = state.parties.verify_document(company_id, party_id).await?;
    Ok(Json(PartyResponse::from(&party)))
}

pub(crate) async fn list_parties<R, C, I, S>(
    State(state): State<Arc<AppState<R, C, I, S>>>,
    CompanyAccess(company_id): CompanyAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<PartyPageResponse>, ApiError>
where
    R: PartyRepository,
    C: CnpjRegistry,
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
