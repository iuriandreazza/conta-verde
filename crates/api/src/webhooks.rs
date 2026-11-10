//! The company's webhook subscriptions, managed from the web app by owners and admins ([ADR 0088]). A secret is generated here
//! and shown once, or typed by the person; it is never read back.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, DeliveryCursor};
use conta_verde_domain::{
    WebhookDelivery, WebhookDeliveryId, WebhookEventKind, WebhookSubscription,
    WebhookSubscriptionDraft, WebhookSubscriptionId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::clock::now_millis;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::{Ports, SharedState};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubscriptionRequest {
    url: String,
    /// The events asked for; none means every one.
    events: Vec<String>,
    /// Optional when making (on), required when replacing: a body without it would otherwise switch a subscription back on.
    active: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SecretRequest {
    /// A secret of the person's own (8 to 200 printable characters, no spaces); absent to have one made.
    secret: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct SubscriptionResponse {
    id: Uuid,
    url: String,
    events: Vec<&'static str>,
    active: bool,
    created_by: String,
    created_at: i64,
}

impl From<&WebhookSubscription> for SubscriptionResponse {
    fn from(subscription: &WebhookSubscription) -> Self {
        Self {
            id: subscription.id().as_uuid(),
            url: subscription.url().to_owned(),
            events: subscription
                .events()
                .iter()
                .map(|kind| kind.as_str())
                .collect(),
            active: subscription.is_active(),
            created_by: subscription.created_by().name().to_owned(),
            created_at: subscription.created_at(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SubscriptionListResponse {
    items: Vec<SubscriptionResponse>,
    /// Whether the server can seal secrets: when it cannot, webhooks are off and nothing can be made.
    available: bool,
    /// What a subscription may ask for.
    events: Vec<&'static str>,
}

/// A subscription just made: the only answer that carries its secret.
#[derive(Serialize)]
pub(crate) struct SubscribedResponse {
    subscription: SubscriptionResponse,
    secret: String,
}

#[derive(Serialize)]
pub(crate) struct SecretResponse {
    /// The secret that was made, shown now and never again; `null` when the person typed their own.
    secret: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct DeliveryResponse {
    id: Uuid,
    event_id: Uuid,
    kind: &'static str,
    /// `pending`, `delivered` or `failed`.
    status: &'static str,
    attempts: u32,
    last_status: Option<u16>,
    last_error: Option<String>,
    created_at: i64,
    updated_at: i64,
    delivered_at: Option<i64>,
}

impl From<&WebhookDelivery> for DeliveryResponse {
    fn from(delivery: &WebhookDelivery) -> Self {
        Self {
            id: delivery.id.as_uuid(),
            event_id: delivery.event_id,
            kind: delivery.kind.as_str(),
            status: delivery.status.as_str(),
            attempts: delivery.attempts,
            last_status: delivery.last_status,
            last_error: delivery.last_error.clone(),
            created_at: delivery.created_at,
            updated_at: delivery.updated_at,
            delivered_at: delivery.delivered_at,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct DeliveryPageResponse {
    items: Vec<DeliveryResponse>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeliveryListQuery {
    limit: Option<usize>,
    after: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text.
fn subscription_id(raw: &str) -> Result<WebhookSubscriptionId, ApiError> {
    Uuid::parse_str(raw)
        .map(WebhookSubscriptionId::from_uuid)
        .map_err(|_| ApplicationError::WebhookNotFound.into())
}

fn encode_cursor(cursor: &DeliveryCursor) -> String {
    format!("{}|{}", cursor.created_at, cursor.id)
}

fn decode_cursor(raw: &str) -> Result<DeliveryCursor, ApiError> {
    let (created_at, id) = raw.split_once('|').ok_or(ApiError::InvalidCursor)?;
    Ok(DeliveryCursor {
        created_at: created_at.parse().map_err(|_| ApiError::InvalidCursor)?,
        id: Uuid::parse_str(id).map_err(|_| ApiError::InvalidCursor)?,
    })
}

pub(crate) async fn list_subscriptions<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<SubscriptionListResponse>, ApiError> {
    let subscriptions = state.webhook_subscriptions.list(company_id).await?;
    Ok(Json(SubscriptionListResponse {
        items: subscriptions
            .iter()
            .map(SubscriptionResponse::from)
            .collect(),
        available: state.webhook_subscriptions.is_available(),
        events: WebhookEventKind::SUBSCRIBABLE
            .iter()
            .map(|kind| kind.as_str())
            .collect(),
    }))
}

pub(crate) async fn create_subscription<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<SubscriptionRequest>,
) -> Result<(StatusCode, Json<SubscribedResponse>), ApiError> {
    let made = state
        .webhook_subscriptions
        .create(
            company_id,
            &actor,
            WebhookSubscriptionDraft {
                url: request.url,
                events: request.events,
                active: request.active.unwrap_or(true),
            },
            now_millis(),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(SubscribedResponse {
            subscription: SubscriptionResponse::from(&made.subscription),
            secret: made.secret,
        }),
    ))
}

pub(crate) async fn update_subscription<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<SubscriptionRequest>,
) -> Result<Json<SubscriptionResponse>, ApiError> {
    let active = request.active.ok_or(ApiError::InvalidField(
        "active is required when replacing a subscription",
    ))?;
    let subscription = state
        .webhook_subscriptions
        .update(
            company_id,
            &actor,
            subscription_id(&id)?,
            WebhookSubscriptionDraft {
                url: request.url,
                events: request.events,
                active,
            },
        )
        .await?;
    Ok(Json(SubscriptionResponse::from(&subscription)))
}

pub(crate) async fn replace_secret<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<SecretRequest>,
) -> Result<Json<SecretResponse>, ApiError> {
    let secret = state
        .webhook_subscriptions
        .replace_secret(
            company_id,
            &actor,
            subscription_id(&id)?,
            request.secret.as_deref(),
            now_millis(),
        )
        .await?;
    Ok(Json(SecretResponse { secret }))
}

pub(crate) async fn send_test<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<(StatusCode, Json<DeliveryResponse>), ApiError> {
    let delivery = state
        .webhook_subscriptions
        .send_test(company_id, subscription_id(&id)?, now_millis())
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(DeliveryResponse::from(&delivery)),
    ))
}

pub(crate) async fn list_deliveries<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiQuery(query): ApiQuery<DeliveryListQuery>,
) -> Result<Json<DeliveryPageResponse>, ApiError> {
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .webhook_subscriptions
        .deliveries(
            company_id,
            subscription_id(&id)?,
            after.as_ref(),
            query.limit,
        )
        .await?;
    Ok(Json(DeliveryPageResponse {
        items: page.items.iter().map(DeliveryResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn redeliver<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id, delivery_id)): Path<(Uuid, String, String)>,
) -> Result<(StatusCode, Json<DeliveryResponse>), ApiError> {
    let delivery_id = Uuid::parse_str(&delivery_id)
        .map(WebhookDeliveryId::from_uuid)
        .map_err(|_| ApplicationError::WebhookDeliveryNotFound)?;
    let delivery = state
        .webhook_subscriptions
        .redeliver(company_id, subscription_id(&id)?, delivery_id, now_millis())
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(DeliveryResponse::from(&delivery)),
    ))
}

pub(crate) async fn subscription_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .webhook_subscriptions
        .history(company_id, subscription_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
