use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, EntrySort, FinancialEntryQuery};
use conta_verde_domain::{
    BankAccountId, CategoryId, CostCenterId, EntryId, EntryInstallmentDraft, EntryKind,
    EntryOrigin, FinancialEntry, FinancialEntryDraft, PartyId, PaymentMethodId, Settlement,
    SettlementDraft, SettlementId, settled_of,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_date_cursor, encode_date_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::list_params;
use crate::{Ports, SharedState};

/// One installment: the day it falls due and what is due, a decimal string in cents.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallmentRequest {
    due_date: String,
    amount: String,
}

/// The body of both registering and changing an entry: a change replaces every field and every
/// installment, and a reference left out is a reference removed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EntryRequest {
    /// `receivable` or `payable`.
    kind: String,
    description: String,
    party_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    /// `YYYY-MM-DD`: the day the entry belongs to for the managerial income statement.
    competence_date: String,
    /// In order: the due dates do not decrease.
    installments: Vec<InstallmentRequest>,
}

impl EntryRequest {
    fn into_draft(self) -> Result<FinancialEntryDraft, ApiError> {
        let kind = EntryKind::parse(&self.kind)
            .ok_or(ApiError::InvalidField("kind must be receivable or payable"))?;
        Ok(FinancialEntryDraft {
            kind,
            description: self.description,
            party_id: self.party_id.map(PartyId::from_uuid),
            category_id: CategoryId::from_uuid(self.category_id),
            cost_center_id: self.cost_center_id.map(CostCenterId::from_uuid),
            competence_date: self.competence_date,
            installments: self
                .installments
                .into_iter()
                .map(|installment| EntryInstallmentDraft {
                    due_date: installment.due_date,
                    amount: installment.amount,
                })
                .collect(),
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the description.
    q: Option<String>,
    /// `receivable` or `payable`.
    kind: Option<String>,
    /// The id of a category.
    category: Option<String>,
    /// `true` for the cancelled entries only, `false` for the others only; both when it is absent.
    voided: Option<String>,
    /// `competence_date` for oldest first, or `-competence_date` (the default) for newest first.
    sort: Option<String>,
}

impl ListQuery {
    fn entry_query(&self) -> Result<FinancialEntryQuery, ApiError> {
        let mut query = FinancialEntryQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(kind) = self.kind.as_deref() {
            query = query.with_kind(EntryKind::parse(kind).ok_or(ApiError::InvalidQuery(
                "invalid kind: expected receivable or payable",
            ))?);
        }
        if let Some(category) = self.category.as_deref() {
            let id = Uuid::parse_str(category)
                .map_err(|_| ApiError::InvalidQuery("invalid category: expected an id"))?;
            query = query.with_category(CategoryId::from_uuid(id));
        }
        if let Some(voided) = list_params::active(self.voided.as_deref())
            .map_err(|_| ApiError::InvalidQuery("invalid voided: expected true or false"))?
        {
            query = query.with_voided(voided);
        }
        let sort = match self.sort.as_deref() {
            None | Some("-competence_date") => EntrySort::CompetenceDesc,
            Some("competence_date") => EntrySort::CompetenceAsc,
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid sort: expected competence_date or -competence_date",
                ));
            }
        };
        Ok(query.sorted(sort))
    }
}

#[derive(Serialize)]
pub(crate) struct InstallmentResponse {
    number: u16,
    due_date: String,
    /// A decimal string without trailing zeros, like `"150.5"` or `"2500"`.
    amount: String,
    /// How much of it the settlements that are not reversed have settled, a decimal string. What is
    /// left is `amount` less this; whether it is overdue is for the reader, who knows today's date.
    settled: String,
}

#[derive(Serialize)]
pub(crate) struct EntryResponse {
    id: Uuid,
    company_id: Uuid,
    kind: &'static str,
    description: String,
    party_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    competence_date: String,
    installments: Vec<InstallmentResponse>,
    /// The sum of the installments, a decimal string: it is not stored, it is added up.
    total: String,
    /// The recurrence that generated it, if one did.
    recurrence_id: Option<Uuid>,
    /// The sale whose receivable it is, if one made it.
    sale_id: Option<Uuid>,
    /// The return it refunds, if one made it.
    sale_return_id: Option<Uuid>,
    /// The receipt of goods whose payable it is, if one made it.
    purchase_receipt_id: Option<Uuid>,
    /// Cancelled: it counts nowhere and takes no edit or settlement.
    voided: bool,
}

impl EntryResponse {
    fn new(entry: &FinancialEntry, settlements: &[Settlement]) -> Self {
        Self {
            id: entry.id().as_uuid(),
            company_id: entry.company_id().as_uuid(),
            kind: entry.kind().as_str(),
            description: entry.description().to_owned(),
            party_id: entry.party_id().map(|id| id.as_uuid()),
            category_id: entry.category_id().as_uuid(),
            cost_center_id: entry.cost_center_id().map(|id| id.as_uuid()),
            competence_date: entry.competence_date().to_string(),
            installments: entry
                .installments()
                .iter()
                .map(|installment| InstallmentResponse {
                    number: installment.number(),
                    due_date: installment.due_date().to_string(),
                    amount: installment.amount().to_string(),
                    settled: settled_of(settlements, installment.number()).to_string(),
                })
                .collect(),
            total: entry.total().to_string(),
            recurrence_id: match entry.origin() {
                Some(EntryOrigin::Recurrence(origin)) => Some(origin.recurrence_id.as_uuid()),
                _ => None,
            },
            sale_id: match entry.origin() {
                Some(EntryOrigin::Sale(id)) => Some(id.as_uuid()),
                _ => None,
            },
            sale_return_id: match entry.origin() {
                Some(EntryOrigin::SaleReturn(id)) => Some(id.as_uuid()),
                _ => None,
            },
            purchase_receipt_id: match entry.origin() {
                Some(EntryOrigin::PurchaseReceipt(id)) => Some(id.as_uuid()),
                _ => None,
            },
            voided: entry.is_voided(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct EntryPageResponse {
    items: Vec<EntryResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<EntryId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntryId::from_uuid)
        .map_err(|_| ApplicationError::EntryNotFound.into())
}

pub(crate) async fn list_entries<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<EntryPageResponse>, ApiError> {
    let filter = query.entry_query()?;
    let after = query.after.as_deref().map(decode_date_cursor).transpose()?;
    let page = state
        .entries
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    let ids: Vec<EntryId> = page.items.iter().map(FinancialEntry::id).collect();
    let settlements = state.entries.settlements_of(company_id, &ids).await?;
    Ok(Json(EntryPageResponse {
        items: page
            .items
            .iter()
            .map(|entry| EntryResponse::new(entry, &of_entry(&settlements, entry.id())))
            .collect(),
        next_cursor: page.next.as_ref().map(encode_date_cursor),
    }))
}

pub(crate) async fn register_entry<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<EntryRequest>,
) -> Result<(StatusCode, Json<EntryResponse>), ApiError> {
    let entry = state
        .entries
        .register(company_id, &actor, request.into_draft()?)
        .await?;
    // Nothing is settled in a new entry.
    Ok((StatusCode::CREATED, Json(EntryResponse::new(&entry, &[]))))
}

pub(crate) async fn get_entry<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<EntryResponse>, ApiError> {
    let entry = state.entries.get(company_id, parse_id(&id)?).await?;
    let settlements = state.entries.settlements(company_id, entry.id()).await?;
    Ok(Json(EntryResponse::new(&entry, &settlements)))
}

/// Replaces every field and every installment. Nothing is written, and nothing is recorded, when
/// nothing changes.
pub(crate) async fn update_entry<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<EntryRequest>,
) -> Result<Json<EntryResponse>, ApiError> {
    let entry = state
        .entries
        .update(company_id, &actor, parse_id(&id)?, request.into_draft()?)
        .await?;
    let settlements = state.entries.settlements(company_id, entry.id()).await?;
    Ok(Json(EntryResponse::new(&entry, &settlements)))
}

/// What was recorded about an entry, oldest first.
pub(crate) async fn entry_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.entries.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

/// The settlements of one entry out of those of a page.
fn of_entry(settlements: &[Settlement], entry_id: EntryId) -> Vec<Settlement> {
    settlements
        .iter()
        .filter(|settlement| settlement.entry_id() == entry_id)
        .cloned()
        .collect()
}

/// Pays an installment: what is applied, and the interest, fine and discount of the day, from a bank
/// account. Amounts are decimal strings in cents.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettlementRequest {
    /// The number of the installment, from 1.
    installment: u16,
    /// `YYYY-MM-DD`: the day the money moved.
    settled_on: String,
    applied: String,
    interest: Option<String>,
    fine: Option<String>,
    discount: Option<String>,
    bank_account_id: Uuid,
    payment_method_id: Option<Uuid>,
}

#[derive(Serialize)]
pub(crate) struct SettlementResponse {
    id: Uuid,
    entry_id: Uuid,
    installment: u16,
    settled_on: String,
    applied: String,
    interest: String,
    fine: String,
    discount: String,
    /// What moved in the account: what is applied, with interest and fine, less the discount.
    cash: String,
    bank_account_id: Uuid,
    payment_method_id: Option<Uuid>,
    reversed: bool,
}

impl From<&Settlement> for SettlementResponse {
    fn from(settlement: &Settlement) -> Self {
        Self {
            id: settlement.id().as_uuid(),
            entry_id: settlement.entry_id().as_uuid(),
            installment: settlement.installment(),
            settled_on: settlement.settled_on().to_string(),
            applied: settlement.applied().to_string(),
            interest: settlement.interest().to_string(),
            fine: settlement.fine().to_string(),
            discount: settlement.discount().to_string(),
            cash: settlement.cash().to_string(),
            bank_account_id: settlement.bank_account_id().as_uuid(),
            payment_method_id: settlement.payment_method_id().map(|id| id.as_uuid()),
            reversed: settlement.is_reversed(),
        }
    }
}

fn parse_settlement_id(raw: &str) -> Result<SettlementId, ApiError> {
    Uuid::parse_str(raw)
        .map(SettlementId::from_uuid)
        .map_err(|_| ApplicationError::SettlementNotFound.into())
}

pub(crate) async fn settle_installment<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<SettlementRequest>,
) -> Result<(StatusCode, Json<SettlementResponse>), ApiError> {
    let draft = SettlementDraft {
        installment: request.installment,
        settled_on: request.settled_on,
        applied: request.applied,
        interest: request.interest,
        fine: request.fine,
        discount: request.discount,
        bank_account_id: BankAccountId::from_uuid(request.bank_account_id),
        payment_method_id: request.payment_method_id.map(PaymentMethodId::from_uuid),
    };
    let settlement = state
        .entries
        .settle(company_id, &actor, parse_id(&id)?, draft)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(SettlementResponse::from(&settlement)),
    ))
}

/// The settlements of an entry, reversed ones included, by day settled.
pub(crate) async fn list_settlements<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<SettlementResponse>>, ApiError> {
    let settlements = state
        .entries
        .settlements(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        settlements.iter().map(SettlementResponse::from).collect(),
    ))
}

/// Undoes a settlement: it stops counting and stays in the trail.
pub(crate) async fn reverse_settlement<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, entry_id, settlement_id)): Path<(Uuid, String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .entries
        .reverse(
            company_id,
            &actor,
            parse_id(&entry_id)?,
            parse_settlement_id(&settlement_id)?,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
