use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, BankAccountId, CalendarDate, ChargeId, CompanyId, Decimal,
    DelinquencyReport, EntryId, EntryKind, EntryOrigin, FinancialEntry, FinancialEntryDraft,
    LateInstallment, Settlement, SettlementDraft, SettlementId, entry_changes,
    entry_creation_changes, settlement_changes, settlement_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, EntryCursor, EntryReferences, FinancialEntryQuery, FinancialEntryRepository,
    OverdueReport, ReportBasis, ReportGroup, ReportRow, RepositoryError,
};

/// How many overdue installments are listed unless asked for more, and the most that can be asked for.
pub const DEFAULT_OVERDUE_LIMIT: usize = 100;
pub const MAX_OVERDUE_LIMIT: usize = 500;

#[derive(Debug, Clone)]
pub struct FinancialEntryPage {
    pub items: Vec<FinancialEntry>,
    /// Present when more entries exist after this page.
    pub next: Option<EntryCursor>,
}

/// Entries of a company, receivable and payable ([ADR 0029]).
pub struct FinancialEntryService<R, F> {
    repository: R,
    references: F,
}

impl<R: FinancialEntryRepository, F: EntryReferences> FinancialEntryService<R, F> {
    pub fn new(repository: R, references: F) -> Self {
        Self {
            repository,
            references,
        }
    }

    /// Validates the draft and its references, and stores the entry; the creation is recorded in
    /// the audit trail under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: FinancialEntryDraft,
    ) -> Result<FinancialEntry, ApplicationError> {
        let record = FinancialEntry::new(EntryId::generate(), company_id, draft)?;
        self.check_references(&record, None).await?;
        let audit = audit_entry(
            &record,
            actor,
            AuditAction::Created,
            entry_creation_changes(&record),
        );
        self.repository.save(&record, &audit).await?;
        Ok(record)
    }

    /// Like `register`, for an entry that something made ([ADR 0033], [ADR 0037]): it remembers where it
    /// came from, and the store refuses, with `DuplicateOrigin`, a second one for the same origin.
    pub async fn register_with_origin(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: FinancialEntryDraft,
        origin: EntryOrigin,
    ) -> Result<FinancialEntry, ApplicationError> {
        let record =
            FinancialEntry::new(EntryId::generate(), company_id, draft)?.with_origin(origin);
        self.check_references(&record, None).await?;
        let audit = audit_entry(
            &record,
            actor,
            AuditAction::Created,
            entry_creation_changes(&record),
        );
        self.repository.save(&record, &audit).await?;
        Ok(record)
    }

    /// Whether the draft would be accepted as an entry of the company, without writing it: the rules of
    /// the entry and of what it points at. A use case that has to be sure of the entry before it changes
    /// anything of its own asks this first.
    pub async fn validate(
        &self,
        company_id: CompanyId,
        draft: FinancialEntryDraft,
    ) -> Result<(), ApplicationError> {
        let record = FinancialEntry::new(EntryId::generate(), company_id, draft)?;
        self.check_references(&record, None).await
    }

    /// Cancels an entry ([ADR 0037]): refused while it has settlements that are not reversed, and one that
    /// is cancelled already is not found.
    pub async fn void(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: EntryId,
    ) -> Result<FinancialEntry, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.voided()?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            entry_changes(&before, &after),
        );
        self.repository
            .void(&after, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::EntryNotFound,
                RepositoryError::EntrySettled => ApplicationError::EntryIsSettled,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// The entry something made, if it did.
    pub async fn made_by(
        &self,
        company_id: CompanyId,
        origin: EntryOrigin,
    ) -> Result<Option<FinancialEntry>, ApplicationError> {
        Ok(self.repository.find_by_origin(company_id, origin).await?)
    }

    /// The entries of the company added up by `group` over a span, see [ADR 0035]. `from` is not after `to`.
    pub async fn report(
        &self,
        company_id: CompanyId,
        group: ReportGroup,
        basis: ReportBasis,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<Vec<ReportRow>, ApplicationError> {
        if from > to {
            return Err(ApplicationError::InvalidSpan("from must not be after to"));
        }
        Ok(self
            .repository
            .report_entries(company_id, group, basis, from, to)
            .await?)
    }

    /// Who owes what late on the day `on`, and how late ([ADR 0051]): the installments of receivables that are not
    /// cancelled that fell due before it and still have something left, added up by customer and by range.
    pub async fn delinquency(
        &self,
        company_id: CompanyId,
        on: CalendarDate,
    ) -> Result<DelinquencyReport, ApplicationError> {
        let Some(until) = on.pred() else {
            return Ok(DelinquencyReport::build(on, &[]));
        };
        let late: Vec<LateInstallment> = self
            .repository
            .receivable_installments_due(company_id, CalendarDate::EARLIEST, until)
            .await?
            .into_iter()
            .map(|item| LateInstallment {
                party_id: item.party_id,
                due_date: item.due_date,
                owed: item.amount - item.settled,
            })
            .collect();
        Ok(DelinquencyReport::build(on, &late))
    }

    /// The installments that fell due before `on` and still have something left ([ADR 0035]).
    pub async fn overdue(
        &self,
        company_id: CompanyId,
        on: CalendarDate,
        kind: Option<EntryKind>,
        limit: Option<usize>,
    ) -> Result<OverdueReport, ApplicationError> {
        let limit = limit
            .unwrap_or(DEFAULT_OVERDUE_LIMIT)
            .clamp(1, MAX_OVERDUE_LIMIT);
        Ok(self
            .repository
            .overdue_installments(company_id, on, kind, limit)
            .await?)
    }

    /// The settlements a bank statement line may be, see `settlement_candidates` of the port.
    pub async fn settlement_candidates(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        kind: EntryKind,
        cash: Decimal,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<Settlement>, ApplicationError> {
        Ok(self
            .repository
            .settlement_candidates(company_id, account, kind, cash, since, until)
            .await?)
    }

    /// Replaces the editable fields and the installments. A draft that changes nothing writes
    /// nothing and records nothing: the trail lists changes, not attempts. Concurrent edits are
    /// last write wins.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: EntryId,
        draft: FinancialEntryDraft,
    ) -> Result<FinancialEntry, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        let changes = entry_changes(&before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        self.check_references(&after, Some(&before)).await?;
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // It vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::EntryNotFound,
                RepositoryError::EntrySettled => ApplicationError::EntryIsSettled,
                RepositoryError::EntryVoided => ApplicationError::EntryIsVoided,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// Pays an installment, in part or in full ([ADR 0030]): the bank account and the payment
    /// method must be usable, the entry must have the installment, and what is applied must fit
    /// in what is left of it. The creation is recorded in the audit trail in the same write.
    pub async fn settle(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        entry_id: EntryId,
        draft: SettlementDraft,
    ) -> Result<Settlement, ApplicationError> {
        self.settle_as(company_id, actor, entry_id, draft, None)
            .await
    }

    /// Like `settle`, for the payment of a charge ([ADR 0046]): the settlement remembers the charge, and the
    /// store refuses, with `DuplicateOrigin`, a second one for it, even if the first was reversed.
    pub async fn settle_charge(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        entry_id: EntryId,
        draft: SettlementDraft,
        charge_id: ChargeId,
    ) -> Result<Settlement, ApplicationError> {
        self.settle_as(company_id, actor, entry_id, draft, Some(charge_id))
            .await
    }

    async fn settle_as(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        entry_id: EntryId,
        draft: SettlementDraft,
        charge_id: Option<ChargeId>,
    ) -> Result<Settlement, ApplicationError> {
        let entry = self.find(company_id, entry_id).await?;
        if entry.is_voided() {
            return Err(ApplicationError::EntryIsVoided);
        }
        let settlement = Settlement::new(SettlementId::generate(), company_id, entry_id, draft)?
            .with_charge(charge_id);
        if entry.installment(settlement.installment()).is_none() {
            return Err(ApplicationError::UnusableReference("installment"));
        }
        if !self
            .references
            .bank_account_is_usable(company_id, settlement.bank_account_id())
            .await?
        {
            return Err(ApplicationError::UnusableReference("bank account"));
        }
        if let Some(method) = settlement.payment_method_id()
            && !self
                .references
                .payment_method_is_usable(company_id, method)
                .await?
        {
            return Err(ApplicationError::UnusableReference("payment method"));
        }
        let audit = audit_entry(
            &settlement,
            actor,
            AuditAction::Created,
            settlement_creation_changes(&settlement),
        );
        self.repository
            .settle(&settlement, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::EntryNotFound,
                RepositoryError::Overpaid => ApplicationError::SettlementExceedsBalance,
                RepositoryError::EntryVoided => ApplicationError::EntryIsVoided,
                other => other.into(),
            })?;
        Ok(settlement)
    }

    /// Undoes a settlement: it is marked reversed, stops counting, and stays in the trail. One
    /// that is not there, or was reversed already, is not found.
    pub async fn reverse(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        entry_id: EntryId,
        id: SettlementId,
    ) -> Result<Settlement, ApplicationError> {
        let stored = self
            .repository
            .settlement(company_id, entry_id, id)
            .await?
            .filter(|settlement| !settlement.is_reversed())
            .ok_or(ApplicationError::SettlementNotFound)?;
        let reversed = stored.clone().with_reversed(true);
        let audit = audit_entry(
            &reversed,
            actor,
            AuditAction::Updated,
            settlement_changes(&stored, &reversed),
        );
        self.repository
            .reverse(&reversed, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::SettlementNotFound,
                other => other.into(),
            })?;
        Ok(reversed)
    }

    /// The settlements of an entry of the company, reversed ones included.
    pub async fn settlements(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Settlement>, ApplicationError> {
        self.find(company_id, entry_id).await?;
        Ok(self
            .repository
            .settlements_of(company_id, &[entry_id])
            .await?)
    }

    /// The settlements of several entries of the company at once (a page of the list), reversed
    /// ones included: one read instead of one per entry.
    pub async fn settlements_of(
        &self,
        company_id: CompanyId,
        entry_ids: &[EntryId],
    ) -> Result<Vec<Settlement>, ApplicationError> {
        Ok(self
            .repository
            .settlements_of(company_id, entry_ids)
            .await?)
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<FinancialEntry, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the entry, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's entry answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &FinancialEntryQuery,
        after: Option<&EntryCursor>,
        limit: Option<usize>,
    ) -> Result<FinancialEntryPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(FinancialEntryPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<FinancialEntry, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::EntryNotFound)
    }

    /// The category, cost center and party must be usable in the company, the answer naming the
    /// field so the form can say which. When the entry was already stored, only a reference that
    /// the edit **changes** is checked: a category that was deactivated since does not stop the
    /// description or an installment from being fixed, it only cannot be chosen again.
    async fn check_references(
        &self,
        entry: &FinancialEntry,
        before: Option<&FinancialEntry>,
    ) -> Result<(), ApplicationError> {
        let company = entry.company_id();
        if before.is_none_or(|stored| stored.category_id() != entry.category_id())
            && !self
                .references
                .category_is_usable(company, entry.category_id())
                .await?
        {
            return Err(ApplicationError::UnusableReference("category"));
        }
        if let Some(id) = entry.cost_center_id()
            && before.is_none_or(|stored| stored.cost_center_id() != entry.cost_center_id())
            && !self.references.cost_center_is_usable(company, id).await?
        {
            return Err(ApplicationError::UnusableReference("cost center"));
        }
        if let Some(id) = entry.party_id()
            && before.is_none_or(|stored| stored.party_id() != entry.party_id())
            && !self.references.party_exists(company, id).await?
        {
            return Err(ApplicationError::UnusableReference("party"));
        }
        Ok(())
    }
}
