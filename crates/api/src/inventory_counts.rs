use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, CountView, InventoryCountQuery};
use conta_verde_domain::{
    CountLineDraft, CountStatus, InventoryCount, InventoryCountDraft, InventoryCountId, ProductId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_number_cursor, encode_number_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::list_params;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CountLineRequest {
    product_id: Uuid,
    /// What was found: 0 or more, up to three decimals.
    counted: String,
}

/// The body of both writing and changing a count: a change replaces every field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InventoryCountRequest {
    /// `YYYY-MM-DD`: the day of the count, on which the adjustments are booked.
    counted_on: String,
    notes: Option<String>,
    lines: Vec<CountLineRequest>,
}

impl From<InventoryCountRequest> for InventoryCountDraft {
    fn from(request: InventoryCountRequest) -> Self {
        Self {
            counted_on: request.counted_on,
            notes: request.notes,
            lines: request
                .lines
                .into_iter()
                .map(|line| CountLineDraft {
                    product_id: ProductId::from_uuid(line.product_id),
                    counted: line.counted,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the number or the notes.
    q: Option<String>,
    /// `open`, `applied` or `cancelled`.
    status: Option<String>,
    /// `-number` (default) or `number`.
    sort: Option<String>,
}

impl ListQuery {
    fn count_query(&self) -> Result<InventoryCountQuery, ApiError> {
        let mut query = InventoryCountQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(raw) = self.status.as_deref() {
            query = query.with_status(CountStatus::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid status: expected open, applied or cancelled",
            ))?);
        }
        Ok(query.sorted(list_params::number_sort(self.sort.as_deref())?))
    }
}

#[derive(Serialize)]
pub(crate) struct CountLineResponse {
    product_id: Uuid,
    counted: String,
    /// What the stock held: now while the count is open, and when it was applied afterwards. `null` in a list and
    /// for a cancelled count.
    expected: Option<String>,
    /// What is, or was, adjusted: counted less expected. `null` where `expected` is.
    difference: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct InventoryCountResponse {
    id: Uuid,
    company_id: Uuid,
    number: i64,
    counted_on: String,
    notes: Option<String>,
    status: &'static str,
    lines: Vec<CountLineResponse>,
}

impl InventoryCountResponse {
    /// A count without what it means for the stock, which only the answers about one count give.
    fn bare(count: &InventoryCount) -> Self {
        Self {
            id: count.id().as_uuid(),
            company_id: count.company_id().as_uuid(),
            number: count.number(),
            counted_on: count.counted_on().to_string(),
            notes: count.notes().map(str::to_owned),
            status: count.status().as_str(),
            lines: count
                .lines()
                .iter()
                .map(|line| CountLineResponse {
                    product_id: line.product_id().as_uuid(),
                    counted: line.counted().to_string(),
                    expected: None,
                    difference: None,
                })
                .collect(),
        }
    }

    fn viewed(view: &CountView) -> Self {
        let mut response = Self::bare(&view.count);
        for (line, seen) in response.lines.iter_mut().zip(&view.lines) {
            line.expected = seen.expected.map(|value| value.normalize().to_string());
            line.difference = seen.difference.map(|value| value.normalize().to_string());
        }
        response
    }
}

#[derive(Serialize)]
pub(crate) struct InventoryCountPageResponse {
    items: Vec<InventoryCountResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<InventoryCountId, ApiError> {
    Uuid::parse_str(raw)
        .map(InventoryCountId::from_uuid)
        .map_err(|_| ApplicationError::CountNotFound.into())
}

pub(crate) async fn list_counts<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<InventoryCountPageResponse>, ApiError> {
    let filter = query.count_query()?;
    let after = query
        .after
        .as_deref()
        .map(decode_number_cursor)
        .transpose()?;
    let page = state
        .counts
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(InventoryCountPageResponse {
        items: page
            .items
            .iter()
            .map(InventoryCountResponse::bare)
            .collect(),
        next_cursor: page.next.as_ref().map(encode_number_cursor),
    }))
}

pub(crate) async fn register_count<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<InventoryCountRequest>,
) -> Result<(StatusCode, Json<InventoryCountResponse>), ApiError> {
    let count = state
        .counts
        .register(company_id, &actor, request.into())
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(InventoryCountResponse::bare(&count)),
    ))
}

pub(crate) async fn get_count<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<InventoryCountResponse>, ApiError> {
    let view = state.counts.view(company_id, parse_id(&id)?).await?;
    Ok(Json(InventoryCountResponse::viewed(&view)))
}

/// Replaces every field of a count that is open; nothing is written when nothing changes.
pub(crate) async fn update_count<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<InventoryCountRequest>,
) -> Result<Json<InventoryCountResponse>, ApiError> {
    let id = parse_id(&id)?;
    state
        .counts
        .update(company_id, &actor, id, request.into())
        .await?;
    let view = state.counts.view(company_id, id).await?;
    Ok(Json(InventoryCountResponse::viewed(&view)))
}

/// Writes the differences as adjustments of the stock and closes the count. Asking again changes nothing.
pub(crate) async fn apply_count<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<InventoryCountResponse>, ApiError> {
    let id = parse_id(&id)?;
    state.counts.apply(company_id, &actor, id).await?;
    let view = state.counts.view(company_id, id).await?;
    Ok(Json(InventoryCountResponse::viewed(&view)))
}

/// Cancels a count that is open.
pub(crate) async fn cancel_count<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<InventoryCountResponse>, ApiError> {
    let id = parse_id(&id)?;
    state.counts.cancel(company_id, &actor, id).await?;
    let view = state.counts.view(company_id, id).await?;
    Ok(Json(InventoryCountResponse::viewed(&view)))
}

/// What was recorded about a count, oldest first.
pub(crate) async fn count_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.counts.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
