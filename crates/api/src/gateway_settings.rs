use axum::Json;
use axum::extract::State;
use conta_verde_application::{
    ApplicationError, FiscalGatewayResolver, GatewaySettingsInput, SecretChange,
};
use conta_verde_domain::{GatewayKind, GatewaySecret, GatewaySettings, SecretText};
use serde::{Deserialize, Serialize};

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::ApiJson;
use crate::{Ports, SharedState};

/// What to do with one secret: `keep` the stored one, `set` a new one (with its `value`) or `clear` it. A value never comes back.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SecretChangeRequest {
    action: String,
    value: Option<String>,
}

impl SecretChangeRequest {
    fn into_change(self) -> Result<SecretChange, ApiError> {
        match (self.action.as_str(), self.value) {
            ("keep", None) => Ok(SecretChange::Keep),
            ("clear", None) => Ok(SecretChange::Clear),
            ("set", Some(value)) => Ok(SecretChange::Set(
                SecretText::new(&value).map_err(ApplicationError::from)?,
            )),
            _ => Err(ApiError::InvalidField(
                "a secret is `keep`, `clear` or `set` with a value (and a value only with `set`)",
            )),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GatewaySettingsRequest {
    /// `focus` or `nfeio`.
    kind: String,
    focus_token_homologation: SecretChangeRequest,
    focus_token_production: SecretChangeRequest,
    nfeio_api_key: SecretChangeRequest,
    webhook_secret: SecretChangeRequest,
    nfeio_company_homologation: Option<String>,
    nfeio_company_production: Option<String>,
    nfeio_search_confirmed: bool,
}

impl GatewaySettingsRequest {
    fn into_input(self) -> Result<GatewaySettingsInput, ApiError> {
        Ok(GatewaySettingsInput {
            kind: GatewayKind::parse(&self.kind)
                .ok_or(ApiError::InvalidField("kind must be focus or nfeio"))?,
            focus_token_homologation: self.focus_token_homologation.into_change()?,
            focus_token_production: self.focus_token_production.into_change()?,
            nfeio_api_key: self.nfeio_api_key.into_change()?,
            webhook_secret: self.webhook_secret.into_change()?,
            nfeio_company_homologation: self.nfeio_company_homologation,
            nfeio_company_production: self.nfeio_company_production,
            nfeio_search_confirmed: self.nfeio_search_confirmed,
        })
    }
}

/// The company's gateway settings **without a single secret**: only whether each is set.
#[derive(Serialize)]
pub(crate) struct GatewaySettingsResponse {
    /// `focus` or `nfeio`, or `null` while the company has set none (and the server's own settings, if any, are used).
    kind: Option<&'static str>,
    focus_token_homologation_set: bool,
    focus_token_production_set: bool,
    nfeio_api_key_set: bool,
    webhook_secret_set: bool,
    nfeio_company_homologation: Option<String>,
    nfeio_company_production: Option<String>,
    nfeio_search_confirmed: bool,
    /// Whether the server can seal secrets (it has `FISCAL_SECRETS_KEY`): without it a secret cannot be saved.
    encryption_available: bool,
    /// The gateway the server's own environment configures, used while the company has set none.
    server_gateway: Option<String>,
    /// Where the gateway is told to knock, relative to the server's address.
    webhook_path: String,
    /// The secrets that are stored and **cannot be read now** (the server's key changed or was lost): they must be typed again.
    unreadable: Vec<&'static str>,
}

async fn response_of<X: Ports>(
    state: &SharedState<X>,
    company_id: conta_verde_domain::CompanyId,
    settings: Option<&GatewaySettings>,
) -> Result<GatewaySettingsResponse, ApiError> {
    let unreadable = state
        .gateway_settings
        .unreadable(company_id)
        .await?
        .into_iter()
        .map(GatewaySecret::field)
        .collect();
    let set = |secret| settings.is_some_and(|settings| settings.secret(secret).is_some());
    Ok(GatewaySettingsResponse {
        kind: settings.map(|settings| settings.kind().key()),
        focus_token_homologation_set: set(GatewaySecret::FocusTokenHomologation),
        focus_token_production_set: set(GatewaySecret::FocusTokenProduction),
        nfeio_api_key_set: set(GatewaySecret::NfeIoApiKey),
        webhook_secret_set: set(GatewaySecret::WebhookSecret),
        nfeio_company_homologation: settings
            .and_then(|settings| settings.nfeio_company_homologation().map(str::to_owned)),
        nfeio_company_production: settings
            .and_then(|settings| settings.nfeio_company_production().map(str::to_owned)),
        nfeio_search_confirmed: settings.is_some_and(GatewaySettings::nfeio_search_confirmed),
        encryption_available: state.gateway_settings.encryption_available(),
        server_gateway: state.fiscal_documents.gateways().server_key(),
        webhook_path: format!("/public/fiscal/{}/webhook", company_id.as_uuid()),
        unreadable,
    })
}

pub(crate) async fn get_gateway_settings<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<GatewaySettingsResponse>, ApiError> {
    let settings = state.gateway_settings.get(company_id).await?;
    Ok(Json(
        response_of(&state, company_id, settings.as_ref()).await?,
    ))
}

pub(crate) async fn replace_gateway_settings<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(body): ApiJson<serde_json::Value>,
) -> Result<Json<GatewaySettingsResponse>, ApiError> {
    // Read here, not by the extractor: a body of the wrong shape is answered with a fixed message, because serde's own would quote
    // the value it found, and the value may be a secret.
    let request: GatewaySettingsRequest = serde_json::from_value(body).map_err(|_| {
        ApiError::InvalidField(
            "the gateway settings have the wrong shape: every secret is { action: keep|clear|set, value? }",
        )
    })?;
    let settings = state
        .gateway_settings
        .replace(company_id, &actor, request.into_input()?)
        .await?;
    Ok(Json(
        response_of(&state, company_id, Some(&settings)).await?,
    ))
}

/// Removes the company's own settings: the server's own are used again. A write by the method, so only the owner and the admin.
pub(crate) async fn reset_gateway_settings<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
) -> Result<Json<GatewaySettingsResponse>, ApiError> {
    state.gateway_settings.reset(company_id, &actor).await?;
    Ok(Json(response_of(&state, company_id, None).await?))
}

/// What was recorded about the company's gateway settings, oldest first: that a secret was set, changed or cleared, never its value.
pub(crate) async fn gateway_settings_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.gateway_settings.history(company_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
