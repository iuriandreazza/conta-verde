use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, BankAccountId, CategoryId, CollectionSettings, CompanyId,
    collection_settings_changes, collection_settings_creation_changes,
};

use crate::audit::audit_entry;
use crate::{ApplicationError, CollectionSettingsRepository, EntryReferences};

/// The collection settings of a company ([ADR 0046]): where the money of a paid charge lands, and what
/// the provider's fee is entered under.
pub struct CollectionService<R, F> {
    repository: R,
    references: F,
}

impl<R: CollectionSettingsRepository, F: EntryReferences> CollectionService<R, F> {
    pub fn new(repository: R, references: F) -> Self {
        Self {
            repository,
            references,
        }
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<CollectionSettings>, ApplicationError> {
        Ok(self.repository.find(company_id).await?)
    }

    /// Replaces the settings. The account must be usable and so must the category when given, and only a
    /// reference that the change makes is checked: a category deactivated since does not keep the account
    /// from being changed. A change that changes nothing writes nothing.
    pub async fn replace(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        bank_account_id: BankAccountId,
        fee_category_id: Option<CategoryId>,
    ) -> Result<CollectionSettings, ApplicationError> {
        let before = self.repository.find(company_id).await?;
        let after = CollectionSettings::new(company_id, bank_account_id, fee_category_id);
        if before.as_ref() == Some(&after) {
            return Ok(after);
        }
        if before
            .as_ref()
            .is_none_or(|stored| stored.bank_account_id() != bank_account_id)
            && !self
                .references
                .bank_account_is_usable(company_id, bank_account_id)
                .await?
        {
            return Err(ApplicationError::UnusableReference("bank account"));
        }
        if let Some(category) = fee_category_id
            && before
                .as_ref()
                .is_none_or(|stored| stored.fee_category_id() != fee_category_id)
            && !self
                .references
                .category_is_usable(company_id, category)
                .await?
        {
            return Err(ApplicationError::UnusableReference("category"));
        }
        let (action, changes) = match &before {
            Some(stored) => (
                AuditAction::Updated,
                collection_settings_changes(stored, &after),
            ),
            None => (
                AuditAction::Created,
                collection_settings_creation_changes(&after),
            ),
        };
        let audit = audit_entry(&after, actor, action, changes);
        self.repository.save(&after, &audit).await?;
        Ok(after)
    }

    pub async fn history(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        Ok(self.repository.history(company_id).await?)
    }
}
