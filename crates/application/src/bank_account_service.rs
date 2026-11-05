use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, BankAccount, BankAccountDraft, BankAccountId, CompanyId,
    bank_account_changes, bank_account_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, BankAccountQuery, BankAccountRepository, NameCursor, RepositoryError,
};

#[derive(Debug, Clone)]
pub struct BankAccountPage {
    pub items: Vec<BankAccount>,
    /// Present when more bank accounts exist after this page.
    pub next: Option<NameCursor>,
}

pub struct BankAccountService<R> {
    repository: R,
}

impl<R: BankAccountRepository> BankAccountService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Validates the draft and stores the bank account; the creation is recorded in the audit trail
    /// under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: BankAccountDraft,
    ) -> Result<BankAccount, ApplicationError> {
        let record = BankAccount::new(BankAccountId::generate(), company_id, draft)?;
        let audit = audit_entry(
            &record,
            actor,
            AuditAction::Created,
            bank_account_creation_changes(&record),
        );
        self.repository.save(&record, &audit).await?;
        Ok(record)
    }

    /// Replaces the editable fields. A draft that changes nothing writes nothing and records
    /// nothing: the trail lists changes, not attempts. Concurrent edits are last write wins.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: BankAccountId,
        draft: BankAccountDraft,
    ) -> Result<BankAccount, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        let changes = bank_account_changes(&before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // It vanished between the read and the write, or its kind differs (the kind is fixed).
                RepositoryError::NotFound => ApplicationError::BankAccountNotFound,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// What was recorded about the bank account, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's bank account answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &BankAccountQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<BankAccountPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(BankAccountPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> Result<BankAccount, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::BankAccountNotFound)
    }
}
