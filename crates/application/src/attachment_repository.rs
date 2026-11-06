use std::future::Future;

use conta_verde_domain::{Attachment, AttachmentId, AuditEntry, AuditRecord, CompanyId, EntryId};

use crate::RepositoryError;

/// Port for the records of the files attached to entries ([ADR 0036]); the bytes are in a `BlobStore`.
/// Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait AttachmentRepository: Send + Sync {
    /// Stores the attachment. The database refuses an entry of another company.
    fn save(
        &self,
        attachment: &Attachment,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Marks a stored attachment removed, with its audit entry, atomically. `NotFound` when the
    /// company has no such attachment or it is removed already.
    fn remove(
        &self,
        removed: &Attachment,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The attachment, removed or not, only if it belongs to the entry of the company.
    fn find(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: AttachmentId,
    ) -> impl Future<Output = Result<Option<Attachment>, RepositoryError>> + Send;

    /// What was recorded about the attachment, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: AttachmentId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// The attachments of the entry that are not removed, in the order they were attached. Empty for
    /// another company's entry.
    fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> impl Future<Output = Result<Vec<Attachment>, RepositoryError>> + Send;
}
