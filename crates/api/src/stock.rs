use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, ProductQuery, StockMovementQuery, StockPosition};
use conta_verde_domain::{
    CalendarDate, ProductId, StockMovement, StockMovementDraft, StockMovementId, StockMovementKind,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_cursor, decode_date_cursor, encode_cursor, encode_date_cursor};
use crate::error::ApiError;
use crate::json::{ApiJson, ApiQuery};
use crate::list_params;
use crate::{Ports, SharedState};

/// A movement written by hand.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StockMovementRequest {
    product_id: Uuid,
    /// `in`, `out` or `adjustment`.
    kind: String,
    /// Above 0 for an `in` and an `out`; signed for an `adjustment`.
    quantity: String,
    /// What an `in` cost in all, in cents; or its `unit_cost`, not both.
    value: Option<String>,
    unit_cost: Option<String>,
    /// `YYYY-MM-DD`.
    occurred_on: String,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReverseRequest {
    /// `YYYY-MM-DD`: the day the undoing is booked.
    occurred_on: String,
    note: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct MovementsQuery {
    limit: Option<usize>,
    after: Option<String>,
    product: Option<Uuid>,
    /// `in`, `out` or `adjustment`.
    kind: Option<String>,
    /// `YYYY-MM-DD`, the day included.
    from: Option<String>,
    to: Option<String>,
}

impl MovementsQuery {
    fn movement_query(&self) -> Result<StockMovementQuery, ApiError> {
        let mut query = StockMovementQuery::default();
        if let Some(product) = self.product {
            query = query.of_product(ProductId::from_uuid(product));
        }
        if let Some(raw) = self.kind.as_deref() {
            query = query.of_kind(StockMovementKind::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid kind: expected in, out or adjustment",
            ))?);
        }
        if let Some(raw) = self.from.as_deref() {
            query = query.from(
                CalendarDate::parse(raw)
                    .ok_or(ApiError::InvalidQuery("invalid from: expected YYYY-MM-DD"))?,
            );
        }
        if let Some(raw) = self.to.as_deref() {
            query = query.to(CalendarDate::parse(raw)
                .ok_or(ApiError::InvalidQuery("invalid to: expected YYYY-MM-DD"))?);
        }
        Ok(query)
    }
}

#[derive(Deserialize)]
pub(crate) struct PositionsQuery {
    limit: Option<usize>,
    after: Option<String>,
    q: Option<String>,
    active: Option<String>,
    /// `YYYY-MM-DD`: what each product had through this day; now when it is left out.
    on: Option<String>,
    sort: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct OriginResponse {
    kind: &'static str,
    id: Uuid,
    line: u16,
}

#[derive(Serialize)]
pub(crate) struct StockMovementResponse {
    id: Uuid,
    product_id: Uuid,
    kind: &'static str,
    /// Signed: what an `in` adds is above 0, what an `out` takes is below.
    quantity: String,
    /// Signed like the quantity, in cents.
    value: String,
    occurred_on: String,
    origin: Option<OriginResponse>,
    /// The movement this one undoes.
    reverses: Option<Uuid>,
    note: Option<String>,
}

impl From<&StockMovement> for StockMovementResponse {
    fn from(movement: &StockMovement) -> Self {
        Self {
            id: movement.id().as_uuid(),
            product_id: movement.product_id().as_uuid(),
            kind: movement.kind().as_str(),
            quantity: movement.quantity().to_string(),
            value: movement.value().to_string(),
            occurred_on: movement.occurred_on().to_string(),
            origin: movement.origin().map(|origin| OriginResponse {
                kind: origin.kind.as_str(),
                id: origin.id,
                line: origin.line,
            }),
            reverses: movement.reverses().map(|id| id.as_uuid()),
            note: movement.note().map(str::to_owned),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct StockMovementPageResponse {
    items: Vec<StockMovementResponse>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct PositionResponse {
    product_id: Uuid,
    name: String,
    sku: Option<String>,
    unit: String,
    active: bool,
    minimum_stock: Option<String>,
    quantity: String,
    /// What the quantity is worth, in cents.
    value: String,
    /// The value over the quantity with six decimals; `null` without something on hand.
    average_cost: Option<String>,
}

impl From<&StockPosition> for PositionResponse {
    fn from(position: &StockPosition) -> Self {
        let product = &position.product;
        Self {
            product_id: product.id().as_uuid(),
            name: product.name().to_owned(),
            sku: product.sku().map(str::to_owned),
            unit: product.unit().to_owned(),
            active: product.is_active(),
            minimum_stock: product.minimum_stock().map(|amount| amount.to_string()),
            quantity: position.balance.quantity().to_string(),
            value: position.balance.value().to_string(),
            average_cost: position.balance.average_cost().map(|cost| cost.to_string()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PositionPageResponse {
    items: Vec<PositionResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<StockMovementId, ApiError> {
    Uuid::parse_str(raw)
        .map(StockMovementId::from_uuid)
        .map_err(|_| ApplicationError::StockMovementNotFound.into())
}

pub(crate) async fn list_movements<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<MovementsQuery>,
) -> Result<Json<StockMovementPageResponse>, ApiError> {
    let filter = query.movement_query()?;
    let after = query.after.as_deref().map(decode_date_cursor).transpose()?;
    let page = state
        .stock
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(StockMovementPageResponse {
        items: page.items.iter().map(StockMovementResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_date_cursor),
    }))
}

pub(crate) async fn record_movement<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<StockMovementRequest>,
) -> Result<(StatusCode, Json<StockMovementResponse>), ApiError> {
    let kind = StockMovementKind::parse(&request.kind).ok_or(ApiError::InvalidField(
        "invalid kind: expected in, out or adjustment",
    ))?;
    let movement = state
        .stock
        .record(
            company_id,
            &actor,
            StockMovementDraft {
                product_id: ProductId::from_uuid(request.product_id),
                kind,
                quantity: request.quantity,
                value: request.value,
                unit_cost: request.unit_cost,
                occurred_on: request.occurred_on,
                note: request.note,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json((&movement).into())))
}

pub(crate) async fn get_movement<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<StockMovementResponse>, ApiError> {
    let movement = state.stock.get(company_id, parse_id(&id)?).await?;
    Ok(Json((&movement).into()))
}

/// Undoes a movement written by hand, once: another movement is written, with the opposite quantity and value.
pub(crate) async fn reverse_movement<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<ReverseRequest>,
) -> Result<(StatusCode, Json<StockMovementResponse>), ApiError> {
    let movement = state
        .stock
        .reverse(
            company_id,
            &actor,
            parse_id(&id)?,
            &request.occurred_on,
            request.note,
        )
        .await?;
    Ok((StatusCode::CREATED, Json((&movement).into())))
}

pub(crate) async fn list_positions<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<PositionsQuery>,
) -> Result<Json<PositionPageResponse>, ApiError> {
    let mut filter = ProductQuery::default().sorted(list_params::sort(query.sort.as_deref())?);
    if let Some(text) = list_params::search(query.q.as_deref())? {
        filter = filter.searching(text);
    }
    if let Some(active) = list_params::active(query.active.as_deref())? {
        filter = filter.with_active(active);
    }
    let on = query
        .on
        .as_deref()
        .map(|raw| {
            CalendarDate::parse(raw)
                .ok_or(ApiError::InvalidQuery("invalid on: expected YYYY-MM-DD"))
        })
        .transpose()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .stock
        .positions(company_id, filter, after.as_ref(), query.limit, on)
        .await?;
    Ok(Json(PositionPageResponse {
        items: page.items.iter().map(PositionResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}
