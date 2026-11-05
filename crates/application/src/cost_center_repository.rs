use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, CostCenter, CostCenterId};

use crate::list_query::clean_search;
use crate::{NameCursor, NameSort, RepositoryError};

impl From<&CostCenter> for NameCursor {
    fn from(cost_center: &CostCenter) -> Self {
        Self {
            name: cost_center.name().to_owned(),
            id: cost_center.id().as_uuid(),
        }
    }
}

/// How a list of cost centers is narrowed and ordered, following ADR 0018. Inactive ones are
/// listed unless asked otherwise, so that they can be found and brought back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CostCenterQuery {
    search: Option<String>,
    active: Option<bool>,
    sort: NameSort,
}

impl CostCenterQuery {
    /// Matches a name anywhere in it, ignoring case. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = Some(active);
        self
    }

    pub fn sorted(mut self, sort: NameSort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn active(&self) -> Option<bool> {
        self.active
    }

    pub fn sort(&self) -> NameSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, cost_center: &CostCenter) -> bool {
        self.active
            .is_none_or(|active| cost_center.is_active() == active)
            && self.search().is_none_or(|text| {
                cost_center
                    .name()
                    .to_lowercase()
                    .contains(&text.to_lowercase())
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write.
pub trait CostCenterRepository: Send + Sync {
    /// Fails with `DuplicateName` when the company already has a cost center with that name.
    fn save(
        &self,
        cost_center: &CostCenter,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The cost center, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> impl Future<Output = Result<Option<CostCenter>, RepositoryError>> + Send;

    /// Replaces the editable fields of a stored cost center. `NotFound` when the company has no
    /// such cost center, `DuplicateName` when another one of the company has the name.
    fn update(
        &self,
        cost_center: &CostCenter,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the cost center, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` cost centers of the company that match `query`, in its order, starting
    /// right after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &CostCenterQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<CostCenter>, RepositoryError>> + Send;
}
