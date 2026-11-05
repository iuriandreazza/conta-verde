use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, Category, CategoryId, CompanyId};

use crate::RepositoryError;

/// The most categories a company can have. The whole tree is listed at once (a tree has to be
/// drawn whole), so it is bounded where categories are created.
pub const MAX_CATEGORIES: usize = 1000;

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write.
pub trait CategoryRepository: Send + Sync {
    /// Fails with `DuplicateName` when a sibling has the name (the roots are siblings of each
    /// other), and with `DuplicateTransferCategory` for a second transfer category.
    fn save(
        &self,
        category: &Category,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Stores every category with its entry, in this order (parents before children), or none of
    /// them: one that is refused leaves nothing behind.
    fn save_all(
        &self,
        categories: &[(Category, AuditEntry)],
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The category, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> impl Future<Output = Result<Option<Category>, RepositoryError>> + Send;

    /// Replaces the name and the active flag of a stored category, the only fields that change.
    /// `NotFound` when the company has no such category, `DuplicateName` when a sibling has the
    /// name.
    fn update(
        &self,
        category: &Category,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the category, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// Every category of the company, or only the active ones or the inactive ones, ordered by
    /// name ignoring case and then by id, so that siblings come out in a fixed order.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        active: Option<bool>,
    ) -> impl Future<Output = Result<Vec<Category>, RepositoryError>> + Send;
}
