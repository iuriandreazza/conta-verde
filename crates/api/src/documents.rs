use axum::Json;
use axum::extract::{Path, State};
use conta_verde_application::{ApplicationError, MailAsk};
use conta_verde_domain::{DocumentMail, DocumentMailKind, EntryId, PurchaseOrderId, SettlementId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::clock::now_millis;
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
    CompanyAccess(company_id, actor): CompanyAccess,
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
        .send_purchase_order(
            company_id,
            &MailAsk {
                actor: &actor,
                now_millis: now_millis(),
                to: request.to.as_deref(),
                company_name: &company_name,
            },
            id,
        )
        .await?;
    Ok(Json(MailedResponse {
        to: mailed.to.as_str().to_owned(),
        emailed: mailed.emailed,
    }))
}

/// E-mails the customer the proof of a settlement of a receivable.
pub(crate) async fn send_payment_receipt<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
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
            &MailAsk {
                actor: &actor,
                now_millis: now_millis(),
                to: request.to.as_deref(),
                company_name: &company_name,
            },
            entry_id,
            settlement_id,
        )
        .await?;
    Ok(Json(MailedResponse {
        to: mailed.to.as_str().to_owned(),
        emailed: mailed.emailed,
    }))
}

#[derive(Serialize)]
pub(crate) struct SentMailResponse {
    to: String,
    /// Unix milliseconds.
    sent_at_millis: i64,
}

impl From<&DocumentMail> for SentMailResponse {
    fn from(mail: &DocumentMail) -> Self {
        Self {
            to: mail.to.as_str().to_owned(),
            sent_at_millis: mail.sent_at_millis,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SentMailsResponse {
    items: Vec<SentMailResponse>,
}

/// When and to whom a purchase order was e-mailed, newest first.
pub(crate) async fn purchase_order_mails<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<SentMailsResponse>, ApiError> {
    let id = parse(
        &id,
        PurchaseOrderId::from_uuid,
        ApplicationError::PurchaseNotFound,
    )?;
    let sent = state
        .documents
        .history(company_id, DocumentMailKind::PurchaseOrder, id.as_uuid())
        .await?;
    Ok(Json(SentMailsResponse {
        items: sent.iter().map(SentMailResponse::from).collect(),
    }))
}

/// When and to whom the receipt of a settlement was e-mailed, newest first.
pub(crate) async fn payment_receipt_mails<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, _, settlement_id)): Path<(Uuid, String, String)>,
) -> Result<Json<SentMailsResponse>, ApiError> {
    let id = parse(
        &settlement_id,
        SettlementId::from_uuid,
        ApplicationError::SettlementNotFound,
    )?;
    let sent = state
        .documents
        .history(company_id, DocumentMailKind::PaymentReceipt, id.as_uuid())
        .await?;
    Ok(Json(SentMailsResponse {
        items: sent.iter().map(SentMailResponse::from).collect(),
    }))
}
