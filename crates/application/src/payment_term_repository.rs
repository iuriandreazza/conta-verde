use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, PaymentTerm, PaymentTermId};

use crate::list_query::clean_search;
use crate::{NameCursor, NameSort, RepositoryError};

impl From<&PaymentTerm> for NameCursor {
    fn from(term: &PaymentTerm) -> Self {
        Self {
            name: term.name().to_owned(),
            id: term.id().as_uuid(),
        }
    }
}

/// How a list of payment terms is narrowed and ordered, following ADR 0018. Inactive ones are
/// listed unless asked otherwise, so that they can be found and brought back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentTermQuery {
    search: Option<String>,
    active: Option<bool>,
    sort: NameSort,
}

impl PaymentTermQuery {
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
    pub fn matches(&self, term: &PaymentTerm) -> bool {
        self.active.is_none_or(|active| term.is_active() == active)
            && self
                .search()
                .is_none_or(|text| term.name().to_lowercase().contains(&text.to_lowercase()))
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write.
pub trait PaymentTermRepository: Send + Sync {
    /// Fails with `DuplicateName` when the company already has a payment term with that name.
    fn save(
        &self,
        term: &PaymentTerm,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The payment term, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> impl Future<Output = Result<Option<PaymentTerm>, RepositoryError>> + Send;

    /// Replaces the editable fields of a stored payment term. `NotFound` when the company has
    /// no such one, `DuplicateName` when another has the name.
    fn update(
        &self,
        term: &PaymentTerm,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the payment term, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` payment terms of the company that match `query`, in its order, starting
    /// right after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PaymentTermQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<PaymentTerm>, RepositoryError>> + Send;
}
