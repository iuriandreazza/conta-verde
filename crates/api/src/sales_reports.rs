use axum::Json;
use axum::extract::State;
use conta_verde_application::SalesReport;
use conta_verde_domain::{CalendarDate, SalesGroup};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
pub(crate) struct ReportQuery {
    /// `customer`, `product` or `seller`.
    by: String,
    /// `YYYY-MM-DD`: the first day of sale.
    from: String,
    /// `YYYY-MM-DD`: the last day of sale.
    to: String,
}

#[derive(Serialize)]
pub(crate) struct RowResponse {
    /// The customer, product or seller; null for the sales that name no seller.
    key: Option<Uuid>,
    sales: usize,
    /// The value sold net of returns, a decimal string.
    net: String,
    /// Its part of the total, in percent with at most two decimals.
    share: String,
    /// The share of this row and the ones before it.
    cumulative: String,
    /// `A`, `B` or `C`, by where the row starts on the curve: A up to 80%, B up to 95%.
    class: &'static str,
}

#[derive(Serialize)]
pub(crate) struct ReportResponse {
    from: String,
    to: String,
    by: String,
    sales: usize,
    total: String,
    rows: Vec<RowResponse>,
}

fn group_of(raw: &str) -> Result<SalesGroup, ApiError> {
    match raw {
        "customer" => Ok(SalesGroup::Customer),
        "product" => Ok(SalesGroup::Product),
        "seller" => Ok(SalesGroup::Seller),
        _ => Err(ApiError::InvalidQuery(
            "invalid by: expected customer, product or seller",
        )),
    }
}

impl ReportResponse {
    fn new(by: String, report: &SalesReport) -> Self {
        Self {
            from: report.from.to_string(),
            to: report.to.to_string(),
            by,
            sales: report.sales,
            total: report.total.to_string(),
            rows: report
                .rows
                .iter()
                .map(|row| RowResponse {
                    key: row.key,
                    sales: row.sales,
                    net: row.net.to_string(),
                    share: row.share.to_string(),
                    cumulative: row.cumulative.to_string(),
                    class: row.class.as_str(),
                })
                .collect(),
        }
    }
}

/// What the completed sales of a span add up to by customer, product or seller, biggest first, each row with its
/// share and its ABC class.
pub(crate) async fn sales_report<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ReportQuery>,
) -> Result<Json<ReportResponse>, ApiError> {
    let group = group_of(&query.by)?;
    let from = CalendarDate::parse(&query.from)
        .ok_or(ApiError::InvalidQuery("invalid from: expected YYYY-MM-DD"))?;
    let to = CalendarDate::parse(&query.to)
        .ok_or(ApiError::InvalidQuery("invalid to: expected YYYY-MM-DD"))?;
    let report = state
        .sales_reports
        .report(company_id, group, from, to)
        .await?;
    Ok(Json(ReportResponse::new(query.by, &report)))
}
