use axum::Json;
use axum::extract::State;
use conta_verde_domain::{BUCKET_STARTS, CalendarDate, DelinquencyReport, DelinquentCustomer};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DelinquencyParams {
    /// `YYYY-MM-DD`: the day the report is for. Required, because the server does not know the person's today.
    on: String,
}

#[derive(Serialize)]
pub(crate) struct BucketResponse {
    /// The first day late of the range.
    from_days: i64,
    /// The last day late of the range; `null` for the last, which has none.
    to_days: Option<i64>,
    owed: String,
    installments: u64,
}

#[derive(Serialize)]
pub(crate) struct DelinquentCustomerResponse {
    /// The customer; `null` for what has no customer.
    party_id: Option<Uuid>,
    owed: String,
    installments: u64,
    oldest_due: String,
    /// What is owed in each range, in the order of `buckets`.
    by_bucket: Vec<String>,
}

impl From<&DelinquentCustomer> for DelinquentCustomerResponse {
    fn from(customer: &DelinquentCustomer) -> Self {
        Self {
            party_id: customer.party_id.map(|id| id.as_uuid()),
            owed: customer.owed.to_string(),
            installments: customer.installments,
            oldest_due: customer.oldest_due.to_string(),
            by_bucket: customer
                .by_bucket
                .iter()
                .map(|amount| amount.to_string())
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct DelinquencyResponse {
    on: String,
    owed: String,
    installments: u64,
    buckets: Vec<BucketResponse>,
    /// The biggest debt first.
    customers: Vec<DelinquentCustomerResponse>,
}

impl From<&DelinquencyReport> for DelinquencyResponse {
    fn from(report: &DelinquencyReport) -> Self {
        Self {
            on: report.on.to_string(),
            owed: report.owed.to_string(),
            installments: report.installments,
            buckets: BUCKET_STARTS
                .iter()
                .enumerate()
                .map(|(index, start)| BucketResponse {
                    from_days: *start,
                    to_days: BUCKET_STARTS.get(index + 1).map(|next| next - 1),
                    owed: report.by_bucket[index].to_string(),
                    installments: report.installments_by_bucket[index],
                })
                .collect(),
            customers: report
                .customers
                .iter()
                .map(DelinquentCustomerResponse::from)
                .collect(),
        }
    }
}

/// Who owes what late on a day, and how late ([ADR 0051]).
pub(crate) async fn delinquency<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<DelinquencyParams>,
) -> Result<Json<DelinquencyResponse>, ApiError> {
    let on = CalendarDate::parse(&params.on)
        .ok_or(ApiError::InvalidQuery("on must be a date, YYYY-MM-DD"))?;
    let report = state.entries.delinquency(company_id, on).await?;
    Ok(Json(DelinquencyResponse::from(&report)))
}
