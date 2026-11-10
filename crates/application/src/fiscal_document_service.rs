use conta_verde_domain::{
    Actor, Amount, AuditAction, AuditRecord, BlobId, CalendarDate, CompanyId, FiscalAuthorization,
    FiscalDocument, FiscalDocumentDraft, FiscalDocumentId, FiscalDocumentKind,
    FiscalDocumentStatus, FiscalEvent, FiscalEventId, FiscalEventStatus, FiscalEventSubject,
    FiscalFile, FiscalFileKind, FiscalOrigin, FiscalRefusal, FiscalSettings,
    MAX_CORRECTION_ATTEMPTS, MAX_CORRECTIONS, NatureKind, Party, Product, ProductKind, Sale,
    SaleId, SaleStatus, TaxRegime, fiscal_day, fiscal_document_changes,
    fiscal_document_creation_changes, fiscal_event_changes, fiscal_event_creation_changes,
};

use crate::accounting_export_service::{
    ExportCell, ExportFile, ExportLimits, ExportTable, fiscal_table,
};
use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, AuthorizedDocument, BlobStore, BuyerPresence, Destination, DocumentOutcome,
    EntryReferences, EventOutcome, EventQuery, FiscalDocumentCursor, FiscalDocumentQuery,
    FiscalDocumentRepository, FiscalGateway, FiscalSettingsRepository, GatewayError, NfeItem,
    NfeRecipient, NfeRequest, PartyRepository, PaymentForm, ProductRepository, RepositoryError,
    SaleRepository,
};

const MIN_REASON_CHARS: usize = 15;
const MAX_CANCEL_REASON_CHARS: usize = 255;
/// How long a document is given before "the gateway has no document with this reference" is believed: a submission
/// that is still on its way would otherwise be refused under the feet of the call that is making it.
const UNKNOWN_GRACE_SECONDS: i64 = 10 * 60;

/// A page of fiscal documents and where the next one starts.
#[derive(Debug, Clone)]
pub struct FiscalDocumentPage {
    pub items: Vec<FiscalDocument>,
    pub next: Option<FiscalDocumentCursor>,
}

/// What one pass of the poll did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PollReport {
    pub asked: usize,
    /// No longer waiting after the answer (authorized, rejected, denied or cancelled).
    pub settled: usize,
    pub failed: usize,
}

/// What the person chooses to issue an NF-e from a completed sale. **Every field is a tax fact the product does not
/// know**, so none has a default ([ADR 0076]): which of the company's operation natures, the origin of the goods
/// (0 to 8, one for the whole document), how the buyer was present, how it was paid and whether the buyer is the
/// final consumer.
#[derive(Debug, Clone)]
pub struct IssueNfe {
    pub sale_id: SaleId,
    pub nature_code: String,
    pub origin: u8,
    pub presence: BuyerPresence,
    pub payment: PaymentForm,
    pub final_consumer: bool,
}

/// Issues NF-e documents from sales and follows them to the authority through a gateway ([ADR 0073], [ADR 0076]).
///
/// Every step can be repeated: asking again finishes what a failure left. The moment is the caller's, in Unix seconds,
/// so a test moves it; the server's clock is read only to make it.
pub struct FiscalDocumentService<D, S, P, Y, F, G, B, R> {
    documents: D,
    sales: S,
    parties: P,
    products: Y,
    settings: F,
    gateway: G,
    blobs: B,
    references: R,
}

impl<D, S, P, Y, F, G, B, R> FiscalDocumentService<D, S, P, Y, F, G, B, R>
where
    D: FiscalDocumentRepository,
    S: SaleRepository,
    P: PartyRepository,
    Y: ProductRepository,
    F: FiscalSettingsRepository,
    G: FiscalGateway,
    B: BlobStore,
    R: EntryReferences,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        documents: D,
        sales: S,
        parties: P,
        products: Y,
        settings: F,
        gateway: G,
        blobs: B,
        references: R,
    ) -> Self {
        Self {
            documents,
            sales,
            parties,
            products,
            settings,
            gateway,
            blobs,
            references,
        }
    }

    /// The gateway the service asks, for what it says about itself.
    pub fn gateway(&self) -> &G {
        &self.gateway
    }

    /// Asks the gateway about every document that is still waiting and was issued at or after `since_unix`, at most
    /// `limit` of them, oldest first: the poll that finishes what a gateway answered "processing" to. Every step is
    /// harmless to repeat, so a pass may overlap another; a document the gateway cannot answer for now is left for the
    /// next pass and the others go on. It reaches every company.
    pub async fn poll_pending(
        &self,
        since_unix: i64,
        limit: usize,
        now_unix: i64,
    ) -> Result<PollReport, ApplicationError> {
        let mut report = PollReport::default();
        for document in self.documents.pending(since_unix, limit).await? {
            report.asked += 1;
            match self
                .refresh(document.company_id(), document.id(), now_unix)
                .await
            {
                Ok(refreshed) if !refreshed.status().is_pending() => report.settled += 1,
                Ok(_) => {}
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(%error, "a fiscal document could not be refreshed");
                }
            }
        }
        let (asked, settled) = self.settle_queued_events(limit).await?;
        report.asked += asked;
        report.settled += settled;
        Ok(report)
    }

    /// The company's documents, newest first, a page at a time.
    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &FiscalDocumentQuery,
        after: Option<&FiscalDocumentCursor>,
        limit: Option<usize>,
    ) -> Result<FiscalDocumentPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .documents
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(FiscalDocumentPage { items, next })
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<FiscalDocument, ApplicationError> {
        self.documents
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::FiscalDocumentNotFound)
    }

    /// What was recorded about the document, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        self.get(company_id, id).await?;
        Ok(self.documents.history(company_id, id).await?)
    }

    /// The kinds of file the document has.
    pub async fn files(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<FiscalFile>, ApplicationError> {
        self.get(company_id, id).await?;
        Ok(self.documents.files_of(company_id, id).await?)
    }

    /// The bytes of a file of the document.
    pub async fn file(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
        kind: FiscalFileKind,
    ) -> Result<Vec<u8>, ApplicationError> {
        let file = self
            .documents
            .find_file(company_id, id, kind)
            .await?
            .ok_or(ApplicationError::FiscalDocumentNotFound)?;
        self.blobs
            .get(company_id, file.blob_id())
            .await?
            .ok_or_else(|| {
                ApplicationError::Repository(RepositoryError::Storage(
                    "a fiscal file's bytes are missing".to_owned(),
                ))
            })
    }

    /// Issues an NF-e from a completed sale, or finishes the one the sale already has: a live document (draft,
    /// processing, contingency or authorized) is the answer to asking twice. `now_unix` is the moment of issue.
    pub async fn issue(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        request: IssueNfe,
        now_unix: i64,
    ) -> Result<FiscalDocument, ApplicationError> {
        let settings = self.ready_settings(company_id).await?;
        let sale = self.completed_sale(company_id, request.sale_id).await?;
        let origin = FiscalOrigin::Sale(sale.id());
        let live = self
            .documents
            .find_live(company_id, origin, FiscalDocumentKind::Nfe)
            .await?;
        // What is past the draft is the answer to asking twice, whatever this request says and whatever has closed since.
        if let Some(live) = live
            .as_ref()
            .filter(|live| live.status() != FiscalDocumentStatus::Draft)
        {
            return self
                .refresh_or_return(company_id, live.clone(), now_unix)
                .await;
        }
        let issued_on = issue_day(&settings, now_unix)?;
        ensure_certificate(&settings, issued_on)?;
        self.references.ensure_open(company_id, issued_on).await?;
        // A draft a failure left keeps its id, which is the gateway's reference.
        let id = live
            .as_ref()
            .map_or_else(FiscalDocumentId::generate, FiscalDocument::id);
        let (draft, nfe) = self
            .build(
                company_id, actor, &settings, &sale, &request, issued_on, now_unix, id,
            )
            .await?;
        let document = match live {
            Some(stored) => {
                let rebuilt = stored.rebuilt_with(draft)?;
                let audit = audit_entry(
                    &rebuilt,
                    actor,
                    AuditAction::Updated,
                    fiscal_document_changes(&stored, &rebuilt),
                );
                self.documents
                    .replace(&rebuilt, FiscalDocumentStatus::Draft, &audit)
                    .await?;
                rebuilt
            }
            None => {
                let document = FiscalDocument::new(id, company_id, draft)?;
                let audit = audit_entry(
                    &document,
                    actor,
                    AuditAction::Created,
                    fiscal_document_creation_changes(&document),
                );
                match self.documents.create(&document, &audit).await {
                    Ok(()) => document,
                    // Someone else made it between the look and the write: theirs is the answer.
                    Err(RepositoryError::FiscalDocumentAlreadyLive) => {
                        let live = self
                            .documents
                            .find_live(company_id, origin, FiscalDocumentKind::Nfe)
                            .await?
                            .ok_or(RepositoryError::FiscalDocumentAlreadyLive)?;
                        return self.refresh_or_return(company_id, live, now_unix).await;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };
        self.send(company_id, actor, document, &nfe).await
    }

    /// Sends a rejected document again under the same id, built again from the sale as it is now: the person fixed
    /// what the authority refused (the sale, a product, the settings) first.
    pub async fn resend(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: FiscalDocumentId,
        request: IssueNfe,
        now_unix: i64,
    ) -> Result<FiscalDocument, ApplicationError> {
        let stored = self.get(company_id, id).await?;
        if stored.status() != FiscalDocumentStatus::Rejected {
            return Err(conta_verde_domain::DomainError::InvalidFiscalTransition.into());
        }
        if stored.origin() != FiscalOrigin::Sale(request.sale_id) {
            return Err(ApplicationError::FiscalRule(
                "the document was made from another sale".to_owned(),
            ));
        }
        let settings = self.ready_settings(company_id).await?;
        let issued_on = issue_day(&settings, now_unix)?;
        ensure_certificate(&settings, issued_on)?;
        self.references.ensure_open(company_id, issued_on).await?;
        let sale = self.completed_sale(company_id, request.sale_id).await?;
        let (draft, nfe) = self
            .build(
                company_id, actor, &settings, &sale, &request, issued_on, now_unix, id,
            )
            .await?;
        let rebuilt = stored.rebuilt_with(draft)?;
        let audit = audit_entry(
            &rebuilt,
            actor,
            AuditAction::Updated,
            fiscal_document_changes(&stored, &rebuilt),
        );
        self.documents
            .replace(&rebuilt, FiscalDocumentStatus::Rejected, &audit)
            .await?;
        self.send(company_id, actor, rebuilt, &nfe).await
    }

    /// Asks the gateway about a document that is still waiting and records the answer. What the poll does too, in the
    /// name of whoever submitted the document. A document that is not waiting is returned as it is.
    pub async fn refresh(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
        now_unix: i64,
    ) -> Result<FiscalDocument, ApplicationError> {
        let document = self.get(company_id, id).await?;
        if !document.status().is_pending() {
            return Ok(document);
        }
        let settings = self.settings_of(company_id, &document).await?;
        let actor = submitter(&document);
        let outcome = self
            .gateway
            .query_nfe(&settings, &reference(&document))
            .await?;
        // "No such document" is believed only once the submission has had time to arrive.
        let unknown_is_final = now_unix - document.issued_at_unix() >= UNKNOWN_GRACE_SECONDS;
        self.apply(company_id, &actor, document, outcome, unknown_is_final)
            .await
    }

    /// Cancels an authorized NF-e with a reason of 15 to 255 characters. The deadline is the authority's. It asks the
    /// gateway for the state first (a document it already cancelled is followed, not cancelled twice) and is refused
    /// for a month that is closed ([ADR 0073]).
    pub async fn cancel(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: FiscalDocumentId,
        reason: &str,
    ) -> Result<FiscalDocument, ApplicationError> {
        let reason = reason.trim();
        let length = reason.chars().count();
        if !(MIN_REASON_CHARS..=MAX_CANCEL_REASON_CHARS).contains(&length)
            || reason.chars().any(char::is_control)
        {
            return Err(ApplicationError::FiscalRule(format!(
                "the reason of a cancellation has {MIN_REASON_CHARS} to {MAX_CANCEL_REASON_CHARS} characters, on one line"
            )));
        }
        let document = self.get(company_id, id).await?;
        if document.status() != FiscalDocumentStatus::Authorized {
            return Err(conta_verde_domain::DomainError::InvalidFiscalTransition.into());
        }
        self.references
            .ensure_open(company_id, document.issued_on())
            .await?;
        let settings = self.settings_of(company_id, &document).await?;
        let state = self
            .gateway
            .query_nfe(&settings, &reference(&document))
            .await?;
        if let DocumentOutcome::Cancelled { .. } = state {
            let cancelled = self.apply(company_id, actor, document, state, true).await?;
            self.accept_open_cancellation(company_id, id).await?;
            return Ok(cancelled);
        }
        // One cancellation at a time: a refused one does not count, so the person may ask again.
        if self
            .documents
            .open_cancellation(company_id, id)
            .await?
            .is_some()
        {
            return Err(ApplicationError::FiscalRule(
                "a cancellation of this document is already waiting for an answer or was accepted"
                    .to_owned(),
            ));
        }
        // Recorded before the gateway is asked, like the other events: an answer that is lost, or that comes later, is
        // found by the poll and the cancellation is never sent twice ([ADR 0078]).
        let subject = FiscalEventSubject::cancellation(id, reason)?;
        let event = self.record_event(company_id, actor, subject).await?;
        let answer = self
            .gateway
            .cancel_nfe(&settings, &reference(&document), reason)
            .await;
        self.settle_event(company_id, actor, event, answer, "cancellation")
            .await?;
        self.get(company_id, id).await
    }

    /// Sends a correction letter (CC-e) for an authorized document. The letter is **recorded as sent before the gateway
    /// is asked**, so an answer that was lost leaves it visible and it is never sent again by the product; a new one
    /// waits until it is settled. The authority's deadline (720 hours) and rules are its own to enforce, and a refusal
    /// is kept and told. A letter changes no figure, so a closed month does not refuse it ([ADR 0073]).
    pub async fn correct(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: FiscalDocumentId,
        text: &str,
    ) -> Result<FiscalEvent, ApplicationError> {
        let document = self.get(company_id, id).await?;
        if document.status() != FiscalDocumentStatus::Authorized {
            return Err(conta_verde_domain::DomainError::InvalidFiscalTransition.into());
        }
        let settings = self.settings_of(company_id, &document).await?;
        let letters = self.documents.corrections_of(company_id, id).await?;
        if letters
            .iter()
            .any(|letter| letter.status() == FiscalEventStatus::Sent)
        {
            return Err(ApplicationError::FiscalRule(
                "a correction letter of this document is still waiting for an answer".to_owned(),
            ));
        }
        // The authority's limit is on the letters it accepted; our place is one more than the attempts so far.
        let accepted = letters
            .iter()
            .filter(|letter| letter.status() == FiscalEventStatus::Accepted)
            .count();
        if accepted >= usize::from(MAX_CORRECTIONS) {
            return Err(ApplicationError::FiscalRule(format!(
                "a document takes at most {MAX_CORRECTIONS} correction letters"
            )));
        }
        let sequence = u8::try_from(letters.len() + 1)
            .ok()
            .filter(|sequence| *sequence <= MAX_CORRECTION_ATTEMPTS)
            .ok_or_else(|| {
                ApplicationError::FiscalRule(
                    "too many attempts at a correction letter were recorded for this document"
                        .to_owned(),
                )
            })?;
        let subject = FiscalEventSubject::correction(id, sequence, text).map_err(|_| {
            ApplicationError::FiscalRule(
                "the correction has 15 to 1000 characters, on one line".to_owned(),
            )
        })?;
        let event = self.record_event(company_id, actor, subject).await?;
        let answer = self
            .gateway
            .correct_nfe(&settings, &reference(&document), event.subject().body())
            .await;
        self.settle_event(company_id, actor, event, answer, "correction letter")
            .await
    }

    /// Declares a range of numbers of a series unused (inutilização), in the environment the company issues in now.
    /// Recorded as sent before the gateway is asked. A number a document of the company holds is never invalidated.
    pub async fn invalidate(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        series: u16,
        first: u32,
        last: u32,
        reason: &str,
    ) -> Result<FiscalEvent, ApplicationError> {
        let settings = self.ready_settings(company_id).await?;
        let subject =
            FiscalEventSubject::invalidation(series, first, last, settings.environment(), reason)
                .map_err(|_| {
                    ApplicationError::FiscalRule(
                        "an invalidation has a series from 1 to 999, a range of numbers and a reason of 15 to 255 characters, on one line"
                            .to_owned(),
                    )
                })?;
        if self
            .documents
            .number_is_used(company_id, settings.environment(), series, first, last)
            .await?
        {
            return Err(ApplicationError::FiscalRule(
                "a document of this company holds a number of the range: a number in use is not invalidated".to_owned(),
            ));
        }
        let event = self.record_event(company_id, actor, subject).await?;
        let answer = self
            .gateway
            .invalidate_nfe_numbers(&settings, series, first, last, reason.trim())
            .await;
        self.settle_event(company_id, actor, event, answer, "invalidation")
            .await
    }

    /// The fiscal documents issued from `from` through `to` in production, as the accountant's package takes them
    /// ([ADR 0073], [ADR 0076]): one row each, and the files under `fiscal/nfe/`: the XML, the DANFE, the cancellation's
    /// XML and each correction letter's. `limits` is what the package has left (rows, files, bytes), checked from the
    /// records **before any file is read**: a span that does not fit is refused, never cut.
    pub async fn package(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
        limits: ExportLimits,
    ) -> Result<(ExportTable, Vec<ExportFile>), ApplicationError> {
        let documents = self
            .documents
            .issued_between(company_id, from, to, limits.rows + 1)
            .await?;
        if documents.len() > limits.rows {
            return Err(ApplicationError::ExportTooLarge(
                "too many fiscal documents",
            ));
        }
        // What each document brings, by the records: the files are read only when they are known to fit.
        let mut planned: Vec<(String, BlobId, u64)> = Vec::new();
        let mut rows = Vec::with_capacity(documents.len());
        for document in &documents {
            let Some(authorization) = document.authorization() else {
                continue;
            };
            let key = authorization.access_key().as_str();
            let base = format!("fiscal/{}/{key}", document.kind().key());
            let files = self.documents.files_of(company_id, document.id()).await?;
            let mut path_of = |kind: FiscalFileKind, suffix: &str| -> String {
                files
                    .iter()
                    .find(|file| file.kind() == kind)
                    .map(|file| {
                        let path = format!("{base}{suffix}");
                        planned.push((path.clone(), file.blob_id(), file.size()));
                        path
                    })
                    .unwrap_or_default()
            };
            let xml = path_of(FiscalFileKind::Xml, ".xml");
            let pdf = path_of(FiscalFileKind::Pdf, ".pdf");
            let cancellation = path_of(FiscalFileKind::CancellationXml, "-cancel.xml");
            for letter in self
                .documents
                .corrections_of(company_id, document.id())
                .await?
            {
                if let (FiscalEventSubject::Correction { sequence, .. }, Some((blob, size))) =
                    (letter.subject(), letter.xml())
                {
                    planned.push((format!("{base}-cce-{sequence}.xml"), blob, size));
                }
            }
            let text = |value: String| ExportCell::Text(value);
            rows.push(vec![
                text(document.id().as_uuid().to_string()),
                text(document.kind().key().to_owned()),
                text(document.status().key().to_owned()),
                ExportCell::Amount(authorization.number().to_string()),
                ExportCell::Amount(authorization.series().to_string()),
                text(key.to_owned()),
                text(document.issued_on().to_string()),
                text(document.recipient_name().to_owned()),
                text(document.recipient_document().unwrap_or_default().to_owned()),
                ExportCell::Amount(document.total().as_decimal().to_string()),
                text(document.environment().key().to_owned()),
                text(document.nature_code().to_owned()),
                text(xml),
                text(pdf),
                text(cancellation),
            ]);
            // Refused as soon as the records pass a limit, so a span that cannot fit costs no more queries.
            if planned.len() > limits.attached_files
                || planned.iter().map(|(_, _, size)| *size).sum::<u64>() > limits.attached_bytes
            {
                break;
            }
        }
        if planned.len() > limits.attached_files {
            return Err(ApplicationError::ExportTooLarge(
                "more than 500 files with the fiscal documents",
            ));
        }
        if planned.iter().map(|(_, _, size)| *size).sum::<u64>() > limits.attached_bytes {
            return Err(ApplicationError::ExportTooLarge(
                "more than 200 MiB of files with the fiscal documents",
            ));
        }
        let mut files = Vec::with_capacity(planned.len());
        for (path, blob, _) in planned {
            let bytes = self.blobs.get(company_id, blob).await?.ok_or_else(|| {
                ApplicationError::Repository(RepositoryError::Storage(format!(
                    "the fiscal file {path} is missing"
                )))
            })?;
            files.push(ExportFile { path, bytes });
        }
        Ok((fiscal_table(rows), files))
    }

    /// Gives up on an event that was sent and never answered: the person has checked at the gateway that the authority did
    /// not take it, and says so. It is settled as refused with the words that say who decided, which frees a new letter.
    /// **Never for an event the authority may have accepted**: the product cannot tell, and the answer is the gateway's.
    pub async fn abandon_event(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: FiscalEventId,
    ) -> Result<FiscalEvent, ApplicationError> {
        let event = self
            .documents
            .find_event(company_id, id)
            .await?
            .ok_or(ApplicationError::FiscalDocumentNotFound)?;
        if event.status() != FiscalEventStatus::Sent {
            return Err(conta_verde_domain::DomainError::InvalidFiscalTransition.into());
        }
        let refused = event.refused(FiscalRefusal::new(
            "abandoned",
            "the answer never came and the person confirmed at the gateway that the authority did not take it",
        ))?;
        self.write_settlement(actor, &event, &refused).await?;
        Ok(refused)
    }

    /// The cancellation of the document that is waiting for the authority's answer or was accepted, if there is one.
    pub async fn open_cancellation(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Option<FiscalEvent>, ApplicationError> {
        Ok(self.documents.open_cancellation(company_id, id).await?)
    }

    /// The correction letters of a document.
    pub async fn corrections(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<FiscalEvent>, ApplicationError> {
        self.get(company_id, id).await?;
        Ok(self.documents.corrections_of(company_id, id).await?)
    }

    /// The invalidations of the company, newest first.
    pub async fn invalidations(
        &self,
        company_id: CompanyId,
        limit: usize,
    ) -> Result<Vec<FiscalEvent>, ApplicationError> {
        Ok(self.documents.invalidations(company_id, limit).await?)
    }

    /// The XML of an event the authority accepted, when the gateway gave one.
    pub async fn event_file(
        &self,
        company_id: CompanyId,
        id: FiscalEventId,
    ) -> Result<Vec<u8>, ApplicationError> {
        let event = self
            .documents
            .find_event(company_id, id)
            .await?
            .ok_or(ApplicationError::FiscalDocumentNotFound)?;
        let (blob, _) = event
            .xml()
            .ok_or(ApplicationError::FiscalDocumentNotFound)?;
        self.blobs.get(company_id, blob).await?.ok_or_else(|| {
            ApplicationError::Repository(RepositoryError::Storage(
                "a fiscal file's bytes are missing".to_owned(),
            ))
        })
    }

    async fn record_event(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        subject: FiscalEventSubject,
    ) -> Result<FiscalEvent, ApplicationError> {
        let Actor::User { id: user, name } = actor;
        let event = FiscalEvent::new(FiscalEventId::generate(), company_id, subject, *user, name)?;
        let audit = audit_entry(
            &event,
            actor,
            AuditAction::Created,
            fiscal_event_creation_changes(&event),
        );
        self.documents.record_event(&event, &audit).await?;
        Ok(event)
    }

    /// Settles a recorded event with what the gateway said. An answer that never came leaves it `sent`.
    async fn settle_event(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        event: FiscalEvent,
        answer: Result<EventOutcome, GatewayError>,
        what: &str,
    ) -> Result<FiscalEvent, ApplicationError> {
        match answer {
            Ok(EventOutcome::Accepted(accepted)) => {
                let is_cancellation =
                    matches!(event.subject(), FiscalEventSubject::Cancellation { .. });
                let xml = match accepted.xml {
                    // A cancellation's XML belongs to the document, like its authorization's.
                    Some(bytes) if !bytes.is_empty() && is_cancellation => {
                        if let FiscalEventSubject::Cancellation { document, .. } = event.subject() {
                            self.store_file(
                                company_id,
                                *document,
                                FiscalFileKind::CancellationXml,
                                &bytes,
                            )
                            .await?;
                        }
                        None
                    }
                    Some(bytes) if !bytes.is_empty() => {
                        let blob = BlobId::generate();
                        self.blobs.put(company_id, blob, &bytes).await?;
                        Some((blob, bytes.len() as u64))
                    }
                    _ => None,
                };
                let settled = event.accepted(accepted.protocol.as_deref(), xml)?;
                // The document first: if this fails the event is still `sent` and the poll finishes it; the other way
                // round would leave an accepted cancellation over a document nothing would ever move.
                if let FiscalEventSubject::Cancellation { document, .. } = event.subject() {
                    self.finish_cancellation(company_id, actor, *document)
                        .await?;
                }
                self.write_settlement(actor, &event, &settled).await?;
                Ok(settled)
            }
            Ok(EventOutcome::Refused(refusal)) => {
                let refused = event.refused(FiscalRefusal::new(&refusal.code, &refusal.message))?;
                self.write_settlement(actor, &event, &refused).await?;
                Err(ApplicationError::FiscalRule(format!(
                    "the authority refused the {what} ({}): {}",
                    refusal.code, refusal.message
                )))
            }
            // The gateway took it and will do it later: the event stays `sent`, and the poll asks again.
            Ok(EventOutcome::Queued) => Ok(event),
            // The call may have reached the authority: the event stays `sent` and is never sent again.
            Err(error @ GatewayError::Unavailable(_)) => Err(error.into()),
            Err(error) => {
                let refused = event.refused(FiscalRefusal::new("gateway", &error.to_string()))?;
                self.write_settlement(actor, &event, &refused).await?;
                Err(error.into())
            }
        }
    }

    /// The gateway says the document is cancelled: a cancellation of ours that was still waiting is the one that was done.
    async fn accept_open_cancellation(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<(), ApplicationError> {
        if let Some(open) = self.documents.open_cancellation(company_id, id).await?
            && open.status() == FiscalEventStatus::Sent
        {
            let actor = Actor::User {
                id: open.requested_by(),
                name: open.requested_by_name().to_owned(),
            };
            let accepted = open.accepted(None, None)?;
            self.write_settlement(&actor, &open, &accepted).await?;
        }
        Ok(())
    }

    /// The authority accepted the cancellation: the document is cancelled, in the name of whoever asked. A document that
    /// is no longer authorized (the poll got there first) is left as it is.
    async fn finish_cancellation(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: FiscalDocumentId,
    ) -> Result<(), ApplicationError> {
        let document = self.get(company_id, id).await?;
        if document.status() == FiscalDocumentStatus::Authorized {
            self.move_to(actor, document, |document| document.cancelled())
                .await?;
        }
        Ok(())
    }

    /// Asks the gateway about the correction letters and cancellations it answered "queued" to and records what it says,
    /// in the name of whoever asked. Returns how many were asked and how many are no longer waiting. A gateway that
    /// never queues has none to ask about.
    pub async fn settle_queued_events(
        &self,
        limit: usize,
    ) -> Result<(usize, usize), ApplicationError> {
        let mut asked = 0;
        let mut settled = 0;
        for event in self.documents.pending_events(limit).await? {
            asked += 1;
            let company_id = event.company_id();
            let (document_id, query) = match event.subject() {
                FiscalEventSubject::Correction { document, .. } => {
                    let before = self
                        .documents
                        .corrections_of(company_id, *document)
                        .await?
                        .iter()
                        .filter(|letter| letter.status() == FiscalEventStatus::Accepted)
                        .count();
                    (
                        *document,
                        EventQuery::Correction {
                            accepted_before: u8::try_from(before).unwrap_or(u8::MAX),
                        },
                    )
                }
                FiscalEventSubject::Cancellation { document, .. } => {
                    (*document, EventQuery::Cancellation)
                }
                FiscalEventSubject::Invalidation { .. } => continue,
            };
            let outcome = async {
                let document = self.get(company_id, document_id).await?;
                let settings = self.settings_of(company_id, &document).await?;
                let actor = Actor::User {
                    id: event.requested_by(),
                    name: event.requested_by_name().to_owned(),
                };
                // Only an answer settles an event: a gateway that cannot be reached, or that does not answer events
                // later (Focus NFe answers them at once), leaves it as it is, never refused for the lack of an answer.
                let answer = match self
                    .gateway
                    .query_event(&settings, &reference(&document), query)
                    .await
                {
                    Ok(answer @ (EventOutcome::Accepted(_) | EventOutcome::Refused(_))) => answer,
                    Ok(EventOutcome::Queued) | Err(_) => return Ok(false),
                };
                match self
                    .settle_event(company_id, &actor, event.clone(), Ok(answer), "event")
                    .await
                {
                    // The authority's refusal is an answer: the event is settled.
                    Ok(_) | Err(ApplicationError::FiscalRule(_)) => Ok(true),
                    Err(error) => Err(error),
                }
            }
            .await;
            match outcome {
                Ok(true) => settled += 1,
                Ok(false) => {}
                Err(error) => tracing::warn!(%error, "a fiscal event could not be settled"),
            }
        }
        Ok((asked, settled))
    }

    async fn write_settlement(
        &self,
        actor: &Actor,
        before: &FiscalEvent,
        after: &FiscalEvent,
    ) -> Result<(), ApplicationError> {
        let audit = audit_entry(
            after,
            actor,
            AuditAction::Updated,
            fiscal_event_changes(before, after),
        );
        self.documents.settle_event(after, &audit).await?;
        Ok(())
    }

    async fn ready_settings(
        &self,
        company_id: CompanyId,
    ) -> Result<FiscalSettings, ApplicationError> {
        if !self.gateway.is_enabled() {
            return Err(GatewayError::Disabled.into());
        }
        self.settings.find(company_id).await?.ok_or_else(|| {
            ApplicationError::FiscalRule("set the fiscal settings before issuing".to_owned())
        })
    }

    /// The settings the document was issued under: the gateway's token follows the environment and its CNPJ is the
    /// issuer's, so a document is asked about only while the company still issues the way it was issued.
    async fn settings_of(
        &self,
        company_id: CompanyId,
        document: &FiscalDocument,
    ) -> Result<FiscalSettings, ApplicationError> {
        let settings = self.ready_settings(company_id).await?;
        if settings.cnpj() != document.issuer_cnpj()
            || settings.environment() != document.environment()
        {
            return Err(ApplicationError::FiscalRule(
                "the fiscal settings changed the issuer or the environment since this document was issued: put them back to ask about it"
                    .to_owned(),
            ));
        }
        Ok(settings)
    }

    async fn completed_sale(
        &self,
        company_id: CompanyId,
        id: SaleId,
    ) -> Result<Sale, ApplicationError> {
        let sale = self
            .sales
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::SaleNotFound)?;
        if sale.status() != SaleStatus::Completed {
            return Err(ApplicationError::FiscalRule(
                "only a completed sale is invoiced".to_owned(),
            ));
        }
        Ok(sale)
    }

    /// A document found live that is past its draft: one waiting is asked about, the others are the answer.
    async fn refresh_or_return(
        &self,
        company_id: CompanyId,
        document: FiscalDocument,
        now_unix: i64,
    ) -> Result<FiscalDocument, ApplicationError> {
        if document.status().is_pending() {
            self.refresh(company_id, document.id(), now_unix).await
        } else {
            Ok(document)
        }
    }

    /// Marks the document processing and asks the gateway to authorize it, then records what came back. A gateway
    /// that cannot be reached leaves it processing: the poll or the person's refresh asks later, and it is never
    /// submitted under another reference.
    async fn send(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        document: FiscalDocument,
        nfe: &NfeRequest,
    ) -> Result<FiscalDocument, ApplicationError> {
        let processing = self
            .move_to(actor, document, |document| document.submitted())
            .await?;
        match self.gateway.submit_nfe(nfe).await {
            Ok(submission) => {
                // A gateway that names its documents says the name now: kept before anything else is recorded, because
                // every later call is about the document by that name.
                let processing = match submission.reference {
                    Some(name) => {
                        self.move_to(actor, processing, |document| document.referenced(&name))
                            .await?
                    }
                    None => processing,
                };
                self.apply(company_id, actor, processing, submission.outcome, true)
                    .await
            }
            Err(GatewayError::Unavailable(detail)) => {
                tracing::warn!(%detail, "the fiscal gateway did not answer a submission");
                Ok(processing)
            }
            // Nothing was sent: the document is refused, in the gateway's own words, and may be sent again.
            Err(error) => {
                let refusal = FiscalRefusal::new("gateway", &error.to_string());
                self.move_to(actor, processing, |document| document.rejected(refusal))
                    .await
            }
        }
    }

    /// Records what the gateway says about a document that is waiting. Every step can be repeated.
    async fn apply(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        document: FiscalDocument,
        outcome: DocumentOutcome,
        unknown_is_final: bool,
    ) -> Result<FiscalDocument, ApplicationError> {
        match outcome {
            DocumentOutcome::Processing => Ok(document),
            DocumentOutcome::Contingency => {
                if document.status() == FiscalDocumentStatus::Processing {
                    self.move_to(actor, document, |d| d.in_contingency()).await
                } else {
                    Ok(document)
                }
            }
            DocumentOutcome::Authorized(authorized) => {
                self.authorize(company_id, actor, document, &authorized)
                    .await
            }
            DocumentOutcome::Rejected(refusal) => {
                let refusal = FiscalRefusal::new(&refusal.code, &refusal.message);
                self.move_to(actor, document, |d| d.rejected(refusal)).await
            }
            DocumentOutcome::Denied(refusal) => {
                let refusal = FiscalRefusal::new(&refusal.code, &refusal.message);
                self.move_to(actor, document, |d| d.denied(refusal)).await
            }
            DocumentOutcome::Cancelled {
                document: authorized,
                cancellation_xml,
            } => {
                let document = if document.status().is_pending() {
                    self.authorize(company_id, actor, document, &authorized)
                        .await?
                } else {
                    document
                };
                // A gateway that cannot hand over the cancellation's XML says so with no bytes: the document is
                // cancelled all the same, and there is no file to keep.
                if !cancellation_xml.is_empty() {
                    self.store_file(
                        company_id,
                        document.id(),
                        FiscalFileKind::CancellationXml,
                        &cancellation_xml,
                    )
                    .await?;
                }
                if document.status() == FiscalDocumentStatus::Authorized {
                    self.move_to(actor, document, |d| d.cancelled()).await
                } else {
                    Ok(document)
                }
            }
            // The gateway has no document under our reference, so nothing of it was taken: it is refused in words
            // the person can act on, and may be sent again under the same reference.
            DocumentOutcome::Unknown if !unknown_is_final => Ok(document),
            DocumentOutcome::Unknown => {
                let refusal = FiscalRefusal::new(
                    "unknown",
                    "the gateway has no document with this reference: send it again",
                );
                self.move_to(actor, document, |d| d.rejected(refusal)).await
            }
        }
    }

    /// The files are written before the record that says the document is authorized, so an authorized document
    /// always has its XML. Authorizing late into a closed month is written with a note, never refused.
    async fn authorize(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        document: FiscalDocument,
        authorized: &AuthorizedDocument,
    ) -> Result<FiscalDocument, ApplicationError> {
        let id = document.id();
        self.store_file(company_id, id, FiscalFileKind::Xml, &authorized.xml)
            .await?;
        if let Some(pdf) = &authorized.pdf {
            self.store_file(company_id, id, FiscalFileKind::Pdf, pdf)
                .await?;
        }
        let authorization = FiscalAuthorization::new(
            authorized.access_key.clone(),
            authorized.number,
            authorized.series,
            authorized.protocol.as_deref(),
        )?;
        let late = matches!(
            self.references
                .ensure_open(company_id, document.issued_on())
                .await,
            Err(ApplicationError::PeriodLocked { .. })
        );
        let note = late.then_some("authorized after the period of its issue day was closed");
        self.move_to(actor, document, |d| d.authorized(authorization, note))
            .await
    }

    /// Stores a file once: the bytes first, then the record that names them. A file already recorded is left as it is.
    async fn store_file(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
        kind: FiscalFileKind,
        bytes: &[u8],
    ) -> Result<(), ApplicationError> {
        if self
            .documents
            .find_file(company_id, id, kind)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let blob = BlobId::generate();
        let file = FiscalFile::new(blob, company_id, id, kind, bytes.len() as u64)?;
        self.blobs.put(company_id, blob, bytes).await?;
        match self.documents.save_file(&file).await {
            Ok(()) | Err(RepositoryError::DuplicateFiscalFile) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Writes one move of the document from the status it has, with its audit entry.
    async fn move_to(
        &self,
        actor: &Actor,
        document: FiscalDocument,
        step: impl FnOnce(&FiscalDocument) -> Result<FiscalDocument, conta_verde_domain::DomainError>,
    ) -> Result<FiscalDocument, ApplicationError> {
        let moved = step(&document)?;
        let audit = audit_entry(
            &moved,
            actor,
            AuditAction::Updated,
            fiscal_document_changes(&document, &moved),
        );
        self.documents
            .replace(&moved, document.status(), &audit)
            .await?;
        Ok(moved)
    }

    /// What the document and the gateway's request are made of. `id` is the document's, which the gateway keeps as
    /// its reference.
    #[allow(clippy::too_many_arguments)]
    async fn build(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        settings: &FiscalSettings,
        sale: &Sale,
        request: &IssueNfe,
        issued_on: CalendarDate,
        now_unix: i64,
        id: FiscalDocumentId,
    ) -> Result<(FiscalDocumentDraft, NfeRequest), ApplicationError> {
        let rule = |text: &str| ApplicationError::FiscalRule(text.to_owned());
        if settings.regime() != TaxRegime::SimplesNacional {
            return Err(rule(
                "only the Simples Nacional regime can issue for now: the tax calculation of the other regimes is not built",
            ));
        }
        if request.origin > 8 {
            return Err(rule("the origin of the goods is a number from 0 to 8"));
        }
        let nature = settings
            .natures()
            .iter()
            .find(|nature| nature.code() == request.nature_code.trim().to_ascii_lowercase())
            .filter(|nature| nature.is_active() && nature.kind() == NatureKind::Product)
            .ok_or_else(|| rule("choose an active operation nature for products"))?;
        let customer = self
            .parties
            .find(company_id, sale.customer_id())
            .await?
            .ok_or_else(|| rule("the customer of the sale does not exist"))?;
        let recipient = recipient_of(&customer)?;
        let recipient_state = customer
            .address()
            .map(|address| address.state().to_owned())
            .ok_or_else(|| {
                rule("the customer has no address: the destination of the goods is not known")
            })?;
        let in_state = recipient_state == settings.address().state();
        let mut items = Vec::new();
        let mut total_products = conta_verde_domain::Decimal::ZERO;
        let mut total_discount = conta_verde_domain::Decimal::ZERO;
        for (index, line) in sale.lines().iter().enumerate() {
            let number =
                u16::try_from(index + 1).map_err(|_| rule("the sale has too many lines"))?;
            let product = self
                .products
                .find(company_id, line.product_id())
                .await?
                .ok_or_else(|| rule("a product of the sale does not exist"))?;
            let item = item_of(
                number,
                line,
                &product,
                nature,
                settings,
                in_state,
                request.origin,
            )?;
            total_products += line.gross();
            total_discount += line.discount().as_decimal();
            items.push(item);
        }
        let total = amount(sale.total())?;
        let nfe = NfeRequest {
            reference: id.as_uuid().to_string(),
            issuer: settings.clone(),
            nature: nature.name().to_owned(),
            issued_at_unix: now_unix,
            destination: if in_state {
                Destination::InState
            } else {
                Destination::Interstate
            },
            final_consumer: request.final_consumer,
            presence: request.presence,
            recipient: recipient.clone(),
            items,
            payments: vec![(request.payment, total)],
            total_products: amount(total_products)?,
            total_discount: amount(total_discount)?,
            total,
        };
        let Actor::User { id: user, name } = actor;
        let draft = FiscalDocumentDraft {
            origin: FiscalOrigin::Sale(sale.id()),
            issuer_cnpj: settings.cnpj().to_owned(),
            issuer_name: settings.legal_name().to_owned(),
            regime: settings.regime(),
            environment: settings.environment(),
            recipient_name: recipient.name,
            recipient_document: recipient.document,
            nature_code: nature.code().to_owned(),
            total,
            issued_at_unix: now_unix,
            issued_on,
            submitted_by: *user,
            submitted_by_name: name.clone(),
        };
        Ok((draft, nfe))
    }
}

/// The name the gateway knows the document by: the one it gave when it assigns its own, ours otherwise ([ADR 0078]).
fn reference(document: &FiscalDocument) -> String {
    document
        .gateway_reference()
        .map_or_else(|| document.id().as_uuid().to_string(), str::to_owned)
}

/// The person who submitted the document: what happens later without a person is written in their name.
fn submitter(document: &FiscalDocument) -> Actor {
    Actor::User {
        id: document.submitted_by(),
        name: document.submitted_by_name().to_owned(),
    }
}

fn issue_day(settings: &FiscalSettings, now_unix: i64) -> Result<CalendarDate, ApplicationError> {
    fiscal_day(now_unix, settings.address().state()).ok_or_else(|| {
        ApplicationError::FiscalRule(
            "the moment of issue is not a date the SEFAZ accepts".to_owned(),
        )
    })
}

/// The certificate's day is the person's, typed from the certificate; after it the documents would be refused at
/// the authority, so they are not sent ([ADR 0073]).
fn ensure_certificate(
    settings: &FiscalSettings,
    day: CalendarDate,
) -> Result<(), ApplicationError> {
    match settings.certificate_valid_until() {
        Some(until) if day > until => Err(ApplicationError::FiscalRule(format!(
            "the A1 certificate expired on {until}: renew it at the gateway and correct the day in the fiscal settings"
        ))),
        _ => Ok(()),
    }
}

fn amount(value: conta_verde_domain::Decimal) -> Result<Amount, ApplicationError> {
    Amount::parse(&value.normalize().to_string()).ok_or_else(|| {
        ApplicationError::FiscalRule("an amount of the sale is not valid for an invoice".to_owned())
    })
}

fn recipient_of(customer: &Party) -> Result<NfeRecipient, ApplicationError> {
    let rule = |text: &str| ApplicationError::FiscalRule(text.to_owned());
    let document = customer
        .document()
        .ok_or_else(|| rule("the customer has no CPF or CNPJ"))?;
    // A company that buys may be a taxpayer of the ICMS, which changes the invoice; the registers do not keep the
    // recipient's state registration yet, so the product would be inventing it (ADR 0076).
    if document.as_str().len() != 11 {
        return Err(rule(
            "only a customer with a CPF can be invoiced for now: the state registration of a company recipient is not kept",
        ));
    }
    Ok(NfeRecipient {
        name: customer.name().to_owned(),
        document: Some(document.as_str().to_owned()),
        email: customer.email().map(|email| email.as_str().to_owned()),
        address: customer.address().cloned(),
        state_registration: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn item_of(
    number: u16,
    line: &conta_verde_domain::Line,
    product: &Product,
    nature: &conta_verde_domain::OperationNature,
    settings: &FiscalSettings,
    in_state: bool,
    origin: u8,
) -> Result<NfeItem, ApplicationError> {
    let named = |what: &str| {
        ApplicationError::FiscalRule(format!("item {number} ({}): {what}", product.name()))
    };
    if product.kind() != ProductKind::Product {
        return Err(named("a service has no NF-e: it is a product's document"));
    }
    let ncm = product
        .ncm()
        .ok_or_else(|| named("the product has no NCM"))?;
    let rule = nature
        .rule_for(settings.regime(), ncm)
        .ok_or_else(|| named("no tax rule of the operation nature covers this NCM"))?;
    let cfop = if in_state {
        nature.cfop_in_state()
    } else {
        nature.cfop_interstate()
    }
    .ok_or_else(|| named("the operation nature has no CFOP for this destination"))?;
    Ok(NfeItem {
        number,
        code: product
            .sku()
            .map_or_else(|| product.id().as_uuid().to_string(), str::to_owned),
        description: line.description().to_owned(),
        ncm: ncm.to_owned(),
        cfop: cfop.to_owned(),
        unit: product.unit().to_owned(),
        quantity: line.quantity().as_decimal(),
        unit_price: line.unit_price().as_decimal(),
        gross: amount(line.gross())?,
        discount: line.discount(),
        origin,
        icms_code: rule.icms_code().to_owned(),
        pis_code: rule.pis_code().to_owned(),
        cofins_code: rule.cofins_code().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use conta_verde_domain::{PartyDraft, PartyId};

    fn customer(document: &str) -> Party {
        Party::new(
            PartyId::generate(),
            CompanyId::generate(),
            PartyDraft {
                name: "Cliente".into(),
                document: Some(document.into()),
                is_customer: true,
                ..PartyDraft::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn only_a_customer_with_a_cpf_is_invoiced_because_a_company_may_be_a_taxpayer_of_the_icms() {
        assert!(recipient_of(&customer("529.982.247-25")).is_ok());
        let refused = recipient_of(&customer("11.222.333/0001-81")).unwrap_err();
        assert!(matches!(refused, ApplicationError::FiscalRule(text) if text.contains("CPF")));
    }
}
