use std::future::Future;

use conta_verde_domain::{CompanyId, DocumentMail, DocumentMailId, DocumentMailKind};
use uuid::Uuid;

use crate::RepositoryError;

/// What asking to record a send answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reservation {
    /// The send is recorded: it may go out.
    Reserved,
    /// The same document went to the same address a moment ago.
    TooSoon,
    /// The company has e-mailed as many documents as it may over the last day.
    CapReached,
}

/// Port for the log of documents that were e-mailed ([ADR 0065]). The check and the record are **one atomic step**, so two
/// sends racing for the last place cannot both have it.
pub trait DocumentMailRepository: Send + Sync {
    /// Records `mail` unless the company's limits say no: the same document, to the same address, less than
    /// `cooldown_millis` before `mail.sent_at_millis`, or `cap` documents already sent over the `day_millis` before it.
    /// Nothing is stored when it says no.
    fn reserve(
        &self,
        mail: &DocumentMail,
        cooldown_millis: i64,
        day_millis: i64,
        cap: u32,
    ) -> impl Future<Output = Result<Reservation, RepositoryError>> + Send;

    /// Takes a record back: the send it stood for went nowhere (no mail server), so it must not use up a place.
    fn release(
        &self,
        company_id: CompanyId,
        id: DocumentMailId,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was sent of a document, newest first, at most `limit`.
    fn of_document(
        &self,
        company_id: CompanyId,
        kind: DocumentMailKind,
        document_id: Uuid,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<DocumentMail>, RepositoryError>> + Send;
}
