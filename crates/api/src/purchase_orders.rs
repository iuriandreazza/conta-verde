use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, PurchaseOrderQuery, ReceiptRecord};
use conta_verde_domain::{
    CategoryId, CostCenterId, LineDraft, PartyId, PaymentTermId, PurchaseOrder, PurchaseOrderDraft,
    PurchaseOrderId, PurchaseReceiptDraft, PurchaseReceiptId, PurchaseStatus, ReceiptItemDraft,
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

/// The body of both writing and changing a purchase order: a change replaces every field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PurchaseOrderRequest {
    supplier_id: Uuid,
    /// `YYYY-MM-DD`: the day of the order.
    ordered_on: String,
    /// `YYYY-MM-DD`: when the goods are expected, not before the day of the order.
    expected_on: Option<String>,
    payment_term_id: Option<Uuid>,
    /// The category of the payables the receipts make.
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    notes: Option<String>,
    /// A line's `unit_price` is the unit cost.
    lines: Vec<LineRequest>,
}

impl From<PurchaseOrderRequest> for PurchaseOrderDraft {
    fn from(request: PurchaseOrderRequest) -> Self {
        Self {
            supplier_id: PartyId::from_uuid(request.supplier_id),
            ordered_on: request.ordered_on,
            expected_on: request.expected_on,
            payment_term_id: request.payment_term_id.map(PaymentTermId::from_uuid),
            category_id: CategoryId::from_uuid(request.category_id),
            cost_center_id: request.cost_center_id.map(CostCenterId::from_uuid),
            notes: request.notes,
            lines: request.lines.into_iter().map(LineDraft::from).collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReceiptItemRequest {
    /// The line's place in the order, from 1.
    line: u16,
    quantity: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReceiptRequest {
    received_on: String,
    notes: Option<String>,
    items: Vec<ReceiptItemRequest>,
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the number, the notes or a line's description.
    q: Option<String>,
    /// `draft`, `approved`, `partially_received`, `received` or `cancelled`.
    status: Option<String>,
    supplier: Option<Uuid>,
    /// `-number` (default) or `number`.
    sort: Option<String>,
}

impl ListQuery {
    fn order_query(&self) -> Result<PurchaseOrderQuery, ApiError> {
        let mut query = PurchaseOrderQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(raw) = self.status.as_deref() {
            query = query.with_status(PurchaseStatus::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid status: expected draft, approved, partially_received, received or cancelled",
            ))?);
        }
        if let Some(supplier) = self.supplier {
            query = query.with_supplier(PartyId::from_uuid(supplier));
        }
        Ok(query.sorted(list_params::number_sort(self.sort.as_deref())?))
    }
}

#[derive(Serialize)]
pub(crate) struct PurchaseOrderResponse {
    id: Uuid,
    company_id: Uuid,
    number: i64,
    supplier_id: Uuid,
    ordered_on: String,
    expected_on: Option<String>,
    payment_term_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    notes: Option<String>,
    status: &'static str,
    /// The access key of the NF-e the order was entered from ([ADR 0057]).
    invoice_key: Option<String>,
    lines: Vec<LineResponse>,
    /// The sum of the lines.
    total: String,
}

impl From<&PurchaseOrder> for PurchaseOrderResponse {
    fn from(order: &PurchaseOrder) -> Self {
        Self {
            id: order.id().as_uuid(),
            company_id: order.company_id().as_uuid(),
            number: order.number(),
            supplier_id: order.supplier_id().as_uuid(),
            ordered_on: order.ordered_on().to_string(),
            expected_on: order.expected_on().map(|day| day.to_string()),
            payment_term_id: order.payment_term_id().map(|id| id.as_uuid()),
            category_id: order.category_id().as_uuid(),
            cost_center_id: order.cost_center_id().map(|id| id.as_uuid()),
            notes: order.notes().map(str::to_owned),
            status: order.status().as_str(),
            invoice_key: order.invoice_key().map(|key| key.as_str().to_owned()),
            lines: order.lines().iter().map(LineResponse::from).collect(),
            total: order.total().to_string(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PurchaseOrderPageResponse {
    items: Vec<PurchaseOrderResponse>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReceiptItemResponse {
    line: u16,
    quantity: String,
    /// What the quantity cost, a decimal string.
    value: String,
}

#[derive(Serialize)]
pub(crate) struct ReceiptResponse {
    id: Uuid,
    purchase_id: Uuid,
    received_on: String,
    notes: Option<String>,
    items: Vec<ReceiptItemResponse>,
    /// The sum of the items, what the payable pays.
    total: String,
    /// The payable of the receipt; `null` for a receipt worth nothing, or when its write failed after the
    /// receipt was stored (asking to finish it makes it).
    payable_id: Option<Uuid>,
}

impl From<&ReceiptRecord> for ReceiptResponse {
    fn from(record: &ReceiptRecord) -> Self {
        let made = &record.receipt;
        Self {
            id: made.id().as_uuid(),
            purchase_id: made.purchase_id().as_uuid(),
            received_on: made.received_on().to_string(),
            notes: made.notes().map(str::to_owned),
            items: made
                .items()
                .iter()
                .map(|item| ReceiptItemResponse {
                    line: item.line,
                    quantity: item.quantity.to_string(),
                    value: item.value.to_string(),
                })
                .collect(),
            total: made.total().to_string(),
            payable_id: record.payable.as_ref().map(|entry| entry.id().as_uuid()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ReceiptListResponse {
    items: Vec<ReceiptResponse>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<PurchaseOrderId, ApiError> {
    Uuid::parse_str(raw)
        .map(PurchaseOrderId::from_uuid)
        .map_err(|_| ApplicationError::PurchaseNotFound.into())
}

fn parse_receipt_id(raw: &str) -> Result<PurchaseReceiptId, ApiError> {
    Uuid::parse_str(raw)
        .map(PurchaseReceiptId::from_uuid)
        .map_err(|_| ApplicationError::PurchaseReceiptNotFound.into())
}

pub(crate) async fn list_purchase_orders<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<PurchaseOrderPageResponse>, ApiError> {
    let filter = query.order_query()?;
    let after = query
        .after
        .as_deref()
        .map(decode_number_cursor)
        .transpose()?;
    let page = state
        .purchases
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(PurchaseOrderPageResponse {
        items: page.items.iter().map(PurchaseOrderResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_number_cursor),
    }))
}

pub(crate) async fn register_purchase_order<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<PurchaseOrderRequest>,
) -> Result<(StatusCode, Json<PurchaseOrderResponse>), ApiError> {
    let order = state
        .purchases
        .register(company_id, &actor, request.into())
        .await?;
    Ok((StatusCode::CREATED, Json((&order).into())))
}

pub(crate) async fn get_purchase_order<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<PurchaseOrderResponse>, ApiError> {
    let order = state.purchases.get(company_id, parse_id(&id)?).await?;
    Ok(Json((&order).into()))
}

/// Replaces every field of a draft; nothing is written when nothing changes.
pub(crate) async fn update_purchase_order<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<PurchaseOrderRequest>,
) -> Result<Json<PurchaseOrderResponse>, ApiError> {
    let order = state
        .purchases
        .update(company_id, &actor, parse_id(&id)?, request.into())
        .await?;
    Ok(Json((&order).into()))
}

/// What was recorded about an order, oldest first.
pub(crate) async fn purchase_order_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.purchases.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

/// Agrees a draft with the supplier: its items are fixed from then on.
pub(crate) async fn approve_purchase_order<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<PurchaseOrderResponse>, ApiError> {
    let order = state
        .purchases
        .approve(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json((&order).into()))
}

/// Cancels an order before anything arrived.
pub(crate) async fn cancel_purchase_order<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<PurchaseOrderResponse>, ApiError> {
    let order = state
        .purchases
        .cancel(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json((&order).into()))
}

/// Closes a partly received order: what has not arrived is given up.
pub(crate) async fn close_purchase_order<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<PurchaseOrderResponse>, ApiError> {
    let order = state
        .purchases
        .close(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json((&order).into()))
}

pub(crate) async fn list_receipts<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<ReceiptListResponse>, ApiError> {
    let receipts = state.purchases.receipts(company_id, parse_id(&id)?).await?;
    Ok(Json(ReceiptListResponse {
        items: receipts.iter().map(ReceiptResponse::from).collect(),
    }))
}

pub(crate) async fn register_receipt<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<ReceiptRequest>,
) -> Result<(StatusCode, Json<ReceiptResponse>), ApiError> {
    let draft = PurchaseReceiptDraft {
        received_on: request.received_on,
        notes: request.notes,
        items: request
            .items
            .into_iter()
            .map(|item| ReceiptItemDraft {
                line: item.line,
                quantity: item.quantity,
            })
            .collect(),
    };
    let record = state
        .purchases
        .receive(company_id, &actor, parse_id(&id)?, draft)
        .await?;
    Ok((StatusCode::CREATED, Json(ReceiptResponse::from(&record))))
}

/// Makes what a receipt still lacks; safe to ask again.
pub(crate) async fn finish_receipt<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id, receipt_id)): Path<(Uuid, String, String)>,
) -> Result<Json<ReceiptResponse>, ApiError> {
    let record = state
        .purchases
        .finish_receipt(
            company_id,
            &actor,
            parse_id(&id)?,
            parse_receipt_id(&receipt_id)?,
        )
        .await?;
    Ok(Json(ReceiptResponse::from(&record)))
}
