use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CompanyId, FiscalDocument, FiscalDocumentId, FiscalDocumentKind,
    FiscalDocumentStatus, FiscalFile, FiscalFileKind, FiscalOrigin,
};

use crate::RepositoryError;

/// Port for fiscal documents and the records of their files ([ADR 0073], [ADR 0076]).
///
/// Every write of a document takes the audit entry that describes it and stores both **atomically** ([ADR 0008]). A
/// document is never deleted, and once it is authorized its content is never written again: an adapter refuses it.
pub trait FiscalDocumentRepository: Send + Sync {
    /// Stores a new document. `FiscalDocumentAlreadyLive` when its sale already has a live document of the kind
    ///. Whether the sale is the company's and usable is the use case's to check (the database also refuses a sale of another company).
    fn create(
        &self,
        document: &FiscalDocument,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Writes a move of a stored document, only if it still has the status `expected`: `NotFound` when the company
    /// has no such document, `FiscalDocumentChanged` when it is in another status. A document that was authorized
    /// keeps its content: writing other content is a storage failure, never silently ignored.
    fn replace(
        &self,
        document: &FiscalDocument,
        expected: FiscalDocumentStatus,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The document, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> impl Future<Output = Result<Option<FiscalDocument>, RepositoryError>> + Send;

    /// The live document of this origin and kind, if there is one.
    fn find_live(
        &self,
        company_id: CompanyId,
        origin: FiscalOrigin,
        kind: FiscalDocumentKind,
    ) -> impl Future<Output = Result<Option<FiscalDocument>, RepositoryError>> + Send;

    /// What was recorded about the document, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// Records a file of a document: one per document and kind, never changed. `DuplicateFiscalFile` when the
    /// document already has one of the kind, `NotFound` when it is not the company's.
    fn save_file(
        &self,
        file: &FiscalFile,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The files of the document, in the order of their kinds. Empty for another company's.
    fn files_of(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> impl Future<Output = Result<Vec<FiscalFile>, RepositoryError>> + Send;

    /// The file of this kind, if the document has one.
    fn find_file(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
        kind: FiscalFileKind,
    ) -> impl Future<Output = Result<Option<FiscalFile>, RepositoryError>> + Send;
}
