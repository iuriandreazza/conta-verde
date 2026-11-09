use axum::Json;
use axum::extract::State;
use conta_verde_domain::{CalendarDate, DomainError, PeriodLock};
use serde::{Deserialize, Serialize};

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::ApiJson;
use crate::{Ports, SharedState};

#[derive(Serialize)]
pub(crate) struct PeriodLockResponse {
    /// `YYYY-MM-DD`, the last day that is closed, or `null` when nothing is.
    locked_through: Option<String>,
}

impl From<&PeriodLock> for PeriodLockResponse {
    fn from(lock: &PeriodLock) -> Self {
        Self {
            locked_through: lock.locked_through().map(|day| day.to_string()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PeriodLockRequest {
    /// The last day of the month to close through, or `null` to reopen everything. It is said every time: a
    /// request that leaves it out is refused instead of reopening what it never mentioned.
    #[serde(deserialize_with = "Option::deserialize")]
    locked_through: Option<String>,
    /// Why, when this opens days that were closed; the trail keeps it.
    reason: Option<String>,
}

pub(crate) async fn get_period_lock<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<PeriodLockResponse>, ApiError> {
    let lock = state.period_locks.get(company_id).await?;
    Ok(Json((&lock).into()))
}

/// Closes through a month or, with an earlier date, none or a reason, reopens ([ADR 0069]).
pub(crate) async fn put_period_lock<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<PeriodLockRequest>,
) -> Result<Json<PeriodLockResponse>, ApiError> {
    let through = request
        .locked_through
        .as_deref()
        .map(|raw| CalendarDate::parse(raw).ok_or(DomainError::InvalidDate("locked_through")))
        .transpose()
        .map_err(conta_verde_application::ApplicationError::from)?;
    let lock = state
        .period_locks
        .replace(company_id, &actor, through, request.reason.as_deref())
        .await?;
    Ok(Json((&lock).into()))
}

/// Who closed and reopened what, oldest first.
pub(crate) async fn period_lock_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.period_locks.history(company_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

#[derive(Serialize)]
pub(crate) struct PackageScheduleResponse {
    /// Whether the previous month's package is announced to the accountants.
    enabled: bool,
    /// The day of the month, 1 to 28, from which it is.
    day: u8,
    /// How many members the announcement reaches: the ones whose role is accountant. With none, nothing is sent.
    accountants: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageScheduleRequest {
    /// Both are said every time: a request that leaves one out is refused, so it cannot switch the schedule by
    /// forgetting what it was.
    enabled: bool,
    day: u8,
}

/// The schedule of the monthly package and who it would reach ([ADR 0072]).
pub(crate) async fn get_package_schedule<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<PackageScheduleResponse>, ApiError> {
    let schedule = state.packages.get(company_id).await?;
    let accountants = state.packages.accountants(company_id).await?;
    Ok(Json(PackageScheduleResponse {
        enabled: schedule.is_enabled(),
        day: schedule.day(),
        accountants,
    }))
}

pub(crate) async fn put_package_schedule<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<PackageScheduleRequest>,
) -> Result<Json<PackageScheduleResponse>, ApiError> {
    let schedule = state
        .packages
        .replace(company_id, &actor, request.enabled, request.day)
        .await?;
    let accountants = state.packages.accountants(company_id).await?;
    Ok(Json(PackageScheduleResponse {
        enabled: schedule.is_enabled(),
        day: schedule.day(),
        accountants,
    }))
}

/// Who chose what, oldest first.
pub(crate) async fn package_schedule_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.packages.history(company_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
