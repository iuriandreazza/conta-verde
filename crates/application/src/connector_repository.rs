use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, Connector, ConnectorId};

use crate::RepositoryError;

/// Port to the connectors of a company ([ADR 0090]). What a connector **is** (made, on, off) is written with the audit entry that
/// describes it, atomically ([ADR 0008]).
pub trait ConnectorRepository: Send + Sync {
    /// Stores a new connector. `DuplicateName` when the company has one with this name, ignoring case.
    fn save(
        &self,
        connector: &Connector,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Writes the key and the subscription of a connector and whether it is on, **only while it is still as the caller read it**
    /// (`was_active`): `ConnectorChanged` otherwise, so that two people turning it on at once make one winner. `NotFound` when the
    /// company has no such connector.
    fn switch(
        &self,
        connector: &Connector,
        was_active: bool,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn find(
        &self,
        company_id: CompanyId,
        id: ConnectorId,
    ) -> impl Future<Output = Result<Option<Connector>, RepositoryError>> + Send;

    /// Newest first. A company has at most [`conta_verde_domain::MAX_CONNECTORS`], so there is no paging.
    fn list(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<Connector>, RepositoryError>> + Send;

    fn history(
        &self,
        company_id: CompanyId,
        id: ConnectorId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;
}
