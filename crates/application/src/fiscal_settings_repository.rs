use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, FiscalSettings};

use crate::RepositoryError;

/// Port for the fiscal settings of a company ([ADR 0074]): at most one, replaced as a whole with its operation
/// natures and their tax rules.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait FiscalSettingsRepository: Send + Sync {
    fn find(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<FiscalSettings>, RepositoryError>> + Send;

    /// Stores the settings, replacing the company's, natures and rules included, in one transaction.
    fn save(
        &self,
        settings: &FiscalSettings,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the company's settings, oldest first.
    fn history(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;
}
