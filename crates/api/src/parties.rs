use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, PartyKind, PartyQuery};
use conta_verde_domain::{
    Address, AddressDraft, Contact, ContactDraft, DocumentCheck, Party, PartyDraft, PartyId, Tag,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, field_or};
use crate::list_params;
use crate::{Ports, SharedState};

/// The body of both registering and changing a party: a change replaces every field, so what is
/// left out (the document, the e-mail, the address) is cleared.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PartyRequest {
    name: String,
    document: Option<String>,
    email: Option<String>,
    is_customer: bool,
    is_supplier: bool,
    /// Optional when registering (no), required when replacing: a body without it would
    /// otherwise drop the role.
    is_carrier: Option<bool>,
    is_seller: Option<bool>,
    address: Option<AddressRequest>,
    /// Optional when registering (none), required when replacing, like the roles: a body from a
    /// client that does not know them would otherwise drop them silently.
    contacts: Option<Vec<ContactRequest>>,
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContactRequest {
    name: String,
    job_title: Option<String>,
    email: Option<String>,
    phone: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddressRequest {
    postal_code: String,
    street: String,
    number: Option<String>,
    complement: Option<String>,
    district: Option<String>,
    city: String,
    state: String,
}

impl From<AddressRequest> for AddressDraft {
    fn from(address: AddressRequest) -> Self {
        Self {
            postal_code: address.postal_code,
            street: address.street,
            number: address.number,
            complement: address.complement,
            district: address.district,
            city: address.city,
            state: address.state,
        }
    }
}

impl PartyRequest {
    /// `registering` says what a missing `is_carrier`, `is_seller`, `contacts` or `tags` means:
    /// no (or none) when registering, and a refusal when replacing, where they must be said.
    fn into_draft(self, registering: bool) -> Result<PartyDraft, ApiError> {
        let default_role = registering.then_some(false);
        let is_carrier = field_or(
            self.is_carrier,
            default_role,
            "is_carrier is required when replacing a party",
        )?;
        let is_seller = field_or(
            self.is_seller,
            default_role,
            "is_seller is required when replacing a party",
        )?;
        let contacts = field_or(
            self.contacts,
            registering.then(Vec::new),
            "contacts is required when replacing a party",
        )?;
        let tags = field_or(
            self.tags,
            registering.then(Vec::new),
            "tags is required when replacing a party",
        )?;
        Ok(PartyDraft {
            name: self.name,
            document: self.document,
            email: self.email,
            is_customer: self.is_customer,
            is_supplier: self.is_supplier,
            is_carrier,
            is_seller,
            address: self.address.map(AddressDraft::from),
            contacts: contacts
                .into_iter()
                .map(|contact| ContactDraft {
                    name: contact.name,
                    job_title: contact.job_title,
                    email: contact.email,
                    phone: contact.phone,
                })
                .collect(),
            tags,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the name or e-mail, or in the document ignoring punctuation.
    q: Option<String>,
    /// `customer`, `supplier`, `carrier` or `seller`.
    kind: Option<String>,
    /// A tag the party carries, in any case.
    tag: Option<String>,
    /// `name` (default) or `-name` for descending, the convention every list follows.
    sort: Option<String>,
}

impl ListQuery {
    fn party_query(&self) -> Result<PartyQuery, ApiError> {
        let mut query = PartyQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        query = match self.kind.as_deref() {
            None => query,
            Some("customer") => query.of_kind(PartyKind::Customer),
            Some("supplier") => query.of_kind(PartyKind::Supplier),
            Some("carrier") => query.of_kind(PartyKind::Carrier),
            Some("seller") => query.of_kind(PartyKind::Seller),
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid kind: expected customer, supplier, carrier or seller",
                ));
            }
        };
        // Blank text means no filter, as for the search.
        if let Some(tag) = self.tag.as_deref().filter(|tag| !tag.trim().is_empty()) {
            let tag = Tag::parse(tag).map_err(|_| {
                ApiError::InvalidQuery("invalid tag: 1 to 30 characters, with no commas")
            })?;
            query = query.tagged(tag);
        }
        Ok(query.sorted(list_params::sort(self.sort.as_deref())?))
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
    is_carrier: bool,
    is_seller: bool,
    address: Option<AddressResponse>,
    contacts: Vec<ContactResponse>,
    tags: Vec<String>,
    document_check: DocumentCheckResponse,
}

#[derive(Serialize)]
struct ContactResponse {
    name: String,
    job_title: Option<String>,
    email: Option<String>,
    phone: Option<String>,
}

impl From<&Contact> for ContactResponse {
    fn from(contact: &Contact) -> Self {
        Self {
            name: contact.name().to_owned(),
            job_title: contact.job_title().map(str::to_owned),
            email: contact.email().map(|email| email.as_str().to_owned()),
            phone: contact.phone().map(str::to_owned),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct AddressResponse {
    postal_code: String,
    street: String,
    number: Option<String>,
    complement: Option<String>,
    district: Option<String>,
    city: String,
    state: String,
}

impl From<&Address> for AddressResponse {
    fn from(address: &Address) -> Self {
        Self {
            postal_code: address.postal_code().to_owned(),
            street: address.street().to_owned(),
            number: address.number().map(str::to_owned),
            complement: address.complement().map(str::to_owned),
            district: address.district().map(str::to_owned),
            city: address.city().to_owned(),
            state: address.state().to_owned(),
        }
    }
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
            is_carrier: party.is_carrier(),
            is_seller: party.is_seller(),
            address: party.address().map(AddressResponse::from),
            contacts: party.contacts().iter().map(ContactResponse::from).collect(),
            tags: party
                .tags()
                .iter()
                .map(|tag| tag.as_str().to_owned())
                .collect(),
            document_check: DocumentCheckResponse::from(party.document_check()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PartyPageResponse {
    items: Vec<PartyResponse>,
    next_cursor: Option<String>,
}

pub(crate) async fn register_party<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<PartyRequest>,
) -> Result<(StatusCode, Json<PartyResponse>), ApiError> {
    let party = state
        .parties
        .register(company_id, &actor, request.into_draft(true)?)
        .await?;
    Ok((StatusCode::CREATED, Json(PartyResponse::from(&party))))
}

/// Replaces every field of a party. A CNPJ that differs from the stored one is checked against
/// the registry; nothing is written, and nothing recorded, when nothing changes.
pub(crate) async fn update_party<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, party_id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<PartyRequest>,
) -> Result<Json<PartyResponse>, ApiError> {
    let party = state
        .parties
        .update(
            company_id,
            &actor,
            parse_party_id(&party_id)?,
            request.into_draft(false)?,
        )
        .await?;
    Ok(Json(PartyResponse::from(&party)))
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_party_id(raw: &str) -> Result<PartyId, ApiError> {
    Uuid::parse_str(raw)
        .map(PartyId::from_uuid)
        .map_err(|_| ApplicationError::PartyNotFound.into())
}

/// Asks the registry again about a party's CNPJ and returns the party with the new answer.
pub(crate) async fn verify_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, party_id)): Path<(Uuid, String)>,
) -> Result<Json<PartyResponse>, ApiError> {
    let party_id = parse_party_id(&party_id)?;
    let party = state
        .parties
        .verify_document(company_id, &actor, party_id)
        .await?;
    Ok(Json(PartyResponse::from(&party)))
}

pub(crate) async fn party_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, party_id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let party_id = parse_party_id(&party_id)?;
    let history = state.parties.history(company_id, party_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

pub(crate) async fn list_parties<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<PartyPageResponse>, ApiError> {
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
