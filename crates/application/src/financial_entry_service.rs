use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, EntryId, FinancialEntry, FinancialEntryDraft,
    entry_changes, entry_creation_changes,
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
                other => other.into(),
            })?;
        Ok(after)
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
