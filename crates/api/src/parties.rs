use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use conta_verde_application::{
    ApplicationError, CnpjRegistry, IdentityRepository, PartyCursor, PartyKind, PartyQuery,
    PartyRepository, PartySort, SecretService,
};
use conta_verde_domain::{Actor, AuditRecord, DocumentCheck, Party, PartyDraft, PartyId};
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
    /// Text found in the name or e-mail, or in the document ignoring punctuation.
    q: Option<String>,
    /// `customer` or `supplier`.
    kind: Option<String>,
    /// `name` (default) or `-name` for descending, the convention every list follows.
    sort: Option<String>,
}

impl ListQuery {
    fn party_query(&self) -> Result<PartyQuery, ApiError> {
        let mut query = PartyQuery::default();
        if let Some(text) = &self.q {
            if text.chars().count() > PartyQuery::MAX_SEARCH_CHARS {
                return Err(ApiError::InvalidQuery("invalid q: at most 100 characters"));
            }
            query = query.searching(text);
        }
        query = match self.kind.as_deref() {
            None => query,
            Some("customer") => query.of_kind(PartyKind::Customer),
            Some("supplier") => query.of_kind(PartyKind::Supplier),
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid kind: expected customer or supplier",
                ));
            }
        };
        Ok(match self.sort.as_deref() {
            None | Some("name") => query,
            Some("-name") => query.sorted(PartySort::NameDesc),
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid sort: expected name or -name",
                ));
            }
        })
    }
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
    CompanyAccess(company_id, actor): CompanyAccess,
    Json(request): Json<RegisterPartyRequest>,
) -> Result<(StatusCode, Json<PartyResponse>), ApiError>
where
    R: PartyRepository,
    C: CnpjRegistry,
    I: IdentityRepository,
    S: SecretService,
{
    let party = state
        .parties
        .register(company_id, &actor, request.into())
        .await?;
    Ok((StatusCode::CREATED, Json(PartyResponse::from(&party))))
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_party_id(raw: &str) -> Result<PartyId, ApiError> {
    Uuid::parse_str(raw)
        .map(PartyId::from_uuid)
        .map_err(|_| ApplicationError::PartyNotFound.into())
}

/// Asks the registry again about a party's CNPJ and returns the party with the new answer.
pub(crate) async fn verify_document<R, C, I, S>(
    State(state): State<Arc<AppState<R, C, I, S>>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, party_id)): Path<(Uuid, String)>,
) -> Result<Json<PartyResponse>, ApiError>
where
    R: PartyRepository,
    C: CnpjRegistry,
    I: IdentityRepository,
    S: SecretService,
{
    let party_id = parse_party_id(&party_id)?;
    let party = state
        .parties
        .verify_document(company_id, &actor, party_id)
        .await?;
    Ok(Json(PartyResponse::from(&party)))
}

/// What was recorded about a party, oldest first. The array order is the order of the trail; the
/// store's own counter is not exposed, because it is shared by every company and would show how
/// busy the installation is.
#[derive(Serialize)]
pub(crate) struct HistoryEntryResponse {
    /// Milliseconds since the Unix epoch; the client formats it in the user's time zone.
    at: i64,
    actor: ActorResponse,
    action: &'static str,
    changes: Vec<ChangeResponse>,
}

#[derive(Serialize)]
struct ActorResponse {
    kind: &'static str,
    id: Uuid,
    /// The name the actor had when they made the change.
    name: String,
}

#[derive(Serialize)]
struct ChangeResponse {
    field: String,
    from: Option<String>,
    to: Option<String>,
}

impl From<&AuditRecord> for HistoryEntryResponse {
    fn from(record: &AuditRecord) -> Self {
        let Actor::User { id, name } = &record.entry.actor;
        Self {
            at: record.at_millis,
            actor: ActorResponse {
                kind: "user",
                id: id.as_uuid(),
                name: name.clone(),
            },
            action: record.entry.action.as_str(),
            changes: record
                .entry
                .changes
                .iter()
                .map(|change| ChangeResponse {
                    field: change.field.clone(),
                    from: change.from.clone(),
                    to: change.to.clone(),
                })
                .collect(),
        }
    }
}

pub(crate) async fn party_history<R, C, I, S>(
    State(state): State<Arc<AppState<R, C, I, S>>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, party_id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError>
where
    R: PartyRepository,
    C: CnpjRegistry,
    I: IdentityRepository,
    S: SecretService,
{
    let party_id = parse_party_id(&party_id)?;
    let history = state.parties.history(company_id, party_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

pub(crate) async fn list_parties<R, C, I, S>(
    State(state): State<Arc<AppState<R, C, I, S>>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Query(query): Query<ListQuery>,
) -> Result<Json<PartyPageResponse>, ApiError>
where
    R: PartyRepository,
    C: CnpjRegistry,
    I: IdentityRepository,
    S: SecretService,
{
    let filter = query.party_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .parties
        .list(company_id, &filter, after.as_ref(), query.limit)
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
    // No stored name holds a NUL, so a cursor that does was made up, and Postgres would fail on it.
    if name.contains('\0') {
        return Err(ApiError::InvalidCursor);
    }
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
        let id = PartyId::generate().as_uuid();
        assert!(decode_cursor(&format!("{id}|An\0a")).is_err());
    }
}
