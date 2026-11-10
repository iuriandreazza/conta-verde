use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, GatewaySettings};

use crate::RepositoryError;

/// Port for a company's gateway settings ([ADR 0085]): at most one, replaced as a whole. The secrets in it are **sealed**: this port
/// never sees a secret in the clear.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait GatewaySettingsRepository: Send + Sync {
    fn find(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<GatewaySettings>, RepositoryError>> + Send;

    /// Stores the settings, replacing the company's.
    fn save(
        &self,
        settings: &GatewaySettings,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the company's gateway settings, oldest first. It never holds a secret.
    fn history(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;
}
