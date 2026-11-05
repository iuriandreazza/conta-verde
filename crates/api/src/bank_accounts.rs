use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, BankAccountQuery};
use conta_verde_domain::{BankAccount, BankAccountDraft, BankAccountId, BankAccountKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, flag_or};
use crate::list_params;
use crate::{Ports, SharedState};

/// The body of both registering and changing a bank account: a change replaces every field, and its
/// `kind` must be the one it was registered with.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BankAccountRequest {
    /// `checking`, `savings`, `cash` or `card`.
    kind: String,
    name: String,
    /// 3 digits (COMPE), or 8 (ISPB) for an institution without one.
    bank_code: Option<String>,
    branch: Option<String>,
    number: Option<String>,
    /// Optional when registering (active), required when replacing: a body without it would
    /// otherwise switch a deactivated account back on.
    active: Option<bool>,
}

impl BankAccountRequest {
    /// `default_active` is what a missing `active` means: `Some(true)` when registering, `None`
    /// when replacing, where it must be said.
    fn into_draft(self, default_active: Option<bool>) -> Result<BankAccountDraft, ApiError> {
        let kind = BankAccountKind::parse(&self.kind).ok_or(ApiError::InvalidField(
            "invalid kind: expected checking, savings, cash or card",
        ))?;
        let active = flag_or(
            self.active,
            default_active,
            "active is required when replacing a bank account",
        )?;
        Ok(BankAccountDraft {
            kind,
            name: self.name,
            bank_code: self.bank_code,
            branch: self.branch,
            number: self.number,
            active,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the name, or in the account number ignoring its punctuation.
    q: Option<String>,
    /// `checking`, `savings`, `cash` or `card`.
    kind: Option<String>,
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
    /// `name` (default) or `-name` for descending.
    sort: Option<String>,
}

impl ListQuery {
    fn bank_account_query(&self) -> Result<BankAccountQuery, ApiError> {
        let mut query = BankAccountQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(raw) = self.kind.as_deref() {
            let kind = BankAccountKind::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid kind: expected checking, savings, cash or card",
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
pub(crate) struct BankAccountResponse {
    id: Uuid,
    company_id: Uuid,
    kind: &'static str,
    name: String,
    bank_code: Option<String>,
    /// Upper case letters and digits, without the punctuation it was written with.
    branch: Option<String>,
    number: Option<String>,
    active: bool,
}

impl From<&BankAccount> for BankAccountResponse {
    fn from(account: &BankAccount) -> Self {
        Self {
            id: account.id().as_uuid(),
            company_id: account.company_id().as_uuid(),
            kind: account.kind().as_str(),
            name: account.name().to_owned(),
            bank_code: account.bank_code().map(str::to_owned),
            branch: account.branch().map(str::to_owned),
            number: account.number().map(str::to_owned),
            active: account.is_active(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct BankAccountPageResponse {
    items: Vec<BankAccountResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<BankAccountId, ApiError> {
    Uuid::parse_str(raw)
        .map(BankAccountId::from_uuid)
        .map_err(|_| ApplicationError::BankAccountNotFound.into())
}

pub(crate) async fn list_bank_accounts<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<BankAccountPageResponse>, ApiError> {
    let filter = query.bank_account_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .bank_accounts
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(BankAccountPageResponse {
        items: page.items.iter().map(BankAccountResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn register_bank_account<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<BankAccountRequest>,
) -> Result<(StatusCode, Json<BankAccountResponse>), ApiError> {
    let account = state
        .bank_accounts
        .register(company_id, &actor, request.into_draft(Some(true))?)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(BankAccountResponse::from(&account)),
    ))
}

/// Replaces every editable field. Nothing is written, and nothing is recorded, when nothing changes.
pub(crate) async fn update_bank_account<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<BankAccountRequest>,
) -> Result<Json<BankAccountResponse>, ApiError> {
    let account = state
        .bank_accounts
        .update(
            company_id,
            &actor,
            parse_id(&id)?,
            request.into_draft(None)?,
        )
        .await?;
    Ok(Json(BankAccountResponse::from(&account)))
}

/// What was recorded about a bank account, oldest first.
pub(crate) async fn bank_account_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .bank_accounts
        .history(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
