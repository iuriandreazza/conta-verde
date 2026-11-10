//! The company's e-commerce connectors, managed from the web app by owners and admins ([ADR 0090]). Turning one on makes an API key
//! and a webhook subscription through the same services as their own pages; the two secrets are in the answer that makes them and in
//! no other.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, ConnectorView, TurnedOn};
use conta_verde_domain::{Connector, ConnectorId, ConnectorKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::clock::now_millis;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::ApiJson;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateRequest {
    /// The kind of the catalogue, such as `woocommerce`.
    kind: String,
    /// What the person calls the store.
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivateRequest {
    /// The public `https` address where the package listens for the webhooks.
    address: String,
}

#[derive(Serialize)]
pub(crate) struct ConnectorResponse {
    id: Uuid,
    kind: &'static str,
    name: String,
    active: bool,
    /// `off`, `on`, or `attention` when it is on but the key was revoked or the subscription switched off elsewhere.
    state: &'static str,
    key_usable: bool,
    subscription_on: bool,
    api_key_id: Option<Uuid>,
    webhook_subscription_id: Option<Uuid>,
    created_at: i64,
    created_by: String,
}

impl ConnectorResponse {
    fn of(connector: &Connector, key_usable: bool, subscription_on: bool) -> Self {
        let state = match (connector.is_active(), key_usable && subscription_on) {
            (false, _) => "off",
            (true, true) => "on",
            (true, false) => "attention",
        };
        Self {
            id: connector.id().as_uuid(),
            kind: connector.kind().as_str(),
            name: connector.name().to_owned(),
            active: connector.is_active(),
            state,
            key_usable,
            subscription_on,
            api_key_id: connector.api_key_id().map(|id| id.as_uuid()),
            webhook_subscription_id: connector.webhook_subscription_id().map(|id| id.as_uuid()),
            created_at: connector.created_at(),
            created_by: connector.created_by().name().to_owned(),
        }
    }
}

impl From<&ConnectorView> for ConnectorResponse {
    fn from(view: &ConnectorView) -> Self {
        Self::of(&view.connector, view.key_usable, view.subscription_on)
    }
}

#[derive(Serialize)]
pub(crate) struct CatalogueEntry {
    kind: &'static str,
    title: &'static str,
    /// What the key may do, as `area:read|write`.
    scopes: &'static [&'static str],
    /// What the subscription asks to be told.
    events: &'static [&'static str],
    /// Where the package and its instructions are, inside the repository.
    package_path: &'static str,
}

#[derive(Serialize)]
pub(crate) struct ConnectorListResponse {
    items: Vec<ConnectorResponse>,
    catalogue: Vec<CatalogueEntry>,
    /// Whether the installation can seal the subscription's secret; without it nothing is turned on.
    available: bool,
}

#[derive(Serialize)]
pub(crate) struct TurnedOnResponse {
    connector: ConnectorResponse,
    /// The key the package speaks with. Shown only now.
    api_key_secret: String,
    /// What signs the webhooks the package receives. Shown only now.
    webhook_secret: String,
}

impl From<TurnedOn> for TurnedOnResponse {
    fn from(on: TurnedOn) -> Self {
        Self {
            connector: ConnectorResponse::of(&on.connector, true, true),
            api_key_secret: on.api_key_secret,
            webhook_secret: on.webhook_secret,
        }
    }
}

// A string, parsed here: axum's own rejection for a malformed id is plain text.
fn parse_id(raw: &str) -> Result<ConnectorId, ApiError> {
    Uuid::parse_str(raw)
        .map(ConnectorId::from_uuid)
        .map_err(|_| ApplicationError::ConnectorNotFound.into())
}

pub(crate) async fn list_connectors<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<ConnectorListResponse>, ApiError> {
    let views = state.connectors.list(company_id, now_millis()).await?;
    Ok(Json(ConnectorListResponse {
        items: views.iter().map(ConnectorResponse::from).collect(),
        catalogue: ConnectorKind::ALL
            .iter()
            .map(|kind| CatalogueEntry {
                kind: kind.as_str(),
                title: kind.title(),
                scopes: kind.scopes(),
                events: kind.events(),
                package_path: kind.package_path(),
            })
            .collect(),
        available: state.connectors.is_available(),
    }))
}

pub(crate) async fn create_connector<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<CreateRequest>,
) -> Result<(StatusCode, Json<ConnectorResponse>), ApiError> {
    let kind = ConnectorKind::parse(&request.kind).ok_or_else(|| ApiError::InvalidField("kind"))?;
    let connector = state
        .connectors
        .create(company_id, &actor, kind, &request.name, now_millis())
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ConnectorResponse::of(&connector, false, false)),
    ))
}

pub(crate) async fn activate_connector<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<ActivateRequest>,
) -> Result<Json<TurnedOnResponse>, ApiError> {
    let on = state
        .connectors
        .turn_on(
            company_id,
            &actor,
            parse_id(&id)?,
            &request.address,
            now_millis(),
        )
        .await?;
    Ok(Json(TurnedOnResponse::from(on)))
}

pub(crate) async fn deactivate_connector<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<ConnectorResponse>, ApiError> {
    let connector = state
        .connectors
        .turn_off(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json(ConnectorResponse::of(&connector, false, false)))
}

pub(crate) async fn connector_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.connectors.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
