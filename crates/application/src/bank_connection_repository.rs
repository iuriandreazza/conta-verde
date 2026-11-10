use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, BankConnection, BankConnectionId, CalendarDate, CompanyId,
};

use crate::RepositoryError;

/// Port to the bank connections of a company ([ADR 0089]). Writes of what the connection **is** (made, status) take the audit
/// entry that describes them and store both atomically ([ADR 0008]); what a pull leaves (the day, a failure) is bookkeeping.
pub trait BankConnectionRepository: Send + Sync {
    /// Stores a new connection. `DuplicateBankConnection` when the bank account has one that is not disconnected, or when the
    /// aggregator's account is held by one of any company.
    fn save(
        &self,
        connection: &BankConnection,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Writes the status (and why). `NotFound` when the company has no such connection.
    fn update_status(
        &self,
        connection: &BankConnection,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Says how a pull went: the day it reached and why it failed, if it did. No audit entry.
    fn record_sync(
        &self,
        connection: &BankConnection,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn find(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
    ) -> impl Future<Output = Result<Option<BankConnection>, RepositoryError>> + Send;

    /// Of any company: the worker is given an id by the queue and nothing else.
    fn find_by_id(
        &self,
        id: BankConnectionId,
    ) -> impl Future<Output = Result<Option<BankConnection>, RepositoryError>> + Send;

    /// Newest first. A company has at most [`conta_verde_domain::MAX_BANK_CONNECTIONS`], so there is no paging.
    fn list(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<BankConnection>, RepositoryError>> + Send;

    /// The connected ones, of every company, that have not pulled through `day` yet: never, or before it. Oldest pull first.
    fn due(
        &self,
        day: CalendarDate,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<BankConnection>, RepositoryError>> + Send;

    /// Whether a connection of any company that is not disconnected still uses the aggregator's `item_id`.
    fn item_is_live(
        &self,
        provider: &str,
        item_id: &str,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    fn history(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;
}
