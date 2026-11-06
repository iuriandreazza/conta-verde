use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, EntryId, EntryOrigin, FinancialEntry,
    FinancialEntryDraft, Settlement, SettlementDraft, SettlementId, entry_changes,
    entry_creation_changes, settlement_changes, settlement_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, EntryCursor, EntryReferences, FinancialEntryQuery, FinancialEntryRepository,
    RepositoryError,
};

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

    /// Like `register`, for an entry a recurrence generated ([ADR 0033]): it remembers where it came
    /// from, and the store refuses, with `DuplicateOccurrence`, a second one for the same
    /// occurrence day.
    pub async fn register_generated(
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
        let entry = self.find(company_id, entry_id).await?;
        let settlement = Settlement::new(SettlementId::generate(), company_id, entry_id, draft)?;
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
