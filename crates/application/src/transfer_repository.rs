use std::future::Future;

use conta_verde_domain::{
    AccountOpening, AuditEntry, AuditRecord, BankAccountId, CalendarDate, CompanyId, Decimal,
    Transfer, TransferId,
};

use crate::{DateCursor, RepositoryError};

impl From<&Transfer> for DateCursor {
    fn from(transfer: &Transfer) -> Self {
        Self {
            date: transfer.occurred_on(),
            id: transfer.id().as_uuid(),
        }
    }
}

/// What came into an account and what went out of it, over a span of days. Both are 0 or more, exact.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccountMovement {
    pub received: Decimal,
    pub paid: Decimal,
}

/// What moved on one day, in an account: both are 0 or more, exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayMovement {
    pub date: CalendarDate,
    pub received: Decimal,
    pub paid: Decimal,
}

/// How a list of transfers is narrowed, following ADR 0018: by an account on either side. They are
/// listed newest first, then by id; reversed ones are listed too, so that they can be seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferQuery {
    account: Option<BankAccountId>,
    occurred_between: Option<(CalendarDate, CalendarDate)>,
    unreversed: bool,
}

impl TransferQuery {
    /// Only the transfers that occurred from `from` through `until`, both included ([ADR 0071]).
    pub fn occurring_between(mut self, from: CalendarDate, until: CalendarDate) -> Self {
        self.occurred_between = Some((from, until));
        self
    }

    /// Leaves out the transfers that were reversed.
    pub fn unreversed(mut self) -> Self {
        self.unreversed = true;
        self
    }

    pub fn span(&self) -> Option<(CalendarDate, CalendarDate)> {
        self.occurred_between
    }

    pub fn only_unreversed(&self) -> bool {
        self.unreversed
    }

    pub fn with_account(mut self, account: BankAccountId) -> Self {
        self.account = Some(account);
        self
    }

    pub fn account(&self) -> Option<BankAccountId> {
        self.account
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, transfer: &Transfer) -> bool {
        self.account
            .is_none_or(|account| transfer.from() == account || transfer.to() == account)
            && self
                .occurred_between
                .is_none_or(|(from, until)| (from..=until).contains(&transfer.occurred_on()))
            && !(self.unreversed && transfer.is_reversed())
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait TransferRepository: Send + Sync {
    fn save(
        &self,
        transfer: &Transfer,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Marks a stored transfer reversed, with its audit entry, atomically. `NotFound` when the
    /// company has no such transfer or it is reversed already.
    fn reverse(
        &self,
        reversed: &Transfer,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The transfer, reversed or not, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: TransferId,
    ) -> impl Future<Output = Result<Option<Transfer>, RepositoryError>> + Send;

    /// At most `limit` transfers of the company that match `query`, newest first, starting right
    /// after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &TransferQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Transfer>, RepositoryError>> + Send;

    /// What was recorded about the transfer, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: TransferId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// The transfers that are not reversed into the account (`incoming`) or out of it, for exactly
    /// `amount`, on the days from `since` through `until`: what a bank statement line of that amount
    /// may be ([ADR 0034]). Oldest day first, then by id.
    fn transfer_candidates(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        incoming: bool,
        amount: Decimal,
        since: CalendarDate,
        until: CalendarDate,
    ) -> impl Future<Output = Result<Vec<Transfer>, RepositoryError>> + Send;

    /// What the transfers that are not reversed moved in the account on each day from `since` through
    /// `until` that had any, oldest day first: `received` what came into it, `paid` what went out.
    fn movement_by_day(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> impl Future<Output = Result<Vec<DayMovement>, RepositoryError>> + Send;

    /// What the transfers that are not reversed moved in the account, on the days from `since`
    /// (every day when `None`) up to and including `until`: `received` what came into it, `paid` what
    /// went out.
    fn movement_of_account(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: Option<CalendarDate>,
        until: CalendarDate,
    ) -> impl Future<Output = Result<AccountMovement, RepositoryError>> + Send;
}

/// Port for the opening balance of each account, one at most ([ADR 0031]).
pub trait OpeningRepository: Send + Sync {
    /// Stores the opening balance of the account, replacing the one it had, with the audit entry,
    /// atomically.
    fn set(
        &self,
        opening: &AccountOpening,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn find(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
    ) -> impl Future<Output = Result<Option<AccountOpening>, RepositoryError>> + Send;

    /// What was recorded about the opening balance of the account, oldest first. Empty for another
    /// company's.
    fn history(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// Every opening balance of the company.
    fn list_by_company(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<AccountOpening>, RepositoryError>> + Send;
}
