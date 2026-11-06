use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use conta_verde_application::{ApplicationError, PageState, WebhookHeaders};
use conta_verde_domain::{
    Actor, BankAccountId, CalendarDate, CategoryId, Charge, ChargeId, ChargeMethod,
    CollectionSettings, CompanyId, EntryId, MAX_EVENT_BYTES,
};
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
    /// What the provider said was paid, once it was: the day, the amount (which can differ from the
    /// charge's), the fee it kept, and why the installment was not settled when it was not.
    paid_on: Option<String>,
    paid_amount: Option<String>,
    fee: Option<String>,
    note: Option<String>,
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
            paid_on: charge.receipt().map(|receipt| receipt.paid_on.to_string()),
            paid_amount: charge
                .receipt()
                .map(|receipt| receipt.paid_amount.to_string()),
            fee: charge.receipt().map(|receipt| receipt.fee.to_string()),
            note: charge.receipt().and_then(|receipt| receipt.note.clone()),
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

#[derive(Serialize)]
pub(crate) struct CollectionSettingsResponse {
    /// Where the money of a paid charge lands; `null` while the company has not chosen one, and then no
    /// charge can be issued.
    bank_account_id: Option<Uuid>,
    /// What the provider's fee is entered under, when the company wants it entered.
    fee_category_id: Option<Uuid>,
}

impl From<Option<&CollectionSettings>> for CollectionSettingsResponse {
    fn from(settings: Option<&CollectionSettings>) -> Self {
        Self {
            bank_account_id: settings.map(|settings| settings.bank_account_id().as_uuid()),
            fee_category_id: settings
                .and_then(|settings| settings.fee_category_id())
                .map(|id| id.as_uuid()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CollectionSettingsRequest {
    bank_account_id: Uuid,
    /// Left out or `null` to leave the fee only on the charge. It must be said when replacing.
    fee_category_id: Option<Uuid>,
}

pub(crate) async fn get_collection_settings<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<CollectionSettingsResponse>, ApiError> {
    let settings = state.collection.get(company_id).await?;
    Ok(Json(CollectionSettingsResponse::from(settings.as_ref())))
}

pub(crate) async fn replace_collection_settings<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<CollectionSettingsRequest>,
) -> Result<Json<CollectionSettingsResponse>, ApiError> {
    let settings = state
        .collection
        .replace(
            company_id,
            &actor,
            BankAccountId::from_uuid(request.bank_account_id),
            request.fee_category_id.map(CategoryId::from_uuid),
        )
        .await?;
    Ok(Json(CollectionSettingsResponse::from(Some(&settings))))
}

/// What was recorded about the company's collection settings, oldest first.
pub(crate) async fn collection_settings_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.collection.history(company_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

#[derive(Serialize)]
pub(crate) struct WebhookResponse {
    received: usize,
}

/// Where a payment provider is told to call ([ADR 0046]). It needs no session: it is authenticated by the
/// provider's signature over the body, checked before anything is read from it. A provider that is not the
/// configured one and a charge that is not the company's are both `404`, so nothing tells them apart.
pub(crate) async fn receive_payment_webhook<X: Ports>(
    State(state): State<SharedState<X>>,
    Path((company_id, provider)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<WebhookResponse>, ApiError> {
    let company_id = Uuid::parse_str(&company_id)
        .map(CompanyId::from_uuid)
        .map_err(|_| ApplicationError::ChargeNotFound)?;
    if body.len() > MAX_EVENT_BYTES {
        return Err(ApiError::FileTooLarge("the notice is too large"));
    }
    let headers = WebhookHeaders::new(headers.iter().filter_map(|(name, value)| {
        value
            .to_str()
            .ok()
            .map(|value| (name.as_str().to_owned(), value.to_owned()))
    }));
    let received = state
        .webhooks
        .receive(company_id, &provider, &headers, &body)
        .await?;
    Ok(Json(WebhookResponse { received }))
}

#[derive(Serialize)]
pub(crate) struct PaymentLinkResponse {
    /// Where to send the customer. It is **shown once**: only its digest is kept.
    url: String,
}

/// Makes a link to the charge's payment page, for the person to send the customer.
pub(crate) async fn new_payment_link<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, entry_id, charge_id)): Path<(Uuid, String, String)>,
) -> Result<(StatusCode, Json<PaymentLinkResponse>), ApiError> {
    let url = state
        .payment_pages
        .new_link(
            company_id,
            parse_entry(&entry_id)?,
            parse_charge(&charge_id)?,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(PaymentLinkResponse { url })))
}

/// What the customer sees at the link: the charge and who is charging, nothing of the company's books.
#[derive(Serialize)]
pub(crate) struct PublicChargeResponse {
    company_name: String,
    /// What it is for, as the company wrote it.
    description: String,
    installment: u16,
    installments: usize,
    /// `boleto`, `pix` or `card_link`.
    method: &'static str,
    /// `payable`, `paid` or `unavailable`: what the page may offer.
    state: &'static str,
    amount: String,
    due_on: String,
    /// Only while it can be paid: what the customer pays with.
    digitable_line: Option<String>,
    pix_payload: Option<String>,
    payment_url: Option<String>,
    /// Only once it was paid.
    paid_on: Option<String>,
}

/// The payment page a link leads to. It needs no session: the customer has none, and the token is the secret.
pub(crate) async fn public_charge<X: Ports>(
    State(state): State<SharedState<X>>,
    Path(token): Path<String>,
) -> Result<Json<PublicChargeResponse>, ApiError> {
    let page = state.payment_pages.page(&token).await?;
    let payable = page.state == PageState::Payable;
    let company_name = state.auth.company_name(page.charge.company_id()).await?;
    let payment = page.charge.payment();
    Ok(Json(PublicChargeResponse {
        company_name: crate::quotes::clipped(&company_name),
        description: page.description,
        installment: page.charge.installment(),
        installments: page.installments,
        method: page.charge.method().as_str(),
        state: match page.state {
            PageState::Payable => "payable",
            PageState::Paid => "paid",
            PageState::Unavailable => "unavailable",
        },
        amount: page.charge.amount().to_string(),
        due_on: page.charge.due_on().to_string(),
        digitable_line: payable.then(|| payment.digitable_line.clone()).flatten(),
        pix_payload: payable.then(|| payment.pix_payload.clone()).flatten(),
        payment_url: payable.then(|| payment.payment_url.clone()).flatten(),
        paid_on: page
            .charge
            .receipt()
            .filter(|_| page.state == PageState::Paid)
            .map(|receipt| receipt.paid_on.to_string()),
    }))
}
