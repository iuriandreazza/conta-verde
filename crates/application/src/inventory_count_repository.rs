use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CompanyId, CountStatus, InventoryCount, InventoryCountId,
};

use crate::list_query::clean_search;
use crate::{NumberCursor, NumberSort, RepositoryError};

impl From<&InventoryCount> for NumberCursor {
    fn from(count: &InventoryCount) -> Self {
        Self {
            number: count.number(),
            id: count.id().as_uuid(),
        }
    }
}

/// How a list of counts is narrowed and ordered, following ADR 0018.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryCountQuery {
    search: Option<String>,
    status: Option<CountStatus>,
    sort: NumberSort,
}

impl InventoryCountQuery {
    /// Matches the number or the notes, ignoring case.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn with_status(mut self, status: CountStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn sorted(mut self, sort: NumberSort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn status(&self) -> Option<CountStatus> {
        self.status
    }

    pub fn sort(&self) -> NumberSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, count: &InventoryCount) -> bool {
        self.status.is_none_or(|status| count.status() == status)
            && self.search().is_none_or(|text| {
                let text = text.to_lowercase();
                count.number().to_string() == text
                    || count
                        .notes()
                        .is_some_and(|notes| notes.to_lowercase().contains(&text))
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait InventoryCountRepository: Send + Sync {
    /// Stores a new count, giving it the next number of the company, which it returns.
    fn save(
        &self,
        count: &InventoryCount,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<i64, RepositoryError>> + Send;

    /// The count, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> impl Future<Output = Result<Option<InventoryCount>, RepositoryError>> + Send;

    /// Replaces the content of a stored count, only if it still has the status `expected`: `NotFound` when the
    /// company has no such count, `CountChanged` when it is in another status.
    fn update(
        &self,
        count: &InventoryCount,
        expected: CountStatus,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Changes only the status of a stored count (applying, cancelling), only if it still has the status
    /// `expected`: `NotFound` or `CountChanged`, like `update`; the content is not written.
    fn set_status(
        &self,
        count: &InventoryCount,
        expected: CountStatus,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the count, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` counts of the company that match `query`, in its order, starting right after `after`.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &InventoryCountQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<InventoryCount>, RepositoryError>> + Send;
}
