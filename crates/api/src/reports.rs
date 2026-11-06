use axum::Json;
use axum::extract::State;
use conta_verde_application::{OverdueItem, ReportBasis, ReportGroup, ReportRow};
use conta_verde_domain::{CalendarDate, EntryKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
pub(crate) struct EntriesReportParams {
    /// `category`, `cost_center` or `party`.
    group: String,
    /// `competence` (the default) or `due`.
    basis: Option<String>,
    /// `YYYY-MM-DD`: the first day. Required, because the server does not know the person's today.
    from: String,
    /// `YYYY-MM-DD`: the last day.
    to: String,
}

fn date(raw: &str, message: &'static str) -> Result<CalendarDate, ApiError> {
    CalendarDate::parse(raw).ok_or(ApiError::InvalidQuery(message))
}

#[derive(Serialize)]
pub(crate) struct ReportRowResponse {
    /// The category, cost center or party; null for what has no cost center or no party.
    id: Option<Uuid>,
    receivable: String,
    payable: String,
    /// Receivable less payable.
    net: String,
    /// How many entries are in it.
    entries: u64,
}

impl From<&ReportRow> for ReportRowResponse {
    fn from(row: &ReportRow) -> Self {
        Self {
            id: row.id,
            receivable: row.receivable.to_string(),
            payable: row.payable.to_string(),
            net: (row.receivable - row.payable).normalize().to_string(),
            entries: row.entries,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct EntriesReportResponse {
    group: &'static str,
    basis: &'static str,
    from: String,
    to: String,
    /// The biggest first.
    rows: Vec<ReportRowResponse>,
}

/// The entries of the company added up by category, cost center or party over a span (ADR 0035).
pub(crate) async fn entries_report<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<EntriesReportParams>,
) -> Result<Json<EntriesReportResponse>, ApiError> {
    let (group, group_key) = match params.group.as_str() {
        "category" => (ReportGroup::Category, "category"),
        "cost_center" => (ReportGroup::CostCenter, "cost_center"),
        "party" => (ReportGroup::Party, "party"),
        _ => {
            return Err(ApiError::InvalidQuery(
                "invalid group: expected category, cost_center or party",
            ));
        }
    };
    let (basis, basis_key) = match params.basis.as_deref() {
        None | Some("competence") => (ReportBasis::Competence, "competence"),
        Some("due") => (ReportBasis::Due, "due"),
        Some(_) => {
            return Err(ApiError::InvalidQuery(
                "invalid basis: expected competence or due",
            ));
        }
    };
    let from = date(&params.from, "invalid from: expected a date, YYYY-MM-DD")?;
    let to = date(&params.to, "invalid to: expected a date, YYYY-MM-DD")?;
    let rows = state
        .entries
        .report(company_id, group, basis, from, to)
        .await?;
    Ok(Json(EntriesReportResponse {
        group: group_key,
        basis: basis_key,
        from: from.to_string(),
        to: to.to_string(),
        rows: rows.iter().map(ReportRowResponse::from).collect(),
    }))
}

#[derive(Deserialize)]
pub(crate) struct OverdueParams {
    /// `YYYY-MM-DD`: today where the person is. Required.
    on: String,
    /// `receivable` or `payable`; both when absent.
    kind: Option<String>,
    /// At most 500; 100 when absent.
    limit: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct OverdueItemResponse {
    entry_id: Uuid,
    kind: &'static str,
    description: String,
    party_id: Option<Uuid>,
    category_id: Uuid,
    installment: u16,
    due_date: String,
    amount: String,
    settled: String,
    /// What is left: the amount less what was settled.
    open: String,
    /// How many days before `on` it fell due.
    days_late: i64,
}

impl OverdueItemResponse {
    fn new(item: &OverdueItem, on: CalendarDate) -> Self {
        Self {
            entry_id: item.entry_id.as_uuid(),
            kind: item.kind.as_str(),
            description: item.description.clone(),
            party_id: item.party_id.map(|id| id.as_uuid()),
            category_id: item.category_id.as_uuid(),
            installment: item.installment,
            due_date: item.due_date.to_string(),
            amount: item.amount.to_string(),
            settled: item.settled.to_string(),
            open: (item.amount - item.settled).normalize().to_string(),
            days_late: on.to_unix_days() - item.due_date.to_unix_days(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct OverdueResponse {
    on: String,
    /// Oldest due day first, cut at `limit`.
    items: Vec<OverdueItemResponse>,
    /// What is open on **all** the overdue installments of receivables and of payables, and how many there are,
    /// whatever the limit.
    receivable: String,
    payable: String,
    count: u64,
}

/// The installments that fell due before a day and still have something left (ADR 0035).
pub(crate) async fn overdue_report<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<OverdueParams>,
) -> Result<Json<OverdueResponse>, ApiError> {
    let on = date(&params.on, "invalid on: expected a date, YYYY-MM-DD")?;
    let kind = params
        .kind
        .as_deref()
        .map(|raw| {
            EntryKind::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid kind: expected receivable or payable",
            ))
        })
        .transpose()?;
    let report = state
        .entries
        .overdue(company_id, on, kind, params.limit)
        .await?;
    Ok(Json(OverdueResponse {
        on: on.to_string(),
        items: report
            .items
            .iter()
            .map(|item| OverdueItemResponse::new(item, on))
            .collect(),
        receivable: report.receivable.to_string(),
        payable: report.payable.to_string(),
        count: report.count,
    }))
}
