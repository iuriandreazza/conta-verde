use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, ContractQuery, NameSort};
use conta_verde_domain::{
    BillingCycle, CategoryId, Contract, ContractDraft, ContractId, ContractStatus, CostCenterId,
    LineDraft, PartyId, PaymentTermId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::clock::customer_today;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, field_or};
use crate::list_params;
use crate::quotes::{LineRequest, LineResponse};
use crate::{Ports, SharedState};

/// The body of both registering and changing a contract: a change replaces every field. What was billed is
/// not touched.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContractRequest {
    name: String,
    customer_id: Uuid,
    seller_id: Option<Uuid>,
    payment_term_id: Option<Uuid>,
    /// The category of the receivable each cycle makes.
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    notes: Option<String>,
    lines: Vec<LineRequest>,
    /// `monthly`, `bimonthly`, `quarterly`, `semiannual` or `yearly`.
    cycle: String,
    /// `YYYY-MM-DD`: the day of the first cycle.
    starts_on: String,
    /// `YYYY-MM-DD`: the last day a cycle may fall on, or none.
    ends_on: Option<String>,
    /// Optional when registering (active), required when replacing: a body without it would otherwise switch a
    /// paused contract back on.
    active: Option<bool>,
}

impl ContractRequest {
    /// `default_active` is what a missing `active` means: `Some(true)` when registering, `None` when replacing.
    fn into_draft(self, default_active: Option<bool>) -> Result<ContractDraft, ApiError> {
        let cycle = BillingCycle::parse(&self.cycle).ok_or(ApiError::InvalidField(
            "cycle must be monthly, bimonthly, quarterly, semiannual or yearly",
        ))?;
        let active = field_or(
            self.active,
            default_active,
            "active is required when replacing a contract",
        )?;
        Ok(ContractDraft {
            name: self.name,
            customer_id: PartyId::from_uuid(self.customer_id),
            seller_id: self.seller_id.map(PartyId::from_uuid),
            payment_term_id: self.payment_term_id.map(PaymentTermId::from_uuid),
            category_id: CategoryId::from_uuid(self.category_id),
            cost_center_id: self.cost_center_id.map(CostCenterId::from_uuid),
            notes: self.notes,
            lines: self.lines.into_iter().map(LineDraft::from).collect(),
            cycle,
            starts_on: self.starts_on,
            ends_on: self.ends_on,
            active,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdjustRequest {
    /// A decimal between -99.99 and 1000 with at most two decimals, like `5.5`.
    percent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminateRequest {
    /// `YYYY-MM-DD`: the last day a cycle may fall on.
    on: String,
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the name.
    q: Option<String>,
    /// `active`, `paused` or `terminated`.
    status: Option<String>,
    customer: Option<Uuid>,
    /// `name` (default) or `-name` for descending.
    sort: Option<String>,
}

impl ListQuery {
    fn contract_query(&self) -> Result<ContractQuery, ApiError> {
        let mut query = ContractQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(raw) = self.status.as_deref() {
            query = query.with_status(ContractStatus::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid status: expected active, paused or terminated",
            ))?);
        }
        if let Some(customer) = self.customer {
            query = query.with_customer(PartyId::from_uuid(customer));
        }
        let sort = match self.sort.as_deref() {
            None | Some("name") => NameSort::NameAsc,
            Some("-name") => NameSort::NameDesc,
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid sort: expected name or -name",
                ));
            }
        };
        Ok(query.sorted(sort))
    }
}

#[derive(Serialize)]
pub(crate) struct ContractResponse {
    id: Uuid,
    company_id: Uuid,
    name: String,
    customer_id: Uuid,
    seller_id: Option<Uuid>,
    payment_term_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    notes: Option<String>,
    lines: Vec<LineResponse>,
    /// What one cycle bills: the sum of the lines.
    total: String,
    cycle: &'static str,
    starts_on: String,
    ends_on: Option<String>,
    status: &'static str,
    /// The day through which the cycles were billed.
    billed_through: Option<String>,
}

impl From<&Contract> for ContractResponse {
    fn from(contract: &Contract) -> Self {
        Self {
            id: contract.id().as_uuid(),
            company_id: contract.company_id().as_uuid(),
            name: contract.name().to_owned(),
            customer_id: contract.customer_id().as_uuid(),
            seller_id: contract.seller_id().map(|id| id.as_uuid()),
            payment_term_id: contract.payment_term_id().map(|id| id.as_uuid()),
            category_id: contract.category_id().as_uuid(),
            cost_center_id: contract.cost_center_id().map(|id| id.as_uuid()),
            notes: contract.notes().map(str::to_owned),
            lines: contract.lines().iter().map(LineResponse::from).collect(),
            total: contract.total().to_string(),
            cycle: contract.cycle().as_str(),
            starts_on: contract.starts_on().to_string(),
            ends_on: contract.ends_on().map(|day| day.to_string()),
            status: contract.status().as_str(),
            billed_through: contract.billed_through().map(|day| day.to_string()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ContractPageResponse {
    items: Vec<ContractResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON errors of the
// rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<ContractId, ApiError> {
    Uuid::parse_str(raw)
        .map(ContractId::from_uuid)
        .map_err(|_| ApplicationError::ContractNotFound.into())
}

pub(crate) async fn list_contracts<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<ContractPageResponse>, ApiError> {
    let filter = query.contract_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .contracts
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(ContractPageResponse {
        items: page.items.iter().map(ContractResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

/// Registers the contract and bills the cycles that have already fallen due.
pub(crate) async fn register_contract<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<ContractRequest>,
) -> Result<(StatusCode, Json<ContractResponse>), ApiError> {
    let contract = state
        .contracts
        .register(
            company_id,
            &actor,
            request.into_draft(Some(true))?,
            customer_today(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(ContractResponse::from(&contract))))
}

pub(crate) async fn get_contract<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<ContractResponse>, ApiError> {
    let contract = state.contracts.get(company_id, parse_id(&id)?).await?;
    Ok(Json(ContractResponse::from(&contract)))
}

/// Replaces every editable field. Nothing is written, and nothing is recorded, when nothing changes.
pub(crate) async fn update_contract<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<ContractRequest>,
) -> Result<Json<ContractResponse>, ApiError> {
    let contract = state
        .contracts
        .update(
            company_id,
            &actor,
            parse_id(&id)?,
            request.into_draft(None)?,
            customer_today(),
        )
        .await?;
    Ok(Json(ContractResponse::from(&contract)))
}

/// Raises (or lowers) the price of every line by a percentage, from the next cycle that is billed.
pub(crate) async fn adjust_contract<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<AdjustRequest>,
) -> Result<Json<ContractResponse>, ApiError> {
    let contract = state
        .contracts
        .adjust(company_id, &actor, parse_id(&id)?, &request.percent)
        .await?;
    Ok(Json(ContractResponse::from(&contract)))
}

/// Ends the contract for good: nothing is billed after the day.
pub(crate) async fn terminate_contract<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<TerminateRequest>,
) -> Result<Json<ContractResponse>, ApiError> {
    let contract = state
        .contracts
        .terminate(
            company_id,
            &actor,
            parse_id(&id)?,
            &request.on,
            customer_today(),
        )
        .await?;
    Ok(Json(ContractResponse::from(&contract)))
}

/// What was recorded about a contract, oldest first.
pub(crate) async fn contract_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.contracts.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
