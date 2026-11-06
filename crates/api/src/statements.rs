use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use conta_verde_application::{
    ApplicationError, DateCursor, EntryFromLine, ImportRow, StatementLineQuery, Suggestion,
};
use conta_verde_domain::{
    BankAccountId, CategoryId, CostCenterId, LineMatch, LineStatus, PartyId, PaymentMethodId,
    SettlementId, StatementLine, StatementLineId, TransferId,
};
use conta_verde_infrastructure::{decode_statement, parse_ofx};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::csv_import::{ImportQuery, ImportResponse, read_file};
use crate::cursor::{decode_date_cursor, encode_date_cursor};
use crate::entries::SettlementResponse;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::transfers::TransferResponse;
use crate::{Ports, SharedState};

#[derive(Serialize)]
pub(crate) struct StatementLineResponse {
    id: Uuid,
    company_id: Uuid,
    account_id: Uuid,
    posted_on: String,
    /// A signed decimal string: positive came into the account, negative went out of it.
    amount: String,
    description: String,
    bank_ref: Option<String>,
    /// `pending`, `matched` or `ignored`.
    status: &'static str,
    matched_settlement_id: Option<Uuid>,
    matched_transfer_id: Option<Uuid>,
}

impl From<&StatementLine> for StatementLineResponse {
    fn from(line: &StatementLine) -> Self {
        let (settlement, transfer) = match line.matched() {
            Some(LineMatch::Settlement(id)) => (Some(id.as_uuid()), None),
            Some(LineMatch::Transfer(id)) => (None, Some(id.as_uuid())),
            None => (None, None),
        };
        Self {
            id: line.id().as_uuid(),
            company_id: line.company_id().as_uuid(),
            account_id: line.account_id().as_uuid(),
            posted_on: line.posted_on().to_string(),
            amount: line.amount().to_string(),
            description: line.description().to_owned(),
            bank_ref: line.bank_ref().map(str::to_owned),
            status: line.status().as_str(),
            matched_settlement_id: settlement,
            matched_transfer_id: transfer,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct StatementLinePageResponse {
    items: Vec<StatementLineResponse>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// The id of an account.
    account: Option<String>,
    /// `pending`, `matched` or `ignored`.
    status: Option<String>,
}

impl ListQuery {
    fn line_query(&self) -> Result<StatementLineQuery, ApiError> {
        let mut query = StatementLineQuery::default();
        if let Some(account) = self.account.as_deref() {
            let id = Uuid::parse_str(account)
                .map_err(|_| ApiError::InvalidQuery("invalid account: expected an id"))?;
            query = query.of_account(BankAccountId::from_uuid(id));
        }
        if let Some(status) = self.status.as_deref() {
            query = query.with_status(LineStatus::parse(status).ok_or(ApiError::InvalidQuery(
                "invalid status: expected pending, matched or ignored",
            ))?);
        }
        Ok(query)
    }
}

// A string, parsed here, like the other ids: axum's own rejection is plain text.
fn parse_id(raw: &str) -> Result<StatementLineId, ApiError> {
    Uuid::parse_str(raw)
        .map(StatementLineId::from_uuid)
        .map_err(|_| ApplicationError::StatementLineNotFound.into())
}

/// Reads an OFX statement and stores its lines in an account, or with `?dry_run=true` only says what
/// would happen. The whole file is checked before anything is written; a line the account already has is
/// skipped, so sending a file again, or one that overlaps, is safe (ADR 0034).
pub(crate) async fn import_statement<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, account)): Path<(Uuid, String)>,
    ApiQuery(query): ApiQuery<ImportQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<ImportResponse>, ApiError> {
    let account = Uuid::parse_str(&account)
        .map(BankAccountId::from_uuid)
        .map_err(|_| ApplicationError::BankAccountNotFound)?;
    let bytes = read_file(&headers, body).await?;
    let drafts = parse_ofx(&decode_statement(&bytes))
        .map_err(|error| ApiError::InvalidFile(error.to_string()))?;
    let rows = drafts
        .into_iter()
        .enumerate()
        .map(|(index, draft)| ImportRow {
            row: index + 1,
            draft,
        })
        .collect();
    let dry_run = query.dry_run();
    let report = state
        .reconciliation
        .import(company_id, &actor, account, rows, dry_run)
        .await?;
    let refused = report.refused.clone();
    Ok(Json(ImportResponse::new(dry_run, report, refused)))
}

/// Newest day first, whatever the status.
pub(crate) async fn list_statement_lines<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<StatementLinePageResponse>, ApiError> {
    let filter = query.line_query()?;
    let after: Option<DateCursor> = query.after.as_deref().map(decode_date_cursor).transpose()?;
    let page = state
        .reconciliation
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(StatementLinePageResponse {
        items: page.items.iter().map(StatementLineResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_date_cursor),
    }))
}

/// What a pending line may be matched to: a settlement (with the description of its entry) or a transfer.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SuggestionResponse {
    Settlement {
        settlement: SettlementResponse,
        entry_description: String,
    },
    Transfer {
        transfer: TransferResponse,
    },
}

impl From<&Suggestion> for SuggestionResponse {
    fn from(suggestion: &Suggestion) -> Self {
        match suggestion {
            Suggestion::Settlement {
                settlement,
                entry_description,
            } => Self::Settlement {
                settlement: SettlementResponse::from(settlement),
                entry_description: entry_description.clone(),
            },
            Suggestion::Transfer(transfer) => Self::Transfer {
                transfer: TransferResponse::from(transfer),
            },
        }
    }
}

pub(crate) async fn statement_line_suggestions<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<SuggestionResponse>>, ApiError> {
    let suggestions = state
        .reconciliation
        .suggestions(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        suggestions.iter().map(SuggestionResponse::from).collect(),
    ))
}

/// The settlement or the transfer to match a line to: exactly one of the two.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MatchRequest {
    settlement_id: Option<Uuid>,
    transfer_id: Option<Uuid>,
}

pub(crate) async fn match_statement_line<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<MatchRequest>,
) -> Result<Json<StatementLineResponse>, ApiError> {
    let target = match (request.settlement_id, request.transfer_id) {
        (Some(settlement), None) => LineMatch::Settlement(SettlementId::from_uuid(settlement)),
        (None, Some(transfer)) => LineMatch::Transfer(TransferId::from_uuid(transfer)),
        _ => {
            return Err(ApiError::InvalidField(
                "say exactly one of settlement_id and transfer_id",
            ));
        }
    };
    let line = state
        .reconciliation
        .match_line(company_id, &actor, parse_id(&id)?, target)
        .await?;
    Ok(Json(StatementLineResponse::from(&line)))
}

pub(crate) async fn unmatch_statement_line<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<StatementLineResponse>, ApiError> {
    let line = state
        .reconciliation
        .unmatch(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json(StatementLineResponse::from(&line)))
}

pub(crate) async fn ignore_statement_line<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<StatementLineResponse>, ApiError> {
    let line = state
        .reconciliation
        .ignore(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json(StatementLineResponse::from(&line)))
}

pub(crate) async fn restore_statement_line<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<StatementLineResponse>, ApiError> {
    let line = state
        .reconciliation
        .restore(company_id, &actor, parse_id(&id)?)
        .await?;
    Ok(Json(StatementLineResponse::from(&line)))
}

/// What the person chooses to make an entry out of a line: a category, and optionally the rest.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateEntryRequest {
    category_id: Uuid,
    party_id: Option<Uuid>,
    cost_center_id: Option<Uuid>,
    payment_method_id: Option<Uuid>,
    /// The line's own description when absent.
    description: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct CreatedEntryResponse {
    line: StatementLineResponse,
    entry_id: Uuid,
    settlement_id: Uuid,
}

/// Makes an entry settled on the day of the line, and matches the line to it.
pub(crate) async fn create_entry_from_statement_line<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<CreateEntryRequest>,
) -> Result<(StatusCode, Json<CreatedEntryResponse>), ApiError> {
    let (line, settlement) = state
        .reconciliation
        .create_entry(
            company_id,
            &actor,
            parse_id(&id)?,
            EntryFromLine {
                category_id: CategoryId::from_uuid(request.category_id),
                party_id: request.party_id.map(PartyId::from_uuid),
                cost_center_id: request.cost_center_id.map(CostCenterId::from_uuid),
                payment_method_id: request.payment_method_id.map(PaymentMethodId::from_uuid),
                description: request.description,
            },
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedEntryResponse {
            line: StatementLineResponse::from(&line),
            entry_id: settlement.entry_id().as_uuid(),
            settlement_id: settlement.id().as_uuid(),
        }),
    ))
}

/// What was recorded about a statement line, oldest first.
pub(crate) async fn statement_line_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .reconciliation
        .history(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
