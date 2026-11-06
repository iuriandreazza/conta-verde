use axum::Json;
use axum::extract::State;
use conta_verde_application::{MovementSummary, StockValuation, ValuationRow};
use conta_verde_domain::CalendarDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

fn day(raw: &str, name: &'static str) -> Result<CalendarDate, ApiError> {
    CalendarDate::parse(raw).ok_or(ApiError::InvalidQuery(name))
}

#[derive(Deserialize)]
pub(crate) struct ValuationQuery {
    /// `YYYY-MM-DD`: what the stock held through this day. The server reads no clock, so it is required.
    on: String,
}

#[derive(Serialize)]
pub(crate) struct ValuationRowResponse {
    product_id: Uuid,
    name: String,
    sku: Option<String>,
    unit: String,
    quantity: String,
    /// The value over the quantity with six decimals; `null` without something on hand.
    average_cost: Option<String>,
    /// What the quantity is worth, in cents.
    value: String,
    /// Its part of the total value, in percent with at most two decimals.
    share: String,
}

impl From<&ValuationRow> for ValuationRowResponse {
    fn from(row: &ValuationRow) -> Self {
        Self {
            product_id: row.product.id().as_uuid(),
            name: row.product.name().to_owned(),
            sku: row.product.sku().map(str::to_owned),
            unit: row.product.unit().to_owned(),
            quantity: row.balance.quantity().to_string(),
            average_cost: row.balance.average_cost().map(|cost| cost.to_string()),
            value: row.balance.value().to_string(),
            share: row.share.to_string(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ValuationResponse {
    on: String,
    /// What the whole stock was worth, in cents.
    total: String,
    /// How many products had less than nothing on hand.
    negative: usize,
    rows: Vec<ValuationRowResponse>,
}

impl From<&StockValuation> for ValuationResponse {
    fn from(valuation: &StockValuation) -> Self {
        Self {
            on: valuation.on.to_string(),
            total: valuation.total.to_string(),
            negative: valuation.negative,
            rows: valuation
                .rows
                .iter()
                .map(ValuationRowResponse::from)
                .collect(),
        }
    }
}

/// What the stock was worth on a day, by product (biggest value first) and in all.
pub(crate) async fn valuation<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ValuationQuery>,
) -> Result<Json<ValuationResponse>, ApiError> {
    let on = day(&query.on, "invalid on: expected YYYY-MM-DD")?;
    let valuation = state.stock.valuation(company_id, on).await?;
    Ok(Json((&valuation).into()))
}

#[derive(Deserialize)]
pub(crate) struct MovementsQuery {
    /// `YYYY-MM-DD`, the day included.
    from: String,
    to: String,
}

#[derive(Serialize)]
pub(crate) struct SummaryResponse {
    product_id: Uuid,
    name: String,
    sku: Option<String>,
    unit: String,
    opening_quantity: String,
    opening_value: String,
    entered_quantity: String,
    entered_value: String,
    /// What went out, as positive numbers.
    exited_quantity: String,
    exited_value: String,
    /// What counts and adjustments added; negative when they took away.
    adjusted_quantity: String,
    adjusted_value: String,
    closing_quantity: String,
    closing_value: String,
}

impl From<&MovementSummary> for SummaryResponse {
    fn from(row: &MovementSummary) -> Self {
        Self {
            product_id: row.product.id().as_uuid(),
            name: row.product.name().to_owned(),
            sku: row.product.sku().map(str::to_owned),
            unit: row.product.unit().to_owned(),
            opening_quantity: row.opening.quantity().to_string(),
            opening_value: row.opening.value().to_string(),
            entered_quantity: row.entered_quantity.to_string(),
            entered_value: row.entered_value.to_string(),
            exited_quantity: row.exited_quantity.to_string(),
            exited_value: row.exited_value.to_string(),
            adjusted_quantity: row.adjusted_quantity.to_string(),
            adjusted_value: row.adjusted_value.to_string(),
            closing_quantity: row.closing.quantity().to_string(),
            closing_value: row.closing.value().to_string(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct MovementsResponse {
    from: String,
    to: String,
    items: Vec<SummaryResponse>,
}

/// What moved of each product over a span, with what it had before and after.
pub(crate) async fn movements<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<MovementsQuery>,
) -> Result<Json<MovementsResponse>, ApiError> {
    let from = day(&query.from, "invalid from: expected YYYY-MM-DD")?;
    let to = day(&query.to, "invalid to: expected YYYY-MM-DD")?;
    let rows = state.stock.movement_summary(company_id, from, to).await?;
    Ok(Json(MovementsResponse {
        from: from.to_string(),
        to: to.to_string(),
        items: rows.iter().map(SummaryResponse::from).collect(),
    }))
}
