use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CalendarDate, CompanyId, Contract, ContractId, ContractStatus, PartyId,
};

use crate::list_query::clean_search;
use crate::{NameCursor, NameSort, RepositoryError};

impl From<&Contract> for NameCursor {
    fn from(contract: &Contract) -> Self {
        Self {
            name: contract.name().to_owned(),
            id: contract.id().as_uuid(),
        }
    }
}

/// How a list of contracts is narrowed and ordered, following ADR 0018. They are ordered by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractQuery {
    search: Option<String>,
    status: Option<ContractStatus>,
    customer: Option<PartyId>,
    sort: NameSort,
}

impl ContractQuery {
    /// Matches the name anywhere in it, ignoring case. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn with_status(mut self, status: ContractStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_customer(mut self, customer: PartyId) -> Self {
        self.customer = Some(customer);
        self
    }

    pub fn sorted(mut self, sort: NameSort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn status(&self) -> Option<ContractStatus> {
        self.status
    }

    pub fn customer(&self) -> Option<PartyId> {
        self.customer
    }

    pub fn sort(&self) -> NameSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, contract: &Contract) -> bool {
        self.status.is_none_or(|status| contract.status() == status)
            && self
                .customer
                .is_none_or(|customer| contract.customer_id() == customer)
            && self.search().is_none_or(|text| {
                contract
                    .name()
                    .to_lowercase()
                    .contains(&text.to_lowercase())
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write of the register takes the audit entry that describes it and stores both **atomically**
/// ([ADR 0008]). The day a contract was billed through is bookkeeping, and is written without one.
pub trait ContractRepository: Send + Sync {
    fn save(
        &self,
        contract: &Contract,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The contract, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> impl Future<Output = Result<Option<Contract>, RepositoryError>> + Send;

    /// Replaces the editable fields, the status and the day billed through of a stored contract: `NotFound`
    /// when the company has no such one, and `ContractChanged` when it was terminated meanwhile.
    fn update(
        &self,
        contract: &Contract,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Records the day the contract was billed through. Written only forward.
    fn mark_billed(
        &self,
        company_id: CompanyId,
        id: ContractId,
        through: CalendarDate,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the contract, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` contracts of the company that match `query`, in its order, starting right after `after`.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &ContractQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Contract>, RepositoryError>> + Send;

    /// At most `limit` contracts of **any company** that are not paused that may have a cycle to bill up to `until`: started,
    /// not billed through `until` and not through their last day, by id after `after`. For the scheduled job.
    fn list_to_bill(
        &self,
        until: CalendarDate,
        after: Option<ContractId>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Contract>, RepositoryError>> + Send;
}
