use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, Conversion, Refund, ReturnRecord, SaleQuery};
use conta_verde_domain::{
    CategoryId, ContractId, CostCenterId, LineDraft, PartyId, PaymentTermId, QuoteId,
    ReturnItemDraft, Sale, SaleDraft, SaleId, SaleReturnDraft, SaleStatus,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_number_cursor, encode_number_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::list_params;
use crate::quotes::{LineRequest, LineResponse};
use crate::{Ports, SharedState};

/// The body of both writing and changing a sale: a change replaces every field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SaleRequest {
    customer_id: Uuid,
    seller_id: Option<Uuid>,
    /// `YYYY-MM-DD`: the day of the sale, from which the payment term counts.
    sold_on: String,
    payment_term_id: Option<Uuid>,
    /// The category of the receivable the sale makes.
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    notes: Option<String>,
    lines: Vec<LineRequest>,
}

impl From<SaleRequest> for SaleDraft {
    fn from(request: SaleRequest) -> Self {
        Self {
            customer_id: PartyId::from_uuid(request.customer_id),
            seller_id: request.seller_id.map(PartyId::from_uuid),
            sold_on: request.sold_on,
            payment_term_id: request.payment_term_id.map(PaymentTermId::from_uuid),
            category_id: CategoryId::from_uuid(request.category_id),
            cost_center_id: request.cost_center_id.map(CostCenterId::from_uuid),
            notes: request.notes,
            lines: request.lines.into_iter().map(LineDraft::from).collect(),
        }
    }
}

/// What a quote becomes a sale with.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConversionRequest {
    sold_on: String,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    seller_id: Option<Uuid>,
    /// The quote's own term unless another is said.
    payment_term_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReturnItemRequest {
    /// The line's place in the sale, from 1.
    line: u16,
    quantity: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReturnRequest {
    returned_on: String,
    reason: Option<String>,
    items: Vec<ReturnItemRequest>,
    /// The category of the payable that gives the money back.
    refund_category_id: Uuid,
    /// `YYYY-MM-DD`: when the refund falls due.
    refund_due_on: String,
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the number, the notes or a line's description.
    q: Option<String>,
    /// `open`, `completed` or `cancelled`.
    status: Option<String>,
    customer: Option<Uuid>,
    seller: Option<Uuid>,
    /// The contract whose cycles they bill.
    contract: Option<Uuid>,
    /// `-number` (default) or `number`.
    sort: Option<String>,
}

impl ListQuery {
    fn sale_query(&self) -> Result<SaleQuery, ApiError> {
        let mut query = SaleQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(raw) = self.status.as_deref() {
            query = query.with_status(SaleStatus::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid status: expected open, completed or cancelled",
            ))?);
        }
        if let Some(customer) = self.customer {
            query = query.with_customer(PartyId::from_uuid(customer));
        }
        if let Some(seller) = self.seller {
            query = query.with_seller(PartyId::from_uuid(seller));
        }
        if let Some(contract) = self.contract {
            query = query.of_contract(ContractId::from_uuid(contract));
        }
        Ok(query.sorted(list_params::number_sort(self.sort.as_deref())?))
    }
}

#[derive(Serialize)]
pub(crate) struct SaleResponse {
    id: Uuid,
    company_id: Uuid,
    number: i64,
    customer_id: Uuid,
    seller_id: Option<Uuid>,
    sold_on: String,
    payment_term_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    notes: Option<String>,
    status: &'static str,
    /// The quote it was made from.
    quote_id: Option<Uuid>,
    /// The contract it bills a cycle of, and the day of the cycle.
    contract_id: Option<Uuid>,
    cycle_on: Option<String>,
    lines: Vec<LineResponse>,
    /// The sum of the lines.
    total: String,
    /// The receivable of a completed sale. Only the answers about one sale say it; the lists leave it out.
    receivable_id: Option<Uuid>,
}

impl SaleResponse {
    fn new(sale: &Sale, receivable_id: Option<Uuid>) -> Self {
        Self {
            id: sale.id().as_uuid(),
            company_id: sale.company_id().as_uuid(),
            number: sale.number(),
            customer_id: sale.customer_id().as_uuid(),
            seller_id: sale.seller_id().map(|id| id.as_uuid()),
            sold_on: sale.sold_on().to_string(),
            payment_term_id: sale.payment_term_id().map(|id| id.as_uuid()),
            category_id: sale.category_id().as_uuid(),
            cost_center_id: sale.cost_center_id().map(|id| id.as_uuid()),
            notes: sale.notes().map(str::to_owned),
            status: sale.status().as_str(),
            quote_id: sale.quote_id().map(|id| id.as_uuid()),
            contract_id: sale.cycle().map(|cycle| cycle.contract_id.as_uuid()),
            cycle_on: sale.cycle().map(|cycle| cycle.cycle_on.to_string()),
            lines: sale.lines().iter().map(LineResponse::from).collect(),
            total: sale.total().to_string(),
            receivable_id,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SalePageResponse {
    items: Vec<SaleResponse>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReturnItemResponse {
    line: u16,
    quantity: String,
}

#[derive(Serialize)]
pub(crate) struct ReturnResponse {
    id: Uuid,
    sale_id: Uuid,
    returned_on: String,
    reason: Option<String>,
    items: Vec<ReturnItemResponse>,
    /// What was refunded, a decimal string.
    refund: String,
    /// The payable that gives the money back; `null` only if its write failed after the return was stored.
    refund_entry_id: Option<Uuid>,
}

impl From<&ReturnRecord> for ReturnResponse {
    fn from(record: &ReturnRecord) -> Self {
        let made = &record.sale_return;
        Self {
            id: made.id().as_uuid(),
            sale_id: made.sale_id().as_uuid(),
            returned_on: made.returned_on().to_string(),
            reason: made.reason().map(str::to_owned),
            items: made
                .items()
                .iter()
                .map(|(line, quantity)| ReturnItemResponse {
                    line: *line,
                    quantity: quantity.to_string(),
                })
                .collect(),
            refund: made.refund().to_string(),
            refund_entry_id: record.refund.as_ref().map(|entry| entry.id().as_uuid()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ReturnListResponse {
    items: Vec<ReturnResponse>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<SaleId, ApiError> {
    Uuid::parse_str(raw)
        .map(SaleId::from_uuid)
        .map_err(|_| ApplicationError::SaleNotFound.into())
}

async fn detail<X: Ports>(state: &SharedState<X>, sale: &Sale) -> Result<SaleResponse, ApiError> {
    let receivable = state
        .sales
        .receivable(sale.company_id(), sale.id())
        .await?
        .map(|entry| entry.id().as_uuid());
    Ok(SaleResponse::new(sale, receivable))
}

pub(crate) async fn list_sales<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<SalePageResponse>, ApiError> {
    let filter = query.sale_query()?;
    let after = query
        .after
        .as_deref()
        .map(decode_number_cursor)
        .transpose()?;
    let page = state
        .sales
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(SalePageResponse {
        items: page
            .items
            .iter()
            .map(|sale| SaleResponse::new(sale, None))
            .collect(),
        next_cursor: page.next.as_ref().map(encode_number_cursor),
    }))
}

pub(crate) async fn register_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<SaleRequest>,
) -> Result<(StatusCode, Json<SaleResponse>), ApiError> {
    let sale = state
        .sales
        .register(company_id, &actor, request.into())
        .await?;
    Ok((StatusCode::CREATED, Json(SaleResponse::new(&sale, None))))
}

pub(crate) async fn get_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<SaleResponse>, ApiError> {
    let sale = state.sales.get(company_id, parse_id(&id)?).await?;
    Ok(Json(detail(&state, &sale).await?))
}

/// Replaces every field of an order that is still open; nothing is written when nothing changes.
pub(crate) async fn update_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<SaleRequest>,
) -> Result<Json<SaleResponse>, ApiError> {
    let sale = state
        .sales
        .update(company_id, &actor, parse_id(&id)?, request.into())
        .await?;
    Ok(Json(SaleResponse::new(&sale, None)))
}

/// What was recorded about a sale, oldest first.
pub(crate) async fn sale_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.sales.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

/// Bills an open order: its receivable is made from its payment term. Asking again for a completed sale
/// finishes a receivable that a failure left unmade, and changes nothing otherwise.
pub(crate) async fn complete_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<SaleResponse>, ApiError> {
    let sale = state
        .sales
        .complete(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json(detail(&state, &sale).await?))
}

/// Cancels a sale and, if it was completed, its receivable.
pub(crate) async fn cancel_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<SaleResponse>, ApiError> {
    let sale = state
        .sales
        .cancel(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json(detail(&state, &sale).await?))
}

pub(crate) async fn list_returns<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<ReturnListResponse>, ApiError> {
    let returns = state.sales.returns(company_id, parse_id(&id)?).await?;
    Ok(Json(ReturnListResponse {
        items: returns.iter().map(ReturnResponse::from).collect(),
    }))
}

pub(crate) async fn register_return<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<ReturnRequest>,
) -> Result<(StatusCode, Json<ReturnResponse>), ApiError> {
    let draft = SaleReturnDraft {
        returned_on: request.returned_on,
        reason: request.reason,
        items: request
            .items
            .into_iter()
            .map(|item| ReturnItemDraft {
                line: item.line,
                quantity: item.quantity,
            })
            .collect(),
    };
    let refund = Refund {
        category_id: CategoryId::from_uuid(request.refund_category_id),
        due_on: request.refund_due_on,
    };
    let record = state
        .sales
        .register_return(company_id, &actor, parse_id(&id)?, draft, refund)
        .await?;
    Ok((StatusCode::CREATED, Json(ReturnResponse::from(&record))))
}

/// Turns a quote into an open order. Asking again for a quote that already became one gives that sale.
pub(crate) async fn convert_quote<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<ConversionRequest>,
) -> Result<(StatusCode, Json<SaleResponse>), ApiError> {
    let quote_id = Uuid::parse_str(&id)
        .map(QuoteId::from_uuid)
        .map_err(|_| ApplicationError::QuoteNotFound)?;
    let sale = state
        .sales
        .convert_quote(
            company_id,
            &actor,
            quote_id,
            Conversion {
                sold_on: request.sold_on,
                category_id: CategoryId::from_uuid(request.category_id),
                cost_center_id: request.cost_center_id.map(CostCenterId::from_uuid),
                seller_id: request.seller_id.map(PartyId::from_uuid),
                payment_term_id: request.payment_term_id.map(PaymentTermId::from_uuid),
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(SaleResponse::new(&sale, None))))
}
