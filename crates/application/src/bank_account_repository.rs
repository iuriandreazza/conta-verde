use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, BankAccount, BankAccountId, BankAccountKind, CompanyId,
};

use crate::list_query::clean_search;
use crate::{NameCursor, NameSort, RepositoryError};

impl From<&BankAccount> for NameCursor {
    fn from(account: &BankAccount) -> Self {
        Self {
            name: account.name().to_owned(),
            id: account.id().as_uuid(),
        }
    }
}

/// How a list of bank accounts is narrowed and ordered, following ADR 0018. Inactive ones are
/// listed unless asked otherwise, so that they can be found and brought back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BankAccountQuery {
    search: Option<String>,
    /// The search written the way account numbers are stored (upper case, no dots, hyphens or
    /// spaces), kept apart because it can be empty when the search is not.
    number_needle: Option<String>,
    kind: Option<BankAccountKind>,
    active: Option<bool>,
    sort: NameSort,
}

impl BankAccountQuery {
    /// Matches a name anywhere in it, ignoring case, or an account number ignoring its
    /// punctuation. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        let needle: String = self
            .search()
            .unwrap_or_default()
            .chars()
            .filter(|c| !matches!(c, '.' | '-') && !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        self.number_needle = (!needle.is_empty()).then_some(needle);
        self
    }

    pub fn of_kind(mut self, kind: BankAccountKind) -> Self {
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

    /// What to look for in account numbers, or `None` when nothing a number could hold is left.
    pub fn number_search(&self) -> Option<&str> {
        self.number_needle.as_deref()
    }

    pub fn kind(&self) -> Option<BankAccountKind> {
        self.kind
    }

    pub fn active(&self) -> Option<bool> {
        self.active
    }

    pub fn sort(&self) -> NameSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, account: &BankAccount) -> bool {
        self.kind.is_none_or(|kind| account.kind() == kind)
            && self
                .active
                .is_none_or(|active| account.is_active() == active)
            && self.search().is_none_or(|text| {
                account.name().to_lowercase().contains(&text.to_lowercase())
                    || self.number_search().is_some_and(|needle| {
                        account
                            .number()
                            .is_some_and(|number| number.contains(needle))
                    })
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write.
pub trait BankAccountRepository: Send + Sync {
    /// Fails with `DuplicateName` when the company has an account with that name, and with
    /// `DuplicateBankAccount` when it has one with that bank, branch and number.
    fn save(
        &self,
        account: &BankAccount,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The account, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> impl Future<Output = Result<Option<BankAccount>, RepositoryError>> + Send;

    /// Replaces the editable fields of a stored account, never its kind. `NotFound` when the
    /// company has no such account of that kind, `DuplicateName` and `DuplicateBankAccount` as
    /// for `save`.
    fn update(
        &self,
        account: &BankAccount,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the account, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` accounts of the company that match `query`, in its order, starting right
    /// after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &BankAccountQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<BankAccount>, RepositoryError>> + Send;
}
