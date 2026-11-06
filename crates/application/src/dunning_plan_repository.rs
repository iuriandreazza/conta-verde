use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, DunningPlan};

use crate::RepositoryError;

/// Port for the reminder plan of a company ([ADR 0048]): at most one, replaced as a whole.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait DunningPlanRepository: Send + Sync {
    fn find(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<DunningPlan>, RepositoryError>> + Send;

    /// Stores the plan, replacing the company's, rules included.
    fn save(
        &self,
        plan: &DunningPlan,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the company's plan, oldest first.
    fn history(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// The companies whose plan is switched on, by id: who the hourly pass looks at.
    fn enabled_companies(
        &self,
    ) -> impl Future<Output = Result<Vec<CompanyId>, RepositoryError>> + Send;
}
