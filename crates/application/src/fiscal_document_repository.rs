use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CalendarDate, CompanyId, FiscalDocument, FiscalDocumentId,
    FiscalDocumentKind, FiscalDocumentStatus, FiscalEnvironment, FiscalEvent, FiscalEventId,
    FiscalFile, FiscalFileKind, FiscalOrigin, SaleId,
};

use crate::RepositoryError;

/// What a list of fiscal documents is narrowed by: a status, a sale. Newest first (by the moment of issue, then the id,
/// so the keyset cursor stays valid), which is the only order ([ADR 0076]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FiscalDocumentQuery {
    pub status: Option<FiscalDocumentStatus>,
    pub sale: Option<SaleId>,
}

/// Where the next page of a list of fiscal documents starts: after the document issued at this moment with this id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FiscalDocumentCursor {
    pub issued_at_unix: i64,
    pub id: FiscalDocumentId,
}

impl From<&FiscalDocument> for FiscalDocumentCursor {
    fn from(document: &FiscalDocument) -> Self {
        Self {
            issued_at_unix: document.issued_at_unix(),
            id: document.id(),
        }
    }
}

impl FiscalDocumentQuery {
    pub fn matches(&self, document: &FiscalDocument) -> bool {
        let FiscalOrigin::Sale(sale) = document.origin();
        self.status.is_none_or(|status| status == document.status())
            && self.sale.is_none_or(|wanted| wanted == sale)
    }
}

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

    /// At most `limit` documents of the company that match `query`, newest first, starting right after `after`.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &FiscalDocumentQuery,
        after: Option<&FiscalDocumentCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<FiscalDocument>, RepositoryError>> + Send;

    /// The live document of this origin and kind, if there is one.
    fn find_live(
        &self,
        company_id: CompanyId,
        origin: FiscalOrigin,
        kind: FiscalDocumentKind,
    ) -> impl Future<Output = Result<Option<FiscalDocument>, RepositoryError>> + Send;

    /// The documents of every company that are waiting for the authority (processing or contingency) and were issued
    /// at or after `since_unix`, oldest first, at most `limit`: what the poll asks about. It reaches every company, so
    /// it is for the background pass, never for a request.
    fn pending(
        &self,
        since_unix: i64,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<FiscalDocument>, RepositoryError>> + Send;

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

    /// The documents of the company issued from `from` through `to` (their issue day, both included) in the production
    /// environment that were authorized and are authorized or cancelled now, oldest first, at most `limit`: what the
    /// accountant's package holds ([ADR 0073]). A test document is not a fiscal document.
    fn issued_between(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<FiscalDocument>, RepositoryError>> + Send;

    /// Records an event **before** the gateway is asked, as `sent`. `FiscalDocumentChanged` when the document already
    /// has a letter in that place of its sequence (someone else took the number): the caller reads again.
    fn record_event(
        &self,
        event: &FiscalEvent,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Writes the answer to an event that is still `sent`, once: `NotFound` when the company has no such event,
    /// `FiscalDocumentChanged` when it was settled already. What was asked is never written again.
    fn settle_event(
        &self,
        event: &FiscalEvent,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the event, oldest first. Empty for another company's.
    fn event_history(
        &self,
        company_id: CompanyId,
        id: FiscalEventId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    fn find_event(
        &self,
        company_id: CompanyId,
        id: FiscalEventId,
    ) -> impl Future<Output = Result<Option<FiscalEvent>, RepositoryError>> + Send;

    /// The correction letters of a document, by their place in its sequence.
    fn corrections_of(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> impl Future<Output = Result<Vec<FiscalEvent>, RepositoryError>> + Send;

    /// The cancellation of the document that is waiting or was accepted, if there is one: a refused one does not count
    /// (the person may ask again).
    fn open_cancellation(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> impl Future<Output = Result<Option<FiscalEvent>, RepositoryError>> + Send;

    /// The correction letters and cancellations of every company that were sent and not answered, oldest first, at most
    /// `limit`: what the poll asks the gateway about when it answered "queued". Reaches every company: for the
    /// background pass only.
    fn pending_events(
        &self,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<FiscalEvent>, RepositoryError>> + Send;

    /// The invalidations of the company, newest first, at most `limit`.
    fn invalidations(
        &self,
        company_id: CompanyId,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<FiscalEvent>, RepositoryError>> + Send;

    /// Whether a document of the company, authorized or cancelled, holds a number of the range in this series and
    /// environment: a number in use is never invalidated.
    fn number_is_used(
        &self,
        company_id: CompanyId,
        environment: FiscalEnvironment,
        series: u16,
        first: u32,
        last: u32,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;
}
