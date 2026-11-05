use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, PaymentMethodQuery};
use conta_verde_domain::{PaymentMethod, PaymentMethodDraft, PaymentMethodId, PaymentMethodKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, field_or};
use crate::list_params;
use crate::{Ports, SharedState};

/// The body of both registering and changing a payment method: a change replaces every field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PaymentMethodRequest {
    /// `cash`, `pix`, `bank_slip`, `credit_card`, `debit_card`, `bank_transfer`, `check` or `other`.
    kind: String,
    name: String,
    /// Optional when registering (active), required when replacing: a body without it would
    /// otherwise switch a deactivated payment method back on.
    active: Option<bool>,
}

impl PaymentMethodRequest {
    /// `default_active` is what a missing `active` means: `Some(true)` when registering, `None`
    /// when replacing, where it must be said.
    fn into_draft(self, default_active: Option<bool>) -> Result<PaymentMethodDraft, ApiError> {
        let kind = PaymentMethodKind::parse(&self.kind).ok_or(ApiError::InvalidField(
            "invalid kind: expected cash, pix, bank_slip, credit_card, debit_card, bank_transfer, check or other",
        ))?;
        let active = field_or(
            self.active,
            default_active,
            "active is required when replacing a payment method",
        )?;
        Ok(PaymentMethodDraft {
            kind,
            name: self.name,
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
    kind: Option<String>,
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
    /// `name` (default) or `-name` for descending.
    sort: Option<String>,
}

impl ListQuery {
    fn payment_method_query(&self) -> Result<PaymentMethodQuery, ApiError> {
        let mut query = PaymentMethodQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(raw) = self.kind.as_deref() {
            let kind = PaymentMethodKind::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid kind: expected cash, pix, bank_slip, credit_card, debit_card, bank_transfer, check or other",
            ))?;
            query = query.of_kind(kind);
        }
        if let Some(active) = list_params::active(self.active.as_deref())? {
            query = query.with_active(active);
        }
        Ok(query.sorted(list_params::sort(self.sort.as_deref())?))
    }
}

#[derive(Serialize)]
pub(crate) struct PaymentMethodResponse {
    id: Uuid,
    company_id: Uuid,
    #[doc = "One of the kinds a payment method can have."]
    kind: &'static str,
    name: String,
    active: bool,
}

impl From<&PaymentMethod> for PaymentMethodResponse {
    fn from(method: &PaymentMethod) -> Self {
        Self {
            id: method.id().as_uuid(),
            company_id: method.company_id().as_uuid(),
            kind: method.kind().as_str(),
            name: method.name().to_owned(),
            active: method.is_active(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PaymentMethodPageResponse {
    items: Vec<PaymentMethodResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<PaymentMethodId, ApiError> {
    Uuid::parse_str(raw)
        .map(PaymentMethodId::from_uuid)
        .map_err(|_| ApplicationError::PaymentMethodNotFound.into())
}

pub(crate) async fn list_payment_methods<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<PaymentMethodPageResponse>, ApiError> {
    let filter = query.payment_method_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .payment_methods
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(PaymentMethodPageResponse {
        items: page.items.iter().map(PaymentMethodResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn register_payment_method<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<PaymentMethodRequest>,
) -> Result<(StatusCode, Json<PaymentMethodResponse>), ApiError> {
    let method = state
        .payment_methods
        .register(company_id, &actor, request.into_draft(Some(true))?)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(PaymentMethodResponse::from(&method)),
    ))
}

/// Replaces every editable field. Nothing is written, and nothing is recorded, when nothing changes.
pub(crate) async fn update_payment_method<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<PaymentMethodRequest>,
) -> Result<Json<PaymentMethodResponse>, ApiError> {
    let method = state
        .payment_methods
        .update(
            company_id,
            &actor,
            parse_id(&id)?,
            request.into_draft(None)?,
        )
        .await?;
    Ok(Json(PaymentMethodResponse::from(&method)))
}

/// What was recorded about a payment method, oldest first.
pub(crate) async fn payment_method_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .payment_methods
        .history(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
