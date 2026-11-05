use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, PaymentTermQuery};
use conta_verde_domain::{InstallmentDraft, PaymentTerm, PaymentTermDraft, PaymentTermId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, field_or};
use crate::list_params;
use crate::{Ports, SharedState};

/// One installment: due `days` after the date the term is applied to, for `percent` of the amount.
/// The percentage is a string, never a JSON number, like every decimal of this API.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallmentRequest {
    days: u32,
    percent: String,
}

/// The body of both registering and changing a payment term: a change replaces the name, the flag
/// and every installment.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PaymentTermRequest {
    name: String,
    /// In order of due date: the days increase, and the percentages add up to exactly 100.
    installments: Vec<InstallmentRequest>,
    /// Optional when registering (active), required when replacing: a body without it would
    /// otherwise switch a deactivated payment term back on.
    active: Option<bool>,
}

impl PaymentTermRequest {
    /// `default_active` is what a missing `active` means: `Some(true)` when registering, `None`
    /// when replacing, where it must be said.
    fn into_draft(self, default_active: Option<bool>) -> Result<PaymentTermDraft, ApiError> {
        let active = field_or(
            self.active,
            default_active,
            "active is required when replacing a payment term",
        )?;
        Ok(PaymentTermDraft {
            name: self.name,
            installments: self
                .installments
                .into_iter()
                .map(|installment| InstallmentDraft {
                    days: installment.days,
                    percent: installment.percent,
                })
                .collect(),
            active,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the name.
    q: Option<String>,
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
    /// `name` (default) or `-name` for descending.
    sort: Option<String>,
}

impl ListQuery {
    fn payment_term_query(&self) -> Result<PaymentTermQuery, ApiError> {
        let mut query = PaymentTermQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(active) = list_params::active(self.active.as_deref())? {
            query = query.with_active(active);
        }
        Ok(query.sorted(list_params::sort(self.sort.as_deref())?))
    }
}

#[derive(Serialize)]
pub(crate) struct InstallmentResponse {
    days: u32,
    /// A decimal string without trailing zeros, like `"33.33"` or `"50"`.
    percent: String,
}

#[derive(Serialize)]
pub(crate) struct PaymentTermResponse {
    id: Uuid,
    company_id: Uuid,
    name: String,
    installments: Vec<InstallmentResponse>,
    active: bool,
}

impl From<&PaymentTerm> for PaymentTermResponse {
    fn from(term: &PaymentTerm) -> Self {
        Self {
            id: term.id().as_uuid(),
            company_id: term.company_id().as_uuid(),
            name: term.name().to_owned(),
            installments: term
                .installments()
                .iter()
                .map(|installment| InstallmentResponse {
                    days: installment.days(),
                    percent: installment.percent().to_string(),
                })
                .collect(),
            active: term.is_active(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PaymentTermPageResponse {
    items: Vec<PaymentTermResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<PaymentTermId, ApiError> {
    Uuid::parse_str(raw)
        .map(PaymentTermId::from_uuid)
        .map_err(|_| ApplicationError::PaymentTermNotFound.into())
}

pub(crate) async fn list_payment_terms<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<PaymentTermPageResponse>, ApiError> {
    let filter = query.payment_term_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .payment_terms
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(PaymentTermPageResponse {
        items: page.items.iter().map(PaymentTermResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn register_payment_term<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<PaymentTermRequest>,
) -> Result<(StatusCode, Json<PaymentTermResponse>), ApiError> {
    let term = state
        .payment_terms
        .register(company_id, &actor, request.into_draft(Some(true))?)
        .await?;
    Ok((StatusCode::CREATED, Json(PaymentTermResponse::from(&term))))
}

/// Replaces the name, the flag and every installment. Nothing is written, and nothing is
/// recorded, when nothing changes.
pub(crate) async fn update_payment_term<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<PaymentTermRequest>,
) -> Result<Json<PaymentTermResponse>, ApiError> {
    let term = state
        .payment_terms
        .update(
            company_id,
            &actor,
            parse_id(&id)?,
            request.into_draft(None)?,
        )
        .await?;
    Ok(Json(PaymentTermResponse::from(&term)))
}

/// What was recorded about a payment term, oldest first.
pub(crate) async fn payment_term_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .payment_terms
        .history(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
