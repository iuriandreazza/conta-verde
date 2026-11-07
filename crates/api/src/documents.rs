use axum::Json;
use axum::extract::{Path, State};
use conta_verde_application::ApplicationError;
use conta_verde_domain::{EntryId, PurchaseOrderId, SettlementId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::json::ApiJson;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MailRequest {
    /// Where to send it; the party's own address when left out.
    to: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct MailedResponse {
    to: String,
    /// Taken for sending. `false` when no mail server is configured: nothing went out.
    emailed: bool,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON errors of the rest of
// the API.
fn parse<T>(raw: &str, make: fn(Uuid) -> T, missing: ApplicationError) -> Result<T, ApiError> {
    Uuid::parse_str(raw).map(make).map_err(|_| missing.into())
}

/// E-mails an approved or partly received purchase order to its supplier, or to the address given.
pub(crate) async fn send_purchase_order<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<MailRequest>,
) -> Result<Json<MailedResponse>, ApiError> {
    let id = parse(
        &id,
        PurchaseOrderId::from_uuid,
        ApplicationError::PurchaseNotFound,
    )?;
    let company_name = state.auth.company_name(company_id).await?;
    let mailed = state
        .documents
        .send_purchase_order(company_id, id, request.to.as_deref(), &company_name)
        .await?;
    Ok(Json(MailedResponse {
        to: mailed.to.as_str().to_owned(),
        emailed: mailed.emailed,
    }))
}

/// E-mails the customer the proof of a settlement of a receivable.
pub(crate) async fn send_payment_receipt<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, entry_id, settlement_id)): Path<(Uuid, String, String)>,
    ApiJson(request): ApiJson<MailRequest>,
) -> Result<Json<MailedResponse>, ApiError> {
    let entry_id = parse(
        &entry_id,
        EntryId::from_uuid,
        ApplicationError::EntryNotFound,
    )?;
    let settlement_id = parse(
        &settlement_id,
        SettlementId::from_uuid,
        ApplicationError::SettlementNotFound,
    )?;
    let company_name = state.auth.company_name(company_id).await?;
    let mailed = state
        .documents
        .send_payment_receipt(
            company_id,
            entry_id,
            settlement_id,
            request.to.as_deref(),
            &company_name,
        )
        .await?;
    Ok(Json(MailedResponse {
        to: mailed.to.as_str().to_owned(),
        emailed: mailed.emailed,
    }))
}
