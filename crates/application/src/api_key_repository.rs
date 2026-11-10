use std::future::Future;

use conta_verde_domain::{ApiKey, ApiKeyId, AuditEntry, AuditRecord, CalendarDate, CompanyId};

use crate::RepositoryError;

/// Port to the keys of a company ([ADR 0087]). Only the digest of a secret is ever stored: the port takes it with the key
/// when it is made and gives the key back, never the digest, when it is found.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait ApiKeyRepository: Send + Sync {
    /// Stores a new key with the digest of its secret.
    fn save(
        &self,
        key: &ApiKey,
        digest: &str,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Stores the key that replaces `old` and the new state of `old` in one transaction, so that a rotation never
    /// leaves two keys where it meant one nor none.
    fn rotate(
        &self,
        new: &ApiKey,
        digest: &str,
        new_audit: &AuditEntry,
        old: &ApiKey,
        old_audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Writes the life of the key (its expiry and whether it is revoked). Nothing else of a key changes.
    fn update(
        &self,
        key: &ApiKey,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn find(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
    ) -> impl Future<Output = Result<Option<ApiKey>, RepositoryError>> + Send;

    /// The key whose secret has this digest, of whichever company: the secret is what says which.
    fn find_by_digest(
        &self,
        digest: &str,
    ) -> impl Future<Output = Result<Option<ApiKey>, RepositoryError>> + Send;

    /// Newest first. A company has at most [`conta_verde_domain::MAX_API_KEYS`], so there is no paging.
    fn list(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<ApiKey>, RepositoryError>> + Send;

    /// Says the key was used on `day`. Bookkeeping: no audit entry.
    fn record_use(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
        day: CalendarDate,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn history(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;
}
