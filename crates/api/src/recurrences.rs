use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, NameSort, RecurrenceQuery};
use conta_verde_domain::{
    CategoryId, CostCenterId, EntryKind, Frequency, PartyId, Recurrence, RecurrenceDraft,
    RecurrenceId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::clock::today;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, field_or};
use crate::list_params;
use crate::{Ports, SharedState};

/// The body of both registering and changing a recurrence: a change replaces every field. What it
/// has already generated is not touched.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecurrenceRequest {
    /// `receivable` or `payable`.
    kind: String,
    description: String,
    party_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    /// A decimal string in cents, above 0: what every occurrence is worth.
    amount: String,
    /// `weekly`, `monthly` or `yearly`.
    frequency: String,
    /// `YYYY-MM-DD`: the day of the first occurrence.
    starts_on: String,
    /// `YYYY-MM-DD`: the last day an occurrence may fall on, or none.
    ends_on: Option<String>,
    /// Optional when registering (active), required when replacing: a body without it would
    /// otherwise switch a paused recurrence back on.
    active: Option<bool>,
}

impl RecurrenceRequest {
    /// `default_active` is what a missing `active` means: `Some(true)` when registering, `None`
    /// when replacing, where it must be said.
    fn into_draft(self, default_active: Option<bool>) -> Result<RecurrenceDraft, ApiError> {
        let kind = EntryKind::parse(&self.kind)
            .ok_or(ApiError::InvalidField("kind must be receivable or payable"))?;
        let frequency = Frequency::parse(&self.frequency).ok_or(ApiError::InvalidField(
            "frequency must be weekly, monthly or yearly",
        ))?;
        let active = field_or(
            self.active,
            default_active,
            "active is required when replacing a recurrence",
        )?;
        Ok(RecurrenceDraft {
            kind,
            description: self.description,
            party_id: self.party_id.map(PartyId::from_uuid),
            category_id: CategoryId::from_uuid(self.category_id),
            cost_center_id: self.cost_center_id.map(CostCenterId::from_uuid),
            amount: self.amount,
            frequency,
            starts_on: self.starts_on,
            ends_on: self.ends_on,
            active,
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
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
    /// `description` (default) or `-description` for descending.
    sort: Option<String>,
}

impl ListQuery {
    fn recurrence_query(&self) -> Result<RecurrenceQuery, ApiError> {
        let mut query = RecurrenceQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(raw) = self.kind.as_deref() {
            query = query.of_kind(EntryKind::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid kind: expected receivable or payable",
            ))?);
        }
        if let Some(active) = list_params::active(self.active.as_deref())? {
            query = query.with_active(active);
        }
        let sort = match self.sort.as_deref() {
            None | Some("description") => NameSort::NameAsc,
            Some("-description") => NameSort::NameDesc,
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid sort: expected description or -description",
                ));
            }
        };
        Ok(query.sorted(sort))
    }
}

#[derive(Serialize)]
pub(crate) struct RecurrenceResponse {
    id: Uuid,
    company_id: Uuid,
    kind: &'static str,
    description: String,
    party_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    amount: String,
    frequency: &'static str,
    starts_on: String,
    ends_on: Option<String>,
    active: bool,
    /// The day through which its entries were made.
    generated_through: Option<String>,
}

impl From<&Recurrence> for RecurrenceResponse {
    fn from(recurrence: &Recurrence) -> Self {
        Self {
            id: recurrence.id().as_uuid(),
            company_id: recurrence.company_id().as_uuid(),
            kind: recurrence.kind().as_str(),
            description: recurrence.description().to_owned(),
            party_id: recurrence.party_id().map(|id| id.as_uuid()),
            category_id: recurrence.category_id().as_uuid(),
            cost_center_id: recurrence.cost_center_id().map(|id| id.as_uuid()),
            amount: recurrence.amount().to_string(),
            frequency: recurrence.frequency().as_str(),
            starts_on: recurrence.starts_on().to_string(),
            ends_on: recurrence.ends_on().map(|day| day.to_string()),
            active: recurrence.is_active(),
            generated_through: recurrence.generated_through().map(|day| day.to_string()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct RecurrencePageResponse {
    items: Vec<RecurrenceResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<RecurrenceId, ApiError> {
    Uuid::parse_str(raw)
        .map(RecurrenceId::from_uuid)
        .map_err(|_| ApplicationError::RecurrenceNotFound.into())
}

pub(crate) async fn list_recurrences<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<RecurrencePageResponse>, ApiError> {
    let filter = query.recurrence_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .recurrences
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(RecurrencePageResponse {
        items: page.items.iter().map(RecurrenceResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

/// Registers the recurrence and generates what falls due in the next two months.
pub(crate) async fn register_recurrence<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<RecurrenceRequest>,
) -> Result<(StatusCode, Json<RecurrenceResponse>), ApiError> {
    let recurrence = state
        .recurrences
        .register(company_id, &actor, request.into_draft(Some(true))?, today())
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(RecurrenceResponse::from(&recurrence)),
    ))
}

/// Replaces every editable field. Nothing is written, and nothing is recorded, when nothing changes.
pub(crate) async fn update_recurrence<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<RecurrenceRequest>,
) -> Result<Json<RecurrenceResponse>, ApiError> {
    let recurrence = state
        .recurrences
        .update(
            company_id,
            &actor,
            parse_id(&id)?,
            request.into_draft(None)?,
            today(),
        )
        .await?;
    Ok(Json(RecurrenceResponse::from(&recurrence)))
}

/// What was recorded about a recurrence, oldest first.
pub(crate) async fn recurrence_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .recurrences
        .history(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
