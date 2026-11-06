use axum::Json;
use axum::extract::{Path, State};
use conta_verde_application::ApplicationError;
use conta_verde_domain::{DunningChannel, DunningPlan, DunningRule, EntryId, Notification};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::ApiJson;
use crate::{Ports, SharedState};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuleBody {
    /// Negative before the due date, 0 on it, positive after it.
    days: i16,
    /// `email` or `whatsapp`.
    channel: String,
}

#[derive(Serialize)]
pub(crate) struct DunningPlanResponse {
    enabled: bool,
    /// In the order they fall.
    rules: Vec<RuleBody>,
    /// Whether the operator configured a WhatsApp provider: without one a rule for it is never tried.
    whatsapp_available: bool,
}

fn response<X: Ports>(state: &SharedState<X>, plan: &DunningPlan) -> DunningPlanResponse {
    DunningPlanResponse {
        enabled: plan.is_enabled(),
        rules: plan
            .rules()
            .iter()
            .map(|rule| RuleBody {
                days: rule.days,
                channel: rule.channel.as_str().to_owned(),
            })
            .collect(),
        whatsapp_available: state.dunning.whatsapp_is_available(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DunningPlanRequest {
    enabled: bool,
    rules: Vec<RuleBody>,
}

pub(crate) async fn get_plan<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<DunningPlanResponse>, ApiError> {
    let plan = state.dunning.plan(company_id).await?;
    Ok(Json(response(&state, &plan)))
}

pub(crate) async fn replace_plan<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<DunningPlanRequest>,
) -> Result<Json<DunningPlanResponse>, ApiError> {
    let rules = request
        .rules
        .iter()
        .map(|rule| {
            DunningChannel::parse(&rule.channel)
                .map(|channel| DunningRule {
                    days: rule.days,
                    channel,
                })
                .ok_or(ApiError::InvalidField(
                    "a rule's channel must be email or whatsapp",
                ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let plan = state
        .dunning
        .replace_plan(company_id, &actor, request.enabled, rules)
        .await?;
    Ok(Json(response(&state, &plan)))
}

/// What was recorded about the company's reminder plan, oldest first.
pub(crate) async fn plan_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.dunning.plan_history(company_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

#[derive(Serialize)]
pub(crate) struct NotificationResponse {
    installment: u16,
    days: i16,
    channel: &'static str,
    /// Where it went.
    recipient: String,
    /// `pending`, `sent` or `failed`.
    status: &'static str,
    attempts: u8,
    note: Option<String>,
}

impl From<&Notification> for NotificationResponse {
    fn from(notification: &Notification) -> Self {
        Self {
            installment: notification.installment(),
            days: notification.days(),
            channel: notification.channel().as_str(),
            recipient: notification.recipient().to_owned(),
            status: notification.status().as_str(),
            attempts: notification.attempts(),
            note: notification.note().map(str::to_owned),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct NotificationListResponse {
    items: Vec<NotificationResponse>,
}

/// The reminders sent about an entry, oldest first.
pub(crate) async fn list_notifications<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, entry_id)): Path<(Uuid, String)>,
) -> Result<Json<NotificationListResponse>, ApiError> {
    let entry_id = Uuid::parse_str(&entry_id)
        .map(EntryId::from_uuid)
        .map_err(|_| ApplicationError::EntryNotFound)?;
    let notifications = state.dunning.notifications(company_id, entry_id).await?;
    Ok(Json(NotificationListResponse {
        items: notifications
            .iter()
            .map(NotificationResponse::from)
            .collect(),
    }))
}
