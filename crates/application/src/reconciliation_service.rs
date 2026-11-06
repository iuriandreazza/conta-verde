use std::collections::HashMap;

use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, BankAccountId, CalendarDate, CategoryId, CompanyId,
    CostCenterId, EntryInstallmentDraft, EntryKind, FinancialEntryDraft, LineMatch, LineStatus,
    PartyId, PaymentMethodId, Settlement, SettlementDraft, SettlementId, StatementLine,
    StatementLineDraft, StatementLineId, Transfer, TransferId, statement_line_changes,
    statement_line_creation_changes,
};

use crate::audit::audit_entry;
use crate::import::import_rows;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, DateCursor, EntryReferences, FinancialEntryRepository, FinancialEntryService,
    ImportReport, ImportRow, MAX_IMPORT_ROWS, RepositoryError, StatementLineQuery,
    StatementLineRepository, TransferRepository, TransferService,
};

/// How many days either side of a line a settlement or a transfer may be and still be suggested for it.
pub const SUGGESTION_WINDOW_DAYS: u32 = 5;

#[derive(Debug, Clone)]
pub struct StatementLinePage {
    pub items: Vec<StatementLine>,
    /// Present when more lines exist after this page.
    pub next: Option<DateCursor>,
}

/// What a line may be matched to, as found for it.
#[derive(Debug, Clone)]
pub enum Suggestion {
    Settlement {
        settlement: Settlement,
        /// The description of the entry it settles, for the person to recognise it by.
        entry_description: String,
    },
    Transfer(Transfer),
}

/// What the person chooses to make an entry out of a line.
#[derive(Debug, Clone)]
pub struct EntryFromLine {
    pub category_id: CategoryId,
    pub party_id: Option<PartyId>,
    pub cost_center_id: Option<CostCenterId>,
    pub payment_method_id: Option<PaymentMethodId>,
    /// The line's own description when absent.
    pub description: Option<String>,
}

/// Statement lines and their reconciliation with what is in the books ([ADR 0034]).
pub struct ReconciliationService<S, E, T, F> {
    lines: S,
    entries: FinancialEntryService<E, F>,
    transfers: TransferService<T, F>,
    references: F,
}

impl<S, E, T, F> ReconciliationService<S, E, T, F>
where
    S: StatementLineRepository,
    E: FinancialEntryRepository,
    T: TransferRepository,
    F: EntryReferences,
{
    pub fn new(
        lines: S,
        entries: FinancialEntryService<E, F>,
        transfers: TransferService<T, F>,
        references: F,
    ) -> Self {
        Self {
            lines,
            entries,
            transfers,
            references,
        }
    }

    /// Stores the lines of a statement in an active account of the company, or with `dry_run` only
    /// says what would happen. Every line is checked before any is written: if one is refused, nothing
    /// is. A line the account already has (the same day, amount, description and reference, in the same
    /// place among identical ones) is skipped, so a file sent twice, or two that overlap, add each line
    /// once.
    pub async fn import(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        account: BankAccountId,
        rows: Vec<ImportRow<StatementLineDraft>>,
        dry_run: bool,
    ) -> Result<ImportReport, ApplicationError> {
        if rows.len() > MAX_IMPORT_ROWS {
            return Err(ApplicationError::TooManyImportRows);
        }
        if !self
            .references
            .bank_account_is_usable(company_id, account)
            .await?
        {
            return Err(ApplicationError::UnusableReference("account"));
        }
        // Where each line stands among the identical ones of the file, so that two coffees of the same
        // day stay two lines when the file is sent again.
        let mut seen: HashMap<String, u32> = HashMap::new();
        let rows = rows
            .into_iter()
            .map(|ImportRow { row, draft }| {
                let key = format!(
                    "{}|{}|{}|{}",
                    draft.posted_on.trim(),
                    draft.amount.trim(),
                    draft
                        .description
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase(),
                    draft.bank_ref.as_deref().map_or("", str::trim),
                );
                let occurrence = seen.entry(key).or_insert(0);
                let item = ImportRow {
                    row,
                    draft: (draft, *occurrence),
                };
                *occurrence += 1;
                item
            })
            .collect();
        import_rows(
            rows,
            dry_run,
            "line",
            RepositoryError::DuplicateLine,
            |(draft, occurrence)| {
                StatementLine::new(
                    StatementLineId::generate(),
                    company_id,
                    account,
                    draft,
                    occurrence,
                )
            },
            |_| None,
            |line| async move {
                let audit = audit_entry(
                    &line,
                    actor,
                    AuditAction::Created,
                    statement_line_creation_changes(&line),
                );
                self.lines.save(&line, &audit).await
            },
        )
        .await
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &StatementLineQuery,
        after: Option<&DateCursor>,
        limit: Option<usize>,
    ) -> Result<StatementLinePage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .lines
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(StatementLinePage { items, next })
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> Result<StatementLine, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the line, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's line answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.lines.history(company_id, id).await?)
    }

    /// The settlements and transfers a pending line may be matched to: of its account, not reversed,
    /// not matched to another line, for exactly its amount, in the direction of its sign, within
    /// five days of it. The nearest day first. A line that is not pending has none.
    pub async fn suggestions(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> Result<Vec<Suggestion>, ApplicationError> {
        let line = self.find(company_id, id).await?;
        if line.status() != LineStatus::Pending {
            return Ok(Vec::new());
        }
        let (since, until) = window(line.posted_on());
        let kind = if line.is_incoming() {
            EntryKind::Receivable
        } else {
            EntryKind::Payable
        };
        let settlements = self
            .entries
            .settlement_candidates(
                company_id,
                line.account_id(),
                kind,
                line.magnitude(),
                since,
                until,
            )
            .await?;
        let transfers = self
            .transfers
            .candidates(
                company_id,
                line.account_id(),
                line.is_incoming(),
                line.magnitude(),
                since,
                until,
            )
            .await?;
        let taken = self
            .lines
            .matched_among(
                company_id,
                &settlements.iter().map(Settlement::id).collect::<Vec<_>>(),
                &transfers.iter().map(Transfer::id).collect::<Vec<_>>(),
            )
            .await?;
        let distance =
            |day: CalendarDate| (day.to_unix_days() - line.posted_on().to_unix_days()).abs();
        let mut found: Vec<(i64, Suggestion)> = Vec::new();
        for settlement in settlements
            .into_iter()
            .filter(|settlement| !taken.settlements.contains(&settlement.id()))
        {
            let entry_description = self
                .entries
                .get(company_id, settlement.entry_id())
                .await?
                .description()
                .to_owned();
            found.push((
                distance(settlement.settled_on()),
                Suggestion::Settlement {
                    settlement,
                    entry_description,
                },
            ));
        }
        for transfer in transfers
            .into_iter()
            .filter(|transfer| !taken.transfers.contains(&transfer.id()))
        {
            found.push((
                distance(transfer.occurred_on()),
                Suggestion::Transfer(transfer),
            ));
        }
        found.sort_by_key(|(days, _)| *days);
        Ok(found
            .into_iter()
            .map(|(_, suggestion)| suggestion)
            .collect())
    }

    /// Matches a pending line to a settlement or a transfer, after checking it is one that is
    /// suggested for it: the line's account, not reversed, the same amount in the same direction.
    pub async fn match_line(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: StatementLineId,
        target: LineMatch,
    ) -> Result<StatementLine, ApplicationError> {
        let line = self.find(company_id, id).await?;
        let moved = line.matched_to(target)?;
        self.check_target(company_id, &line, target).await?;
        self.store_as(actor, &line, moved).await
    }

    /// Takes a matched line back to pending.
    pub async fn unmatch(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: StatementLineId,
    ) -> Result<StatementLine, ApplicationError> {
        let line = self.find(company_id, id).await?;
        let moved = line.unmatched()?;
        self.store_as(actor, &line, moved).await
    }

    /// Leaves a pending line out of the reconciliation.
    pub async fn ignore(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: StatementLineId,
    ) -> Result<StatementLine, ApplicationError> {
        let line = self.find(company_id, id).await?;
        let moved = line.ignored()?;
        self.store_as(actor, &line, moved).await
    }

    /// Takes an ignored line back to pending.
    pub async fn restore(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: StatementLineId,
    ) -> Result<StatementLine, ApplicationError> {
        let line = self.find(company_id, id).await?;
        let moved = line.restored()?;
        self.store_as(actor, &line, moved).await
    }

    /// Makes an entry out of a pending line that is not in the books: one installment for its amount,
    /// due on its day, in the direction of its sign, **settled** that day in the line's account, and
    /// matches the line to that settlement. The writes are separate and each is audited: if the
    /// settlement is refused the entry stays, unsettled, and the line stays pending.
    pub async fn create_entry(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: StatementLineId,
        choice: EntryFromLine,
    ) -> Result<(StatementLine, Settlement), ApplicationError> {
        let line = self.find(company_id, id).await?;
        // Checked before anything is written.
        line.ignored()?;
        let day = line.posted_on().to_string();
        let amount = line.magnitude().to_string();
        let description = choice
            .description
            .unwrap_or_else(|| line.description().to_owned());
        let entry = self
            .entries
            .register(
                company_id,
                actor,
                FinancialEntryDraft {
                    kind: if line.is_incoming() {
                        EntryKind::Receivable
                    } else {
                        EntryKind::Payable
                    },
                    description,
                    party_id: choice.party_id,
                    category_id: choice.category_id,
                    cost_center_id: choice.cost_center_id,
                    competence_date: day.clone(),
                    installments: vec![EntryInstallmentDraft {
                        due_date: day.clone(),
                        amount: amount.clone(),
                    }],
                },
            )
            .await?;
        let settlement = self
            .entries
            .settle(
                company_id,
                actor,
                entry.id(),
                SettlementDraft {
                    installment: 1,
                    settled_on: day,
                    applied: amount,
                    interest: None,
                    fine: None,
                    discount: None,
                    bank_account_id: line.account_id(),
                    payment_method_id: choice.payment_method_id,
                },
            )
            .await?;
        let moved = line.matched_to(LineMatch::Settlement(settlement.id()))?;
        let stored = self.store_as(actor, &line, moved).await?;
        Ok((stored, settlement))
    }

    async fn check_target(
        &self,
        company_id: CompanyId,
        line: &StatementLine,
        target: LineMatch,
    ) -> Result<(), ApplicationError> {
        let fits = match target {
            LineMatch::Settlement(id) => self.settlement_fits(company_id, line, id).await?,
            LineMatch::Transfer(id) => self.transfer_fits(company_id, line, id).await?,
        };
        if fits {
            Ok(())
        } else {
            Err(ApplicationError::MatchDoesNotFit)
        }
    }

    async fn settlement_fits(
        &self,
        company_id: CompanyId,
        line: &StatementLine,
        id: SettlementId,
    ) -> Result<bool, ApplicationError> {
        let (since, until) = window(line.posted_on());
        let kind = if line.is_incoming() {
            EntryKind::Receivable
        } else {
            EntryKind::Payable
        };
        Ok(self
            .entries
            .settlement_candidates(
                company_id,
                line.account_id(),
                kind,
                line.magnitude(),
                since,
                until,
            )
            .await?
            .iter()
            .any(|settlement| settlement.id() == id))
    }

    async fn transfer_fits(
        &self,
        company_id: CompanyId,
        line: &StatementLine,
        id: TransferId,
    ) -> Result<bool, ApplicationError> {
        let (since, until) = window(line.posted_on());
        Ok(self
            .transfers
            .candidates(
                company_id,
                line.account_id(),
                line.is_incoming(),
                line.magnitude(),
                since,
                until,
            )
            .await?
            .iter()
            .any(|transfer| transfer.id() == id))
    }

    async fn store_as(
        &self,
        actor: &Actor,
        before: &StatementLine,
        after: StatementLine,
    ) -> Result<StatementLine, ApplicationError> {
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            statement_line_changes(before, &after),
        );
        self.lines
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::StatementLineNotFound,
                RepositoryError::AlreadyMatched => ApplicationError::AlreadyMatched,
                other => other.into(),
            })?;
        Ok(after)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> Result<StatementLine, ApplicationError> {
        self.lines
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::StatementLineNotFound)
    }
}

/// The days a settlement or a transfer may fall on to be suggested for a line of `day`.
fn window(day: CalendarDate) -> (CalendarDate, CalendarDate) {
    let days = day.to_unix_days();
    let at = |offset: i64| CalendarDate::from_unix_days(days + offset);
    let width = i64::from(SUGGESTION_WINDOW_DAYS);
    (
        at(-width).unwrap_or(CalendarDate::EARLIEST),
        at(width).unwrap_or(day),
    )
}
