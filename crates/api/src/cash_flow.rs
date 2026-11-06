use axum::Json;
use axum::extract::State;
use conta_verde_application::{CashFlow, CashFlowPeriod, CashFlowQuery, Granularity};
use conta_verde_domain::{BankAccountId, CalendarDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
pub(crate) struct CashFlowParams {
    /// `YYYY-MM-DD`: the first day. Required, because the server does not know the person's today.
    from: String,
    /// `YYYY-MM-DD`: the last day.
    to: String,
    /// `day` or `month` (the default).
    granularity: Option<String>,
    /// The id of one account; every account of the company when absent.
    account: Option<String>,
}

impl CashFlowParams {
    fn cash_flow_query(&self) -> Result<CashFlowQuery, ApiError> {
        let from = CalendarDate::parse(&self.from).ok_or(ApiError::InvalidQuery(
            "invalid from: expected a date, YYYY-MM-DD",
        ))?;
        let to = CalendarDate::parse(&self.to).ok_or(ApiError::InvalidQuery(
            "invalid to: expected a date, YYYY-MM-DD",
        ))?;
        let granularity = match self.granularity.as_deref() {
            None | Some("month") => Granularity::Month,
            Some("day") => Granularity::Day,
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid granularity: expected day or month",
                ));
            }
        };
        let account = self
            .account
            .as_deref()
            .map(|raw| {
                Uuid::parse_str(raw)
                    .map(BankAccountId::from_uuid)
                    .map_err(|_| ApiError::InvalidQuery("invalid account: expected an id"))
            })
            .transpose()?;
        Ok(CashFlowQuery::new(from, to, granularity, account)?)
    }
}

#[derive(Serialize)]
pub(crate) struct PeriodResponse {
    start: String,
    end: String,
    /// What settled receivables brought in, by the day they were settled.
    received: String,
    paid: String,
    transfers_in: String,
    transfers_out: String,
    /// `received - paid + transfers_in - transfers_out`.
    net: String,
    /// The opening balances whose day falls in the period.
    new_opening_balances: String,
    /// The balance at the end of the period.
    closing_balance: String,
    /// What is open on the installments due in the period; null when one account is asked for.
    to_receive: Option<String>,
    to_pay: Option<String>,
    /// The closing balance plus what is open and due up to the end of the period.
    projected_balance: Option<String>,
}

impl From<&CashFlowPeriod> for PeriodResponse {
    fn from(period: &CashFlowPeriod) -> Self {
        Self {
            start: period.start.to_string(),
            end: period.end.to_string(),
            received: period.received.to_string(),
            paid: period.paid.to_string(),
            transfers_in: period.transfers_in.to_string(),
            transfers_out: period.transfers_out.to_string(),
            net: period.net.to_string(),
            new_opening_balances: period.new_opening_balances.to_string(),
            closing_balance: period.closing_balance.to_string(),
            to_receive: period.to_receive.map(|amount| amount.to_string()),
            to_pay: period.to_pay.map(|amount| amount.to_string()),
            projected_balance: period.projected_balance.map(|amount| amount.to_string()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PastDueResponse {
    to_receive: String,
    to_pay: String,
}

#[derive(Serialize)]
pub(crate) struct CashFlowResponse {
    from: String,
    to: String,
    granularity: &'static str,
    account_id: Option<Uuid>,
    /// The balance at the start of `from`.
    opening_balance: String,
    /// What is still open on installments that fell due before `from`; null when one account is asked
    /// for. The projected balances count it.
    past_due: Option<PastDueResponse>,
    periods: Vec<PeriodResponse>,
}

impl CashFlowResponse {
    fn new(query: &CashFlowQuery, flow: &CashFlow) -> Self {
        Self {
            from: query.from().to_string(),
            to: query.to().to_string(),
            granularity: match query.granularity() {
                Granularity::Day => "day",
                Granularity::Month => "month",
            },
            account_id: query.account().map(|id| id.as_uuid()),
            opening_balance: flow.opening_balance.to_string(),
            past_due: flow.past_due.map(|late| PastDueResponse {
                to_receive: late.receivable.to_string(),
                to_pay: late.payable.to_string(),
            }),
            periods: flow.periods.iter().map(PeriodResponse::from).collect(),
        }
    }
}

/// What settlements and transfers moved over a span, by period, and what is still open to come.
pub(crate) async fn cash_flow<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<CashFlowParams>,
) -> Result<Json<CashFlowResponse>, ApiError> {
    let query = params.cash_flow_query()?;
    let flow = state.balances.cash_flow(company_id, &query).await?;
    Ok(Json(CashFlowResponse::new(&query, &flow)))
}
