use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, CostCenterQuery};
use conta_verde_domain::{CostCenter, CostCenterDraft, CostCenterId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, field_or};
use crate::list_params;
use crate::{Ports, SharedState};

/// The body of both registering and changing a cost center: a change replaces every field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CostCenterRequest {
    name: String,
    /// Optional when registering (active), required when replacing: a body without it would
    /// otherwise switch a deactivated cost center back on.
    active: Option<bool>,
}

impl CostCenterRequest {
    /// `default_active` is what a missing `active` means: `Some(true)` when registering, `None`
    /// when replacing, where it must be said.
    fn into_draft(self, default_active: Option<bool>) -> Result<CostCenterDraft, ApiError> {
        let active = field_or(
            self.active,
            default_active,
            "active is required when replacing a cost center",
        )?;
        Ok(CostCenterDraft {
            name: self.name,
            active,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the name.
    q: Option<String>,
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
    /// `name` (default) or `-name` for descending.
    sort: Option<String>,
}

impl ListQuery {
    fn cost_center_query(&self) -> Result<CostCenterQuery, ApiError> {
        let mut query = CostCenterQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(active) = list_params::active(self.active.as_deref())? {
            query = query.with_active(active);
        }
        Ok(query.sorted(list_params::sort(self.sort.as_deref())?))
    }
}

#[derive(Serialize)]
pub(crate) struct CostCenterResponse {
    id: Uuid,
    company_id: Uuid,
    name: String,
    active: bool,
}

impl From<&CostCenter> for CostCenterResponse {
    fn from(cost_center: &CostCenter) -> Self {
        Self {
            id: cost_center.id().as_uuid(),
            company_id: cost_center.company_id().as_uuid(),
            name: cost_center.name().to_owned(),
            active: cost_center.is_active(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct CostCenterPageResponse {
    items: Vec<CostCenterResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<CostCenterId, ApiError> {
    Uuid::parse_str(raw)
        .map(CostCenterId::from_uuid)
        .map_err(|_| ApplicationError::CostCenterNotFound.into())
}

pub(crate) async fn list_cost_centers<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<CostCenterPageResponse>, ApiError> {
    let filter = query.cost_center_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .cost_centers
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(CostCenterPageResponse {
        items: page.items.iter().map(CostCenterResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn register_cost_center<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<CostCenterRequest>,
) -> Result<(StatusCode, Json<CostCenterResponse>), ApiError> {
    let cost_center = state
        .cost_centers
        .register(company_id, &actor, request.into_draft(Some(true))?)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CostCenterResponse::from(&cost_center)),
    ))
}

/// Replaces every editable field. Nothing is written, and nothing is recorded, when nothing changes.
pub(crate) async fn update_cost_center<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<CostCenterRequest>,
) -> Result<Json<CostCenterResponse>, ApiError> {
    let cost_center = state
        .cost_centers
        .update(
            company_id,
            &actor,
            parse_id(&id)?,
            request.into_draft(None)?,
        )
        .await?;
    Ok(Json(CostCenterResponse::from(&cost_center)))
}

/// What was recorded about a cost center, oldest first.
pub(crate) async fn cost_center_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .cost_centers
        .history(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
