use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, StockSettings};

use crate::RepositoryError;

/// Port for the stock settings of a company ([ADR 0055]): at most one, replaced as a whole.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait StockSettingsRepository: Send + Sync {
    fn find(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<StockSettings>, RepositoryError>> + Send;

    /// Stores the settings, replacing the company's.
    fn save(
        &self,
        settings: &StockSettings,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the company's settings, oldest first.
    fn history(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;
}
