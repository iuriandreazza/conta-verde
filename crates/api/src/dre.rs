use axum::Json;
use axum::extract::State;
use conta_verde_application::{CostCenterFilter, Dre, DreBy};
use conta_verde_domain::{CalendarDate, CostCenterId, DreColumn, DreColumnKey, DreLines};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
pub(crate) struct DreParams {
    /// `YYYY-MM-DD`: the first day. Required, because the server does not know the person's today.
    from: String,
    /// `YYYY-MM-DD`: the last day.
    to: String,
    /// `month` (the default) or `cost_center`: what a column is.
    by: Option<String>,
    /// One cost center's id, or `none` for the entries that have none; every entry when absent.
    cost_center: Option<String>,
}

fn date(raw: &str, message: &'static str) -> Result<CalendarDate, ApiError> {
    CalendarDate::parse(raw).ok_or(ApiError::InvalidQuery(message))
}

#[derive(Serialize)]
pub(crate) struct DreLinesResponse {
    gross_revenue: String,
    deductions: String,
    costs: String,
    operating_expenses: String,
    financial_result: String,
    /// Below the line: not part of the result.
    investments: String,
    outside_result: String,
    /// Worked out from the groups: gross revenue less deductions, less costs, less operating expenses, plus the
    /// financial result.
    net_revenue: String,
    gross_profit: String,
    operating_result: String,
    net_result: String,
}

fn text(amount: conta_verde_domain::Decimal) -> String {
    amount.normalize().to_string()
}

impl From<&DreLines> for DreLinesResponse {
    fn from(lines: &DreLines) -> Self {
        Self {
            gross_revenue: text(lines.gross_revenue),
            deductions: text(lines.deductions),
            costs: text(lines.costs),
            operating_expenses: text(lines.operating_expenses),
            financial_result: text(lines.financial_result),
            investments: text(lines.investments),
            outside_result: text(lines.outside_result),
            net_revenue: text(lines.net_revenue()),
            gross_profit: text(lines.gross_profit()),
            operating_result: text(lines.operating_result()),
            net_result: text(lines.net_result()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct DreColumnResponse {
    /// `YYYY-MM` for a month; a cost center's id, or null for the entries that have none.
    key: Option<String>,
    lines: DreLinesResponse,
}

impl From<&DreColumn> for DreColumnResponse {
    fn from(column: &DreColumn) -> Self {
        let key = match column.key {
            DreColumnKey::Month(month) => Some(format!("{:04}-{:02}", month.year(), month.month())),
            DreColumnKey::CostCenter(id) => id.map(|id| id.as_uuid().to_string()),
        };
        Self {
            key,
            lines: (&column.lines).into(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct DreResponse {
    from: String,
    to: String,
    by: &'static str,
    columns: Vec<DreColumnResponse>,
    total: DreLinesResponse,
}

impl From<&Dre> for DreResponse {
    fn from(dre: &Dre) -> Self {
        Self {
            from: dre.from.to_string(),
            to: dre.to.to_string(),
            by: match dre.by {
                DreBy::Month => "month",
                DreBy::CostCenter => "cost_center",
            },
            columns: dre.columns.iter().map(DreColumnResponse::from).collect(),
            total: (&dre.total).into(),
        }
    }
}

/// The managerial income statement over a span, by month or by cost center ([ADR 0070]).
pub(crate) async fn income_statement<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<DreParams>,
) -> Result<Json<DreResponse>, ApiError> {
    let by = match params.by.as_deref() {
        None | Some("month") => DreBy::Month,
        Some("cost_center") => DreBy::CostCenter,
        Some(_) => {
            return Err(ApiError::InvalidQuery(
                "invalid by: expected month or cost_center",
            ));
        }
    };
    let filter = match params.cost_center.as_deref() {
        None => CostCenterFilter::All,
        Some("none") => CostCenterFilter::Without,
        Some(raw) => {
            CostCenterFilter::Only(CostCenterId::from_uuid(Uuid::parse_str(raw).map_err(
                |_| ApiError::InvalidQuery("invalid cost_center: expected an id or none"),
            )?))
        }
    };
    let from = date(&params.from, "invalid from: expected a date, YYYY-MM-DD")?;
    let to = date(&params.to, "invalid to: expected a date, YYYY-MM-DD")?;
    let statement = state
        .dre
        .statement(company_id, from, to, by, filter)
        .await?;
    Ok(Json((&statement).into()))
}
