use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CollectionSettings, CompanyId};

use crate::RepositoryError;

/// Port for the collection settings of a company ([ADR 0046]): at most one, replaced as a whole.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait CollectionSettingsRepository: Send + Sync {
    fn find(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<CollectionSettings>, RepositoryError>> + Send;

    /// Stores the settings, replacing the company's. The database refuses an account or a category of
    /// another company.
    fn save(
        &self,
        settings: &CollectionSettings,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the company's settings, oldest first.
    fn history(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;
}
