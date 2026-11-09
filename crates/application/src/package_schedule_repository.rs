use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CalendarDate, CompanyId, PackageSchedule};

use crate::RepositoryError;

/// Port for a company's schedule of the monthly package ([ADR 0072]): at most one, replaced as a whole, and the claims
/// that make each month go out once.
///
/// Every write of the schedule takes the audit entry that describes it and stores both **atomically** ([ADR 0008]). A
/// claim is operational, like a job: it is not audited and has no actor.
pub trait PackageScheduleRepository: Send + Sync {
    fn find(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<PackageSchedule>, RepositoryError>> + Send;

    /// Stores the schedule, replacing the company's.
    fn save(
        &self,
        schedule: &PackageSchedule,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the company's schedule, oldest first.
    fn history(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// The companies whose schedule is on and whose day is `day_of_month` or earlier: the ones a pass on that day of a
    /// month may owe a package. By company id.
    fn due(
        &self,
        day_of_month: u8,
    ) -> impl Future<Output = Result<Vec<CompanyId>, RepositoryError>> + Send;

    /// Claims the package of a month (its first day) for the company, in one statement: `true` for the one caller that
    /// got it, `false` for every other, however many instances run. `at_millis` is when, from the caller's clock.
    fn claim(
        &self,
        company_id: CompanyId,
        month: CalendarDate,
        at_millis: i64,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// Gives a claim back, so the next pass tries the month again. Nothing there is not an error.
    fn release(
        &self,
        company_id: CompanyId,
        month: CalendarDate,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
}
