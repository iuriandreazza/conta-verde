use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CompanyId, PaymentMethod, PaymentMethodId, PaymentMethodKind,
};

use crate::list_query::clean_search;
use crate::{NameCursor, NameSort, RepositoryError};

impl From<&PaymentMethod> for NameCursor {
    fn from(method: &PaymentMethod) -> Self {
        Self {
            name: method.name().to_owned(),
            id: method.id().as_uuid(),
        }
    }
}

/// How a list of payment methods is narrowed and ordered, following ADR 0018. Inactive ones are
/// listed unless asked otherwise, so that they can be found and brought back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentMethodQuery {
    search: Option<String>,
    kind: Option<PaymentMethodKind>,
    active: Option<bool>,
    sort: NameSort,
}

impl PaymentMethodQuery {
    /// Matches a name anywhere in it, ignoring case. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn of_kind(mut self, kind: PaymentMethodKind) -> Self {
        self.kind = Some(kind);
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

    pub fn kind(&self) -> Option<PaymentMethodKind> {
        self.kind
    }

    pub fn active(&self) -> Option<bool> {
        self.active
    }

    pub fn sort(&self) -> NameSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, method: &PaymentMethod) -> bool {
        self.kind.is_none_or(|kind| method.kind() == kind)
            && self
                .active
                .is_none_or(|active| method.is_active() == active)
            && self
                .search()
                .is_none_or(|text| method.name().to_lowercase().contains(&text.to_lowercase()))
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write.
pub trait PaymentMethodRepository: Send + Sync {
    /// Fails with `DuplicateName` when the company already has a payment method with that name.
    fn save(
        &self,
        method: &PaymentMethod,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The payment method, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> impl Future<Output = Result<Option<PaymentMethod>, RepositoryError>> + Send;

    /// Replaces the editable fields of a stored payment method. `NotFound` when the company has
    /// no such one, `DuplicateName` when another has the name.
    fn update(
        &self,
        method: &PaymentMethod,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the payment method, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` payment methods of the company that match `query`, in its order, starting
    /// right after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PaymentMethodQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<PaymentMethod>, RepositoryError>> + Send;
}
