//! The company's bank connections ([ADR 0089]): the person links a bank account of the company to an account at the aggregator, and
//! its statement is pulled every day into statement lines. It holds no credential; the aggregator's keys are the installation's.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{AggregatedAccount, ApplicationError, SyncReport};
use conta_verde_domain::{BankAccountId, BankConnection, BankConnectionId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::clock::{now_millis, today};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::ApiJson;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectRequest {
    bank_account_id: Uuid,
    /// The aggregator's connection with the bank, which its widget gave back.
    item_id: String,
    /// The aggregator's account inside it.
    account_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TokenRequest {
    /// The connection to renew, when there is one.
    item_id: Option<String>,
}

/// Asked with a `POST`, so that only who may write finance spends the installation's calls to the aggregator on an id of their choice.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccountsRequest {
    item_id: String,
}

#[derive(Serialize)]
pub(crate) struct ConnectionResponse {
    id: Uuid,
    bank_account_id: Uuid,
    provider: String,
    item_id: String,
    account_id: String,
    /// `connected`, `needs_attention` or `disconnected`.
    status: &'static str,
    /// `YYYY-MM-DD`: the last day a pull reached.
    last_synced_on: Option<String>,
    /// Why the last pull failed, or what the bank needs; a few words, never what the aggregator answered.
    last_error: Option<String>,
    connected_by: String,
    created_at: i64,
}

impl From<&BankConnection> for ConnectionResponse {
    fn from(connection: &BankConnection) -> Self {
        Self {
            id: connection.id().as_uuid(),
            bank_account_id: connection.bank_account_id().as_uuid(),
            provider: connection.provider().to_owned(),
            item_id: connection.item_id().to_owned(),
            account_id: connection.account_id().to_owned(),
            status: connection.status().as_str(),
            last_synced_on: connection.last_synced_on().map(|day| day.to_string()),
            last_error: connection.last_error().map(str::to_owned),
            connected_by: connection.connected_by().name().to_owned(),
            created_at: connection.created_at(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ConnectionListResponse {
    items: Vec<ConnectionResponse>,
    /// Whether the server has an aggregator configured; when it has none, OFX import is the way to bring a statement in.
    available: bool,
    /// The aggregator's key (`pluggy`), or `none`.
    provider: &'static str,
}

#[derive(Serialize)]
pub(crate) struct TokenResponse {
    access_token: String,
}

#[derive(Serialize)]
pub(crate) struct AccountResponse {
    id: String,
    name: String,
    kind: String,
    number: Option<String>,
}

impl From<AggregatedAccount> for AccountResponse {
    fn from(account: AggregatedAccount) -> Self {
        Self {
            id: account.id,
            name: account.name,
            kind: account.kind,
            number: account.number,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct AccountListResponse {
    items: Vec<AccountResponse>,
}

#[derive(Serialize)]
pub(crate) struct SyncResponse {
    created: usize,
    skipped: usize,
    refused: usize,
    needs_attention: bool,
}

impl From<SyncReport> for SyncResponse {
    fn from(report: SyncReport) -> Self {
        Self {
            created: report.created,
            skipped: report.skipped,
            refused: report.refused,
            needs_attention: report.needs_attention,
        }
    }
}

// A string, parsed here: axum's own rejection for a malformed id is plain text.
fn parse_id(raw: &str) -> Result<BankConnectionId, ApiError> {
    Uuid::parse_str(raw)
        .map(BankConnectionId::from_uuid)
        .map_err(|_| ApplicationError::BankConnectionNotFound.into())
}

pub(crate) async fn list_connections<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<ConnectionListResponse>, ApiError> {
    let connections = state.bank_sync.list(company_id).await?;
    Ok(Json(ConnectionListResponse {
        items: connections.iter().map(ConnectionResponse::from).collect(),
        available: state.bank_sync.is_available(),
        provider: state.bank_sync.provider(),
    }))
}

pub(crate) async fn connect_token<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiJson(request): ApiJson<TokenRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    let access_token = state
        .bank_sync
        .connect_token(company_id, request.item_id.as_deref())
        .await?;
    Ok(Json(TokenResponse { access_token }))
}

pub(crate) async fn aggregator_accounts<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(_, _): CompanyAccess,
    ApiJson(request): ApiJson<AccountsRequest>,
) -> Result<Json<AccountListResponse>, ApiError> {
    let accounts = state.bank_sync.accounts_of(&request.item_id).await?;
    Ok(Json(AccountListResponse {
        items: accounts.into_iter().map(AccountResponse::from).collect(),
    }))
}

pub(crate) async fn connect<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<ConnectRequest>,
) -> Result<(StatusCode, Json<ConnectionResponse>), ApiError> {
    let connection = state
        .bank_sync
        .connect(
            company_id,
            &actor,
            BankAccountId::from_uuid(request.bank_account_id),
            &request.item_id,
            &request.account_id,
            now_millis(),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ConnectionResponse::from(&connection)),
    ))
}

pub(crate) async fn sync_now<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<SyncResponse>, ApiError> {
    let report = state
        .bank_sync
        .sync(company_id, parse_id(&id)?, today())
        .await?;
    Ok(Json(SyncResponse::from(report)))
}

pub(crate) async fn disconnect<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<ConnectionResponse>, ApiError> {
    let connection = state
        .bank_sync
        .disconnect(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json(ConnectionResponse::from(&connection)))
}

pub(crate) async fn connection_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.bank_sync.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
