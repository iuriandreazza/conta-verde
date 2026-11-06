use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, EntryCursor, EntrySort, FinancialEntryQuery};
use conta_verde_domain::{
    CalendarDate, CategoryId, CostCenterId, EntryId, EntryInstallmentDraft, EntryKind,
    FinancialEntry, FinancialEntryDraft, PartyId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::list_params;
use crate::{Ports, SharedState};

/// One installment: the day it falls due and what is due, a decimal string in cents.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallmentRequest {
    due_date: String,
    amount: String,
}

/// The body of both registering and changing an entry: a change replaces every field and every
/// installment, and a reference left out is a reference removed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EntryRequest {
    /// `receivable` or `payable`.
    kind: String,
    description: String,
    party_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    /// `YYYY-MM-DD`: the day the entry belongs to for the managerial income statement.
    competence_date: String,
    /// In order: the due dates do not decrease.
    installments: Vec<InstallmentRequest>,
}

impl EntryRequest {
    fn into_draft(self) -> Result<FinancialEntryDraft, ApiError> {
        let kind = EntryKind::parse(&self.kind)
            .ok_or(ApiError::InvalidField("kind must be receivable or payable"))?;
        Ok(FinancialEntryDraft {
            kind,
            description: self.description,
            party_id: self.party_id.map(PartyId::from_uuid),
            category_id: CategoryId::from_uuid(self.category_id),
            cost_center_id: self.cost_center_id.map(CostCenterId::from_uuid),
            competence_date: self.competence_date,
            installments: self
                .installments
                .into_iter()
                .map(|installment| EntryInstallmentDraft {
                    due_date: installment.due_date,
                    amount: installment.amount,
                })
                .collect(),
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the description.
    q: Option<String>,
    /// `receivable` or `payable`.
    kind: Option<String>,
    /// The id of a category.
    category: Option<String>,
    /// `competence_date` for oldest first, or `-competence_date` (the default) for newest first.
    sort: Option<String>,
}

impl ListQuery {
    fn entry_query(&self) -> Result<FinancialEntryQuery, ApiError> {
        let mut query = FinancialEntryQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(kind) = self.kind.as_deref() {
            query = query.with_kind(EntryKind::parse(kind).ok_or(ApiError::InvalidQuery(
                "invalid kind: expected receivable or payable",
            ))?);
        }
        if let Some(category) = self.category.as_deref() {
            let id = Uuid::parse_str(category)
                .map_err(|_| ApiError::InvalidQuery("invalid category: expected an id"))?;
            query = query.with_category(CategoryId::from_uuid(id));
        }
        let sort = match self.sort.as_deref() {
            None | Some("-competence_date") => EntrySort::CompetenceDesc,
            Some("competence_date") => EntrySort::CompetenceAsc,
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid sort: expected competence_date or -competence_date",
                ));
            }
        };
        Ok(query.sorted(sort))
    }
}

#[derive(Serialize)]
pub(crate) struct InstallmentResponse {
    number: u16,
    due_date: String,
    /// A decimal string without trailing zeros, like `"150.5"` or `"2500"`.
    amount: String,
}

#[derive(Serialize)]
pub(crate) struct EntryResponse {
    id: Uuid,
    company_id: Uuid,
    kind: &'static str,
    description: String,
    party_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    competence_date: String,
    installments: Vec<InstallmentResponse>,
    /// The sum of the installments, a decimal string: it is not stored, it is added up.
    total: String,
}

impl From<&FinancialEntry> for EntryResponse {
    fn from(entry: &FinancialEntry) -> Self {
        Self {
            id: entry.id().as_uuid(),
            company_id: entry.company_id().as_uuid(),
            kind: entry.kind().as_str(),
            description: entry.description().to_owned(),
            party_id: entry.party_id().map(|id| id.as_uuid()),
            category_id: entry.category_id().as_uuid(),
            cost_center_id: entry.cost_center_id().map(|id| id.as_uuid()),
            competence_date: entry.competence_date().to_string(),
            installments: entry
                .installments()
                .iter()
                .map(|installment| InstallmentResponse {
                    number: installment.number(),
                    due_date: installment.due_date().to_string(),
                    amount: installment.amount().to_string(),
                })
                .collect(),
            total: entry.total().to_string(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct EntryPageResponse {
    items: Vec<EntryResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<EntryId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntryId::from_uuid)
        .map_err(|_| ApplicationError::EntryNotFound.into())
}

// Clients treat the cursor as opaque. The id comes first because it has a fixed shape.
fn encode_cursor(cursor: &EntryCursor) -> String {
    format!("{}|{}", cursor.id, cursor.competence_date)
}

fn decode_cursor(raw: &str) -> Result<EntryCursor, ApiError> {
    let (id, date) = raw.split_once('|').ok_or(ApiError::InvalidCursor)?;
    Ok(EntryCursor {
        id: Uuid::parse_str(id).map_err(|_| ApiError::InvalidCursor)?,
        competence_date: CalendarDate::parse(date).ok_or(ApiError::InvalidCursor)?,
    })
}

pub(crate) async fn list_entries<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<EntryPageResponse>, ApiError> {
    let filter = query.entry_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .entries
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(EntryPageResponse {
        items: page.items.iter().map(EntryResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn register_entry<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<EntryRequest>,
) -> Result<(StatusCode, Json<EntryResponse>), ApiError> {
    let entry = state
        .entries
        .register(company_id, &actor, request.into_draft()?)
        .await?;
    Ok((StatusCode::CREATED, Json(EntryResponse::from(&entry))))
}

pub(crate) async fn get_entry<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<EntryResponse>, ApiError> {
    let entry = state.entries.get(company_id, parse_id(&id)?).await?;
    Ok(Json(EntryResponse::from(&entry)))
}

/// Replaces every field and every installment. Nothing is written, and nothing is recorded, when
/// nothing changes.
pub(crate) async fn update_entry<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<EntryRequest>,
) -> Result<Json<EntryResponse>, ApiError> {
    let entry = state
        .entries
        .update(company_id, &actor, parse_id(&id)?, request.into_draft()?)
        .await?;
    Ok(Json(EntryResponse::from(&entry)))
}

/// What was recorded about an entry, oldest first.
pub(crate) async fn entry_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.entries.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
