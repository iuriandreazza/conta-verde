use std::future::Future;

use conta_verde_domain::{BlobId, CompanyId};

use crate::RepositoryError;

/// Where the bytes of attachments are kept ([ADR 0036]): a directory on disk, or any store of named
/// files. It knows nothing of entries or audit; the attachment record says whose a file is. Futures are
/// `Send` so use cases can run on a multi-threaded runtime.
pub trait BlobStore: Send + Sync {
    /// Stores the bytes under the company and the id (an attachment's, or a fiscal file's), replacing what was there.
    fn put(
        &self,
        company_id: CompanyId,
        id: BlobId,
        bytes: &[u8],
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The bytes, or `None` when this company has nothing stored under the id: another company's file
    /// is never reachable with a company's id.
    fn get(
        &self,
        company_id: CompanyId,
        id: BlobId,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, RepositoryError>> + Send;

    /// Drops the bytes. Nothing there is not an error, so a removal can be asked again.
    fn delete(
        &self,
        company_id: CompanyId,
        id: BlobId,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
}
