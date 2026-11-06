use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::ApplicationError;
use conta_verde_domain::{Actor, CalendarDate, Charge, ChargeId, ChargeMethod, EntryId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::ApiJson;
use crate::{Ports, SharedState};

#[derive(Serialize)]
pub(crate) struct ChargeResponse {
    id: Uuid,
    entry_id: Uuid,
    installment: u16,
    /// `boleto`, `pix` or `card_link`.
    method: &'static str,
    /// `open`, `paid`, `cancelled` or `expired`.
    status: &'static str,
    /// A decimal string, what was left of the installment when the charge was made.
    amount: String,
    due_on: String,
    provider: String,
    /// The boleto's line, to type in an app.
    digitable_line: Option<String>,
    /// The Pix copy-and-paste text.
    pix_payload: Option<String>,
    /// The provider's page: the boleto's paper or the card checkout.
    payment_url: Option<String>,
    issued_by: String,
}

impl From<&Charge> for ChargeResponse {
    fn from(charge: &Charge) -> Self {
        let Actor::User { name, .. } = charge.issued_by();
        Self {
            id: charge.id().as_uuid(),
            entry_id: charge.entry_id().as_uuid(),
            installment: charge.installment(),
            method: charge.method().as_str(),
            status: charge.status().as_str(),
            amount: charge.amount().to_string(),
            due_on: charge.due_on().to_string(),
            provider: charge.provider().to_owned(),
            digitable_line: charge.payment().digitable_line.clone(),
            pix_payload: charge.payment().pix_payload.clone(),
            payment_url: charge.payment().payment_url.clone(),
            issued_by: name.clone(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ChargeListResponse {
    items: Vec<ChargeResponse>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueChargeRequest {
    installment: u16,
    method: String,
    /// The day the charge falls due, when it is not the installment's own.
    due_on: Option<String>,
}

/// What the web app needs to know before offering to charge.
#[derive(Serialize)]
pub(crate) struct CollectionResponse {
    /// The provider charges are made at, or `null` when the operator configured none.
    provider: Option<String>,
}

// Strings, parsed here, like the other ids: axum's own rejection is plain text.
fn parse_entry(raw: &str) -> Result<EntryId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntryId::from_uuid)
        .map_err(|_| ApplicationError::EntryNotFound.into())
}

fn parse_charge(raw: &str) -> Result<ChargeId, ApiError> {
    Uuid::parse_str(raw)
        .map(ChargeId::from_uuid)
        .map_err(|_| ApplicationError::ChargeNotFound.into())
}

pub(crate) async fn collection<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(..): CompanyAccess,
) -> Json<CollectionResponse> {
    Json(CollectionResponse {
        provider: state.charges.provider_key().map(str::to_owned),
    })
}

pub(crate) async fn list_charges<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, entry_id)): Path<(Uuid, String)>,
) -> Result<Json<ChargeListResponse>, ApiError> {
    let charges = state
        .charges
        .list(company_id, parse_entry(&entry_id)?)
        .await?;
    Ok(Json(ChargeListResponse {
        items: charges.iter().map(ChargeResponse::from).collect(),
    }))
}

pub(crate) async fn issue_charge<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, entry_id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<IssueChargeRequest>,
) -> Result<(StatusCode, Json<ChargeResponse>), ApiError> {
    let method = ChargeMethod::parse(&request.method).ok_or(ApiError::InvalidField(
        "method must be boleto, pix or card_link",
    ))?;
    let due_on = request
        .due_on
        .map(|raw| {
            CalendarDate::parse(&raw)
                .ok_or(ApiError::InvalidField("due_on must be a date, YYYY-MM-DD"))
        })
        .transpose()?;
    let charge = state
        .charges
        .issue(
            company_id,
            &actor,
            parse_entry(&entry_id)?,
            request.installment,
            method,
            due_on,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(ChargeResponse::from(&charge))))
}

pub(crate) async fn cancel_charge<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, entry_id, charge_id)): Path<(Uuid, String, String)>,
) -> Result<Json<ChargeResponse>, ApiError> {
    let charge = state
        .charges
        .cancel(
            company_id,
            &actor,
            parse_entry(&entry_id)?,
            parse_charge(&charge_id)?,
        )
        .await?;
    Ok(Json(ChargeResponse::from(&charge)))
}

/// What was recorded about a charge, oldest first.
pub(crate) async fn charge_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, entry_id, charge_id)): Path<(Uuid, String, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .charges
        .history(
            company_id,
            parse_entry(&entry_id)?,
            parse_charge(&charge_id)?,
        )
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
