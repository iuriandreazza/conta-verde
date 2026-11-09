use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CalendarDate, CompanyId, PeriodLock};

use crate::RepositoryError;

/// Port for a company's closed period ([ADR 0068]): at most one, replaced as a whole.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait PeriodLockRepository: Send + Sync {
    fn find(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<PeriodLock>, RepositoryError>> + Send;

    /// Stores the lock, replacing the company's.
    fn save(
        &self,
        lock: &PeriodLock,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the company's lock, oldest first.
    fn history(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// The day through which the company is closed, if it is: what every write of a dated record asks first.
    fn locked_through(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<CalendarDate>, RepositoryError>> + Send {
        async move {
            Ok(self
                .find(company_id)
                .await?
                .and_then(|lock| lock.locked_through()))
        }
    }
}

/// No lock kept, so nothing is ever closed. It is what a service has until it is given a real lock port with
/// `with_period_lock`: the API's wiring names the lock in its types, so a deployment cannot leave it out
/// ([ADR 0068]); the services' own tests, which are about something else, need not.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPeriodLock;

impl PeriodLockRepository for NoPeriodLock {
    async fn find(&self, _company_id: CompanyId) -> Result<Option<PeriodLock>, RepositoryError> {
        Ok(None)
    }

    async fn save(&self, _lock: &PeriodLock, _audit: &AuditEntry) -> Result<(), RepositoryError> {
        Err(RepositoryError::Storage(
            "no period lock is kept here".to_owned(),
        ))
    }

    async fn history(&self, _company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        Ok(Vec::new())
    }
}
