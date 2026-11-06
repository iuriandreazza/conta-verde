use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, Charge, ChargeId, CompanyId, EntryId};

use crate::RepositoryError;

/// Port for the charges of the installments of receivables ([ADR 0045]). Futures are `Send` so use
/// cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait ChargeRepository: Send + Sync {
    /// Stores a new open charge. `ChargeAlreadyOpen` when the installment has another open one: the
    /// rule is the adapter's, inside the write, so two requests at once cannot both pass. The database
    /// refuses an entry of another company.
    fn save(
        &self,
        charge: &Charge,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Marks a stored charge cancelled, with its audit entry, atomically. `NotFound` when the
    /// company's entry has no such charge or it is not open any more.
    fn cancel(
        &self,
        cancelled: &Charge,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The charge, in any status, only if it belongs to the entry of the company.
    fn find(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: ChargeId,
    ) -> impl Future<Output = Result<Option<Charge>, RepositoryError>> + Send;

    /// The charges of the entry, in any status, in the order they were issued. Empty for another
    /// company's entry.
    fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> impl Future<Output = Result<Vec<Charge>, RepositoryError>> + Send;

    /// The charge, in any status, only if it belongs to the company: what a link leads to.
    fn find_by_id(
        &self,
        company_id: CompanyId,
        id: ChargeId,
    ) -> impl Future<Output = Result<Option<Charge>, RepositoryError>> + Send;

    /// The charge the provider knows as `external_id`, only if it is the company's.
    fn find_by_external(
        &self,
        company_id: CompanyId,
        provider: &str,
        external_id: &str,
    ) -> impl Future<Output = Result<Option<Charge>, RepositoryError>> + Send;

    /// Stores that a charge was paid, with its receipt and its audit entry, atomically. `NotFound` when
    /// the company has no such charge or it is paid already, so a payment is recorded once.
    fn mark_paid(
        &self,
        paid: &Charge,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Stores that an open charge expired, with its audit entry, atomically. `NotFound` when the company
    /// has no such charge or it is not open.
    fn expire(
        &self,
        expired: &Charge,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the charge, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: ChargeId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;
}
