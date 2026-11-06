use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CalendarDate, CategoryId, CompanyId, CostCenterId, EntryId, EntryKind,
    FinancialEntry, PartyId,
};
use uuid::Uuid;

use crate::RepositoryError;
use crate::list_query::clean_search;

/// Position of the last row of a list of entries ordered by competence date, then by id (both
/// reversed for a descending sort): the date and id of the last entry are enough to resume right
/// after it in either direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryCursor {
    pub competence_date: CalendarDate,
    pub id: Uuid,
}

impl From<&FinancialEntry> for EntryCursor {
    fn from(entry: &FinancialEntry) -> Self {
        Self {
            competence_date: entry.competence_date(),
            id: entry.id().as_uuid(),
        }
    }
}

/// The order of the list of entries. Newest competence first is what a person looks at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EntrySort {
    #[default]
    CompetenceDesc,
    CompetenceAsc,
}

/// How a list of entries is narrowed and ordered, following ADR 0018.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FinancialEntryQuery {
    search: Option<String>,
    kind: Option<EntryKind>,
    category: Option<CategoryId>,
    sort: EntrySort,
}

impl FinancialEntryQuery {
    /// Matches the description anywhere in it, ignoring case. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn with_kind(mut self, kind: EntryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_category(mut self, category: CategoryId) -> Self {
        self.category = Some(category);
        self
    }

    pub fn sorted(mut self, sort: EntrySort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn kind(&self) -> Option<EntryKind> {
        self.kind
    }

    pub fn category(&self) -> Option<CategoryId> {
        self.category
    }

    pub fn sort(&self) -> EntrySort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, entry: &FinancialEntry) -> bool {
        self.kind.is_none_or(|kind| entry.kind() == kind)
            && self
                .category
                .is_none_or(|category| entry.category_id() == category)
            && self.search().is_none_or(|text| {
                entry
                    .description()
                    .to_lowercase()
                    .contains(&text.to_lowercase())
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write. The
/// installments are stored with the entry and replaced with it, in the same transaction.
pub trait FinancialEntryRepository: Send + Sync {
    fn save(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The entry, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> impl Future<Output = Result<Option<FinancialEntry>, RepositoryError>> + Send;

    /// Replaces the editable fields and the installments of a stored entry. `NotFound` when the
    /// company has no such one.
    fn update(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the entry, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` entries of the company that match `query`, in its order, starting right
    /// after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &FinancialEntryQuery,
        after: Option<&EntryCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<FinancialEntry>, RepositoryError>> + Send;
}

/// Whether what an entry points at can be used, asked before it is written ([ADR 0029]). A
/// reference to another company's record answers like one that does not exist.
pub trait EntryReferences: Send + Sync {
    /// The category exists in the company, is active and is not the one for transfers.
    fn category_is_usable(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The cost center exists in the company and is active.
    fn cost_center_is_usable(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The party exists in the company.
    fn party_exists(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;
}
