use conta_verde_domain::{
    Actor, Amount, AuditAction, AuditRecord, BlobId, CalendarDate, CompanyId, FiscalAuthorization,
    FiscalDocument, FiscalDocumentDraft, FiscalDocumentId, FiscalDocumentKind,
    FiscalDocumentStatus, FiscalFile, FiscalFileKind, FiscalOrigin, FiscalRefusal, FiscalSettings,
    NatureKind, Party, Product, ProductKind, Sale, SaleId, SaleStatus, TaxRegime, fiscal_day,
    fiscal_document_changes, fiscal_document_creation_changes,
};

use crate::audit::audit_entry;
use crate::{
    ApplicationError, AuthorizedDocument, BlobStore, BuyerPresence, Destination, DocumentOutcome,
    EntryReferences, EventOutcome, FiscalDocumentRepository, FiscalGateway,
    FiscalSettingsRepository, GatewayError, NfeItem, NfeRecipient, NfeRequest, PartyRepository,
    PaymentForm, ProductRepository, RepositoryError, SaleRepository,
};

const MIN_REASON_CHARS: usize = 15;
const MAX_CANCEL_REASON_CHARS: usize = 255;

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
    ) -> Result<PollReport, ApplicationError> {
        let mut report = PollReport::default();
        for document in self.documents.pending(since_unix, limit).await? {
            report.asked += 1;
            match self.refresh(document.company_id(), document.id()).await {
                Ok(refreshed) if !refreshed.status().is_pending() => report.settled += 1,
                Ok(_) => {}
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(%error, "a fiscal document could not be refreshed");
                }
            }
        }
        Ok(report)
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
        let issued_on = issue_day(&settings, now_unix)?;
        ensure_certificate(&settings, issued_on)?;
        self.references.ensure_open(company_id, issued_on).await?;
        let sale = self.completed_sale(company_id, request.sale_id).await?;
        let origin = FiscalOrigin::Sale(sale.id());
        let live = self
            .documents
            .find_live(company_id, origin, FiscalDocumentKind::Nfe)
            .await?;
        // What is past the draft is the answer to asking twice, whatever this request says.
        if let Some(live) = live
            .as_ref()
            .filter(|live| live.status() != FiscalDocumentStatus::Draft)
        {
            return self.refresh_or_return(company_id, live.clone()).await;
        }
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
                        return self.refresh_or_return(company_id, live).await;
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
    ) -> Result<FiscalDocument, ApplicationError> {
        let document = self.get(company_id, id).await?;
        if !document.status().is_pending() {
            return Ok(document);
        }
        let settings = self.ready_settings(company_id).await?;
        let actor = submitter(&document);
        let outcome = self
            .gateway
            .query_nfe(&settings, &reference(&document))
            .await?;
        self.apply(company_id, &actor, document, outcome).await
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
        let settings = self.ready_settings(company_id).await?;
        let state = self
            .gateway
            .query_nfe(&settings, &reference(&document))
            .await?;
        if let DocumentOutcome::Cancelled { .. } = state {
            return self.apply(company_id, actor, document, state).await;
        }
        match self
            .gateway
            .cancel_nfe(&settings, &reference(&document), reason)
            .await?
        {
            EventOutcome::Accepted(event) => {
                if let Some(xml) = event.xml {
                    self.store_file(company_id, id, FiscalFileKind::CancellationXml, &xml)
                        .await?;
                }
                self.move_to(actor, document, |document| document.cancelled())
                    .await
            }
            EventOutcome::Refused(refusal) => Err(ApplicationError::FiscalRule(format!(
                "the authority refused the cancellation ({}): {}",
                refusal.code, refusal.message
            ))),
        }
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
    ) -> Result<FiscalDocument, ApplicationError> {
        if document.status().is_pending() {
            self.refresh(company_id, document.id()).await
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
            Ok(outcome) => self.apply(company_id, actor, processing, outcome).await,
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
                self.store_file(
                    company_id,
                    document.id(),
                    FiscalFileKind::CancellationXml,
                    &cancellation_xml,
                )
                .await?;
                if document.status() == FiscalDocumentStatus::Authorized {
                    self.move_to(actor, document, |d| d.cancelled()).await
                } else {
                    Ok(document)
                }
            }
            // The gateway has no document under our reference, so nothing of it was taken: it is refused in words
            // the person can act on, and may be sent again under the same reference.
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

fn reference(document: &FiscalDocument) -> String {
    document.id().as_uuid().to_string()
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
