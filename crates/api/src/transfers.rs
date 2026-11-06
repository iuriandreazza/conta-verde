use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{AccountBalance, ApplicationError, Balances, TransferQuery};
use conta_verde_domain::TransferId;
use conta_verde_domain::{AccountOpening, BankAccountId, CalendarDate, Transfer, TransferDraft};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::bank_accounts::BankAccountResponse;
use crate::cursor::{decode_date_cursor, encode_date_cursor};
use crate::error::ApiError;
use crate::json::{ApiJson, ApiQuery};
use crate::{Ports, SharedState};

/// Money moved from one account of the company to another. Amounts are decimal strings in cents.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TransferRequest {
    from_account_id: Uuid,
    to_account_id: Uuid,
    amount: String,
    /// `YYYY-MM-DD`: the day the money moved.
    occurred_on: String,
    /// One line, at most 200 characters.
    note: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct TransferResponse {
    id: Uuid,
    company_id: Uuid,
    from_account_id: Uuid,
    to_account_id: Uuid,
    /// A decimal string without trailing zeros.
    amount: String,
    occurred_on: String,
    note: Option<String>,
    /// Undone: it stays listed, and does not count in any balance.
    reversed: bool,
}

impl From<&Transfer> for TransferResponse {
    fn from(transfer: &Transfer) -> Self {
        Self {
            id: transfer.id().as_uuid(),
            company_id: transfer.company_id().as_uuid(),
            from_account_id: transfer.from().as_uuid(),
            to_account_id: transfer.to().as_uuid(),
            amount: transfer.amount().to_string(),
            occurred_on: transfer.occurred_on().to_string(),
            note: transfer.note().map(str::to_owned),
            reversed: transfer.is_reversed(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct TransferPageResponse {
    items: Vec<TransferResponse>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// The id of an account: the transfers that come from it or go to it.
    account: Option<String>,
}

impl ListQuery {
    fn transfer_query(&self) -> Result<TransferQuery, ApiError> {
        let mut query = TransferQuery::default();
        if let Some(account) = self.account.as_deref() {
            let id = Uuid::parse_str(account)
                .map_err(|_| ApiError::InvalidQuery("invalid account: expected an id"))?;
            query = query.with_account(BankAccountId::from_uuid(id));
        }
        Ok(query)
    }
}

// A string, parsed here, like the other ids: axum's own rejection is plain text.
fn parse_id(raw: &str) -> Result<TransferId, ApiError> {
    Uuid::parse_str(raw)
        .map(TransferId::from_uuid)
        .map_err(|_| ApplicationError::TransferNotFound.into())
}

fn parse_account(raw: &str) -> Result<BankAccountId, ApiError> {
    Uuid::parse_str(raw)
        .map(BankAccountId::from_uuid)
        .map_err(|_| ApplicationError::BankAccountNotFound.into())
}

/// Newest first, reversed ones included.
pub(crate) async fn list_transfers<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<TransferPageResponse>, ApiError> {
    let filter = query.transfer_query()?;
    let after = query.after.as_deref().map(decode_date_cursor).transpose()?;
    let page = state
        .transfers
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(TransferPageResponse {
        items: page.items.iter().map(TransferResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_date_cursor),
    }))
}

pub(crate) async fn register_transfer<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<TransferRequest>,
) -> Result<(StatusCode, Json<TransferResponse>), ApiError> {
    let draft = TransferDraft {
        from: BankAccountId::from_uuid(request.from_account_id),
        to: BankAccountId::from_uuid(request.to_account_id),
        amount: request.amount,
        occurred_on: request.occurred_on,
        note: request.note,
    };
    let transfer = state.transfers.register(company_id, &actor, draft).await?;
    Ok((StatusCode::CREATED, Json(TransferResponse::from(&transfer))))
}

/// Undoes a transfer: it stops counting and stays in the trail.
pub(crate) async fn reverse_transfer<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .transfers
        .reverse(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// What an account held at the start of a day.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpeningRequest {
    /// A signed decimal string in cents: an account in overdraft starts below zero.
    balance: String,
    /// `YYYY-MM-DD`.
    on: String,
}

#[derive(Serialize)]
pub(crate) struct OpeningResponse {
    balance: String,
    on: String,
}

impl From<&AccountOpening> for OpeningResponse {
    fn from(opening: &AccountOpening) -> Self {
        Self {
            balance: opening.balance().to_string(),
            on: opening.on().to_string(),
        }
    }
}

/// Sets or replaces the opening balance of an account. The same one again records nothing.
pub(crate) async fn set_opening_balance<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<OpeningRequest>,
) -> Result<Json<OpeningResponse>, ApiError> {
    let opening = state
        .balances
        .set_opening(
            company_id,
            &actor,
            parse_account(&id)?,
            &request.balance,
            &request.on,
        )
        .await?;
    Ok(Json(OpeningResponse::from(&opening)))
}

#[derive(Deserialize)]
pub(crate) struct BalancesQuery {
    /// `YYYY-MM-DD`: required, because the server does not know the person's today.
    on: String,
}

#[derive(Serialize)]
pub(crate) struct AccountBalanceResponse {
    account: BankAccountResponse,
    opening: Option<OpeningResponse>,
    /// What settled receivables brought in, up to and including the day.
    received: String,
    /// What settled payables took out.
    paid: String,
    transfers_in: String,
    transfers_out: String,
    /// The opening balance plus all of the above, from the opening day: 0 before it.
    balance: String,
}

impl From<&AccountBalance> for AccountBalanceResponse {
    fn from(item: &AccountBalance) -> Self {
        Self {
            account: BankAccountResponse::from(&item.account),
            opening: item.opening.as_ref().map(OpeningResponse::from),
            received: item.received.to_string(),
            paid: item.paid.to_string(),
            transfers_in: item.transfers_in.to_string(),
            transfers_out: item.transfers_out.to_string(),
            balance: item.balance.to_string(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct BalancesResponse {
    on: String,
    accounts: Vec<AccountBalanceResponse>,
    /// The sum of the balances of every account, closed ones included.
    total: String,
}

impl BalancesResponse {
    fn new(on: CalendarDate, balances: &Balances) -> Self {
        Self {
            on: on.to_string(),
            accounts: balances
                .accounts
                .iter()
                .map(AccountBalanceResponse::from)
                .collect(),
            total: balances.total.to_string(),
        }
    }
}

/// The balance of every account on a day, and of all of them together.
pub(crate) async fn balances<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<BalancesQuery>,
) -> Result<Json<BalancesResponse>, ApiError> {
    let on = CalendarDate::parse(&query.on).ok_or(ApiError::InvalidQuery(
        "invalid on: expected a date, YYYY-MM-DD",
    ))?;
    let balances = state.balances.balances(company_id, on).await?;
    Ok(Json(BalancesResponse::new(on, &balances)))
}
