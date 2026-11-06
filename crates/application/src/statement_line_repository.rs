use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, BankAccountId, CompanyId, LineStatus, SettlementId, StatementLine,
    StatementLineId, TransferId,
};

use crate::{DateCursor, RepositoryError};

impl From<&StatementLine> for DateCursor {
    fn from(line: &StatementLine) -> Self {
        Self {
            date: line.posted_on(),
            id: line.id().as_uuid(),
        }
    }
}

/// How a list of statement lines is narrowed, following ADR 0018: by account and by status. They are
/// listed newest day first, then by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatementLineQuery {
    account: Option<BankAccountId>,
    status: Option<LineStatus>,
}

impl StatementLineQuery {
    pub fn of_account(mut self, account: BankAccountId) -> Self {
        self.account = Some(account);
        self
    }

    pub fn with_status(mut self, status: LineStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn account(&self) -> Option<BankAccountId> {
        self.account
    }

    pub fn status(&self) -> Option<LineStatus> {
        self.status
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, line: &StatementLine) -> bool {
        self.account
            .is_none_or(|account| line.account_id() == account)
            && self.status.is_none_or(|status| line.status() == status)
    }
}

/// The settlements and transfers, out of some asked about, that a line is matched to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchedTargets {
    pub settlements: Vec<SettlementId>,
    pub transfers: Vec<TransferId>,
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait StatementLineRepository: Send + Sync {
    /// Fails with `DuplicateLine` when the account already has a line with the same fingerprint.
    fn save(
        &self,
        line: &StatementLine,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The line, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> impl Future<Output = Result<Option<StatementLine>, RepositoryError>> + Send;

    /// Stores the status and what the stored line is matched to. `NotFound` when the company has no
    /// such line, and `AlreadyMatched` when another line is matched to the settlement or the transfer.
    fn update(
        &self,
        line: &StatementLine,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the line, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` lines of the company that match `query`, newest day first, starting right
    /// after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &StatementLineQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<StatementLine>, RepositoryError>> + Send;

    /// Which of these settlements and transfers of the company have a line matched to them.
    fn matched_among(
        &self,
        company_id: CompanyId,
        settlements: &[SettlementId],
        transfers: &[TransferId],
    ) -> impl Future<Output = Result<MatchedTargets, RepositoryError>> + Send;
}
