use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use conta_verde_application::{
    AcceptedEvent, AuthorizedDocument, AuthorizedNfse, DocumentOutcome, EventOutcome, EventQuery,
    FiscalGateway, GatewayError, NfceRequest, NfeRequest, NfseRequest, Refusal, Submission,
};
use conta_verde_domain::{
    AccessKey, CalendarDate, FiscalSettings, state_code, state_utc_offset_minutes,
};

const MIN_REASON_CHARS: usize = 15;
const MAX_CANCEL_REASON_CHARS: usize = 255;
const MAX_CORRECTION_CHARS: usize = 1000;
const MAX_CORRECTIONS: u32 = 20;

/// The adapter for when the operator configured no gateway (`FISCAL_GATEWAY` unset or `off`): every call is refused
/// before anything is asked, the way `PAYMENT_PROVIDER` unset switches collection off.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFiscalGateway;

impl FiscalGateway for NoFiscalGateway {
    fn key(&self) -> &str {
        "off"
    }

    fn is_enabled(&self) -> bool {
        false
    }

    async fn submit_nfe(&self, _request: &NfeRequest) -> Result<Submission, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn query_nfe(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
    ) -> Result<DocumentOutcome, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn cancel_nfe(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
        _reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn correct_nfe(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
        _text: &str,
    ) -> Result<EventOutcome, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn query_event(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
        _event: EventQuery,
    ) -> Result<EventOutcome, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn invalidate_nfe_numbers(
        &self,
        _issuer: &FiscalSettings,
        _series: u16,
        _first: u32,
        _last: u32,
        _reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn submit_nfce(&self, _request: &NfceRequest) -> Result<Submission, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn query_nfce(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
    ) -> Result<DocumentOutcome, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn cancel_nfce(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
        _reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn submit_nfse(
        &self,
        _request: &NfseRequest,
    ) -> Result<Submission<AuthorizedNfse>, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn query_nfse(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
    ) -> Result<DocumentOutcome<AuthorizedNfse>, GatewayError> {
        Err(GatewayError::Disabled)
    }

    async fn cancel_nfse(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
        _reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        Err(GatewayError::Disabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    Processing { queries_left: u32 },
    Authorized,
    Rejected(Refusal),
    Denied(Refusal),
    Cancelled,
}

#[derive(Debug, Clone)]
struct FakeDocument {
    stage: Stage,
    issuer_cnpj: String,
    series: u16,
    number: u32,
    access_key: AccessKey,
    total: String,
    corrections: u32,
}

/// An NFS-e as the fake keeps it: the number the system gave it and the DPS it came from.
#[derive(Debug, Clone)]
struct FakeNfse {
    stage: Stage,
    number: u64,
    issuer_cnpj: String,
    /// Homologation and production are two systems: a DPS number of one is not the other's.
    environment: &'static str,
    total: String,
    dps: (u16, u64),
}

#[derive(Debug, Default)]
struct FakeState {
    documents: HashMap<String, FakeDocument>,
    nfse_documents: HashMap<String, FakeNfse>,
    /// The last NFS-e number given per issuer and environment: the national system's own counter.
    nfse_counters: HashMap<(String, &'static str), u64>,
    submitted_nfse: Vec<NfseRequest>,
    /// A gateway that does not issue NFS-e, like NFE.io for now.
    without_nfse: bool,
    /// The last number given, per issuer, environment and series: the gateway's own counter, like the real one's.
    counters: HashMap<(String, &'static str, u16, u8), u32>,
    submitted: Vec<NfeRequest>,
    submitted_nfce: Vec<NfceRequest>,
    /// A gateway that does not issue NFC-e.
    without_nfce: bool,
    invalidated: Vec<(u16, u32, u32)>,
    rejection: Option<Refusal>,
    denial: Option<Refusal>,
    refuse_events: Option<Refusal>,
    queries_before_authorized: u32,
    unavailable: bool,
    /// Names its documents itself, like NFE.io: `by_request` finds what a request of ours made.
    assigns_references: bool,
    by_request: HashMap<String, String>,
    /// Answers cancellations and correction letters "queued" and does them when asked again, like NFE.io.
    queues_events: bool,
    queued: Vec<(String, QueuedEvent)>,
}

#[derive(Debug, Clone)]
enum QueuedEvent {
    Cancel(String),
    Correct(String),
}

/// A gateway that answers from memory, for development and tests: it never reaches a network, follows the same moves
/// as the real one (asynchronous authorization, numbers it assigns itself, a reference that makes asking twice safe)
/// and what it makes has **no fiscal value**. Clones share what was asked of it, so a test can look at the calls.
#[derive(Debug, Default, Clone)]
pub struct FakeFiscalGateway {
    state: Arc<Mutex<FakeState>>,
}

impl FakeFiscalGateway {
    fn lock(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().expect("fake gateway lock poisoned")
    }

    /// A fake that answers `Processing` to this many queries before it authorizes, like the real one does while the
    /// authority takes its time.
    pub fn authorizing_after(queries: u32) -> Self {
        let fake = Self::default();
        fake.lock().queries_before_authorized = queries;
        fake
    }

    /// From now on a submission is refused with this code and text.
    pub fn reject_with(&self, code: &str, message: &str) {
        self.lock().rejection = Some(Refusal::new(code, message));
    }

    pub fn accept_again(&self) {
        self.lock().rejection = None;
    }

    /// From now on a submission is denied (final) with this code and text.
    pub fn deny_with(&self, code: &str, message: &str) {
        self.lock().denial = Some(Refusal::new(code, message));
    }

    /// From now on cancellations, corrections and invalidations are refused with this code and text.
    pub fn refuse_events_with(&self, code: &str, message: &str) {
        self.lock().refuse_events = Some(Refusal::new(code, message));
    }

    pub fn accept_events_again(&self) {
        self.lock().refuse_events = None;
    }

    /// From now on the gateway names the documents itself (the reference it answers is not ours) and finds what a
    /// request of ours already made, like NFE.io does.
    pub fn assigning_references(&self) {
        self.lock().assigns_references = true;
    }

    /// From now on cancellations and correction letters answer "queued" and are done when asked about again.
    pub fn queueing_events(&self) {
        self.lock().queues_events = true;
    }

    /// Whether every call fails as if the network were down.
    pub fn set_unavailable(&self, unavailable: bool) {
        self.lock().unavailable = unavailable;
    }

    /// What was submitted, in order, repeats included.
    pub fn submitted(&self) -> Vec<NfeRequest> {
        self.lock().submitted.clone()
    }

    /// The ranges of numbers invalidated, in order.
    pub fn invalidated(&self) -> Vec<(u16, u32, u32)> {
        self.lock().invalidated.clone()
    }

    /// From now on the gateway does not issue NFS-e: an issue is refused before a DPS number is taken.
    pub fn without_nfse(&self, without: bool) {
        self.lock().without_nfse = without;
    }

    /// From now on the gateway does not issue NFC-e.
    pub fn without_nfce(&self, without: bool) {
        self.lock().without_nfce = without;
    }

    /// The NFC-e submitted, in order, repeats included.
    pub fn submitted_nfce(&self) -> Vec<NfceRequest> {
        self.lock().submitted_nfce.clone()
    }

    /// The NFS-e submitted, in order, repeats included.
    pub fn submitted_nfse(&self) -> Vec<NfseRequest> {
        self.lock().submitted_nfse.clone()
    }
}

fn nfse_files(document: &FakeNfse) -> AuthorizedNfse {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!-- fake gateway: no fiscal value --><NFSe>\
         <infNFSe><nNFSe>{number}</nNFSe><emit><CNPJ>{cnpj}</CNPJ></emit>\
         <DPS><nDPS>{dps_number}</nDPS><serie>{dps_series}</serie></DPS>\
         <valores><vLiq>{total}</vLiq></valores></infNFSe></NFSe>",
        number = document.number,
        cnpj = document.issuer_cnpj,
        dps_number = document.dps.1,
        dps_series = document.dps.0,
        total = document.total,
    );
    AuthorizedNfse {
        number: document.number,
        key: Some(format!("NFS{:047}", document.number)),
        verification_code: Some(format!("FAKE{:08}", document.number)),
        protocol: Some(format!("FAKENFSE{:07}", document.number)),
        xml: xml.into_bytes(),
        pdf: Some(b"%PDF-1.4\n% fake DANFSe, no fiscal value\n%%EOF\n".to_vec()),
    }
}

fn nfse_outcome(document: &FakeNfse) -> DocumentOutcome<AuthorizedNfse> {
    match &document.stage {
        Stage::Processing { .. } => DocumentOutcome::Processing,
        Stage::Authorized => DocumentOutcome::Authorized(nfse_files(document)),
        Stage::Rejected(refusal) => DocumentOutcome::Rejected(refusal.clone()),
        Stage::Denied(refusal) => DocumentOutcome::Denied(refusal.clone()),
        Stage::Cancelled => DocumentOutcome::Cancelled {
            document: nfse_files(document),
            cancellation_xml: format!(
                "<?xml version=\"1.0\"?><!-- fake gateway: no fiscal value --><evento>\
                 <nNFSe>{}</nNFSe><tpEvento>101101</tpEvento></evento>",
                document.number
            )
            .into_bytes(),
        },
    }
}

fn files(document: &FakeDocument) -> AuthorizedDocument {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!-- fake gateway: no fiscal value --><nfeProc><NFe>\
         <infNFe Id=\"NFe{key}\"><ide><nNF>{number}</nNF><serie>{series}</serie></ide>\
         <emit><CNPJ>{cnpj}</CNPJ></emit><total><vNF>{total}</vNF></total></infNFe></NFe>\
         <protNFe><infProt><nProt>{protocol}</nProt></infProt></protNFe></nfeProc>",
        key = document.access_key.as_str(),
        number = document.number,
        series = document.series,
        cnpj = document.issuer_cnpj,
        total = document.total,
        protocol = protocol(document),
    );
    AuthorizedDocument {
        access_key: document.access_key.clone(),
        number: document.number,
        series: document.series,
        protocol: Some(protocol(document)),
        xml: xml.into_bytes(),
        pdf: Some(b"%PDF-1.4\n% fake DANFE, no fiscal value\n%%EOF\n".to_vec()),
    }
}

fn protocol(document: &FakeDocument) -> String {
    format!("FAKE{:011}", document.number)
}

fn outcome(document: &FakeDocument) -> DocumentOutcome {
    match &document.stage {
        Stage::Processing { .. } => DocumentOutcome::Processing,
        Stage::Authorized => DocumentOutcome::Authorized(files(document)),
        Stage::Rejected(refusal) => DocumentOutcome::Rejected(refusal.clone()),
        Stage::Denied(refusal) => DocumentOutcome::Denied(refusal.clone()),
        Stage::Cancelled => DocumentOutcome::Cancelled {
            document: files(document),
            cancellation_xml: format!(
                "<?xml version=\"1.0\"?><!-- fake gateway: no fiscal value --><procEventoNFe>\
                 <chNFe>{}</chNFe><tpEvento>110111</tpEvento></procEventoNFe>",
                document.access_key.as_str()
            )
            .into_bytes(),
        },
    }
}

fn event(xml: String) -> EventOutcome {
    EventOutcome::Accepted(AcceptedEvent {
        protocol: Some("FAKEEVENT".to_owned()),
        number: None,
        xml: Some(xml.into_bytes()),
        pdf: None,
    })
}

/// The name the fake keeps a document under: what a request of ours made, when the gateway names its own documents.
fn resolve(state: &FakeState, reference: &str) -> String {
    state
        .by_request
        .get(reference)
        .cloned()
        .unwrap_or_else(|| reference.to_owned())
}

fn do_cancel(state: &mut FakeState, reference: &str, reason: &str) -> EventOutcome {
    if let Some(refusal) = state.refuse_events.clone() {
        return EventOutcome::Refused(refusal);
    }
    let length = reason.trim().chars().count();
    if !(MIN_REASON_CHARS..=MAX_CANCEL_REASON_CHARS).contains(&length) {
        return EventOutcome::Refused(Refusal::new(
            "justificativa",
            "the reason has from 15 to 255 characters",
        ));
    }
    match state.documents.get_mut(reference) {
        Some(document) if document.stage == Stage::Authorized => {
            document.stage = Stage::Cancelled;
            let key = document.access_key.as_str().to_owned();
            event(format!(
                "<?xml version=\"1.0\"?><!-- fake gateway: no fiscal value --><procEventoNFe>\
                 <chNFe>{key}</chNFe><tpEvento>110111</tpEvento></procEventoNFe>"
            ))
        }
        _ => EventOutcome::Refused(Refusal::new(
            "nao_autorizada",
            "only an authorized document can be cancelled",
        )),
    }
}

fn do_correct(state: &mut FakeState, reference: &str, text: &str) -> EventOutcome {
    if let Some(refusal) = state.refuse_events.clone() {
        return EventOutcome::Refused(refusal);
    }
    let length = text.trim().chars().count();
    if !(MIN_REASON_CHARS..=MAX_CORRECTION_CHARS).contains(&length) {
        return EventOutcome::Refused(Refusal::new(
            "correcao",
            "the correction has from 15 to 1000 characters",
        ));
    }
    match state.documents.get_mut(reference) {
        Some(document)
            if document.stage == Stage::Authorized && document.corrections < MAX_CORRECTIONS =>
        {
            document.corrections += 1;
            let number = document.corrections;
            let key = document.access_key.as_str().to_owned();
            EventOutcome::Accepted(AcceptedEvent {
                protocol: Some(format!("FAKEEVENT{number}")),
                number: Some(number),
                xml: Some(
                    format!(
                        "<?xml version=\"1.0\"?><!-- fake gateway: no fiscal value --><procEventoNFe>\
                         <chNFe>{key}</chNFe><tpEvento>110110</tpEvento><nSeqEvento>{number}</nSeqEvento>\
                         </procEventoNFe>"
                    )
                    .into_bytes(),
                ),
                pdf: Some(b"%PDF-1.4\n% fake CC-e, no fiscal value\n%%EOF\n".to_vec()),
            })
        }
        _ => EventOutcome::Refused(Refusal::new(
            "nao_corrigivel",
            "only an authorized document can be corrected, up to 20 times",
        )),
    }
}

impl FiscalGateway for FakeFiscalGateway {
    fn key(&self) -> &str {
        "fake"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn submit_nfe(&self, request: &NfeRequest) -> Result<Submission, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        state.submitted.push(request.clone());
        submit_keyed(
            &mut state,
            55,
            request.issuer.nfe_series(),
            &request.reference,
            &request.issuer,
            request.issued_at_unix,
            request.total.to_string(),
        )
    }

    async fn submit_nfce(&self, request: &NfceRequest) -> Result<Submission, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        state.submitted_nfce.push(request.clone());
        submit_keyed(
            &mut state,
            65,
            request.issuer.nfce_series(),
            &request.reference,
            &request.issuer,
            request.issued_at_unix,
            request.total.to_string(),
        )
    }

    fn issues_nfce(&self) -> bool {
        !self.lock().without_nfce
    }

    async fn query_nfce(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome, GatewayError> {
        self.query_nfe(issuer, reference).await
    }

    async fn cancel_nfce(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        self.cancel_nfe(issuer, reference, reason).await
    }

    async fn query_nfe(
        &self,
        _issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        let known_as = resolve(&state, reference);
        let Some(document) = state.documents.get_mut(&known_as) else {
            return Ok(DocumentOutcome::Unknown);
        };
        if let Stage::Processing { queries_left } = &mut document.stage {
            *queries_left -= 1;
            if *queries_left == 0 {
                document.stage = Stage::Authorized;
            }
        }
        Ok(outcome(document))
    }

    async fn cancel_nfe(
        &self,
        _issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        let known_as = resolve(&state, reference);
        if state.queues_events && state.refuse_events.is_none() {
            state
                .queued
                .push((known_as, QueuedEvent::Cancel(reason.to_owned())));
            return Ok(EventOutcome::Queued);
        }
        Ok(do_cancel(&mut state, &known_as, reason))
    }

    async fn correct_nfe(
        &self,
        _issuer: &FiscalSettings,
        reference: &str,
        text: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        let known_as = resolve(&state, reference);
        if state.queues_events && state.refuse_events.is_none() {
            state
                .queued
                .push((known_as, QueuedEvent::Correct(text.to_owned())));
            return Ok(EventOutcome::Queued);
        }
        Ok(do_correct(&mut state, &known_as, text))
    }

    async fn query_event(
        &self,
        _issuer: &FiscalSettings,
        reference: &str,
        _event: EventQuery,
    ) -> Result<EventOutcome, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        let known_as = resolve(&state, reference);
        if let Some(refusal) = state.refuse_events.clone() {
            state.queued.retain(|(queued, _)| *queued != known_as);
            return Ok(EventOutcome::Refused(refusal));
        }
        let Some(place) = state
            .queued
            .iter()
            .position(|(queued, _)| *queued == known_as)
        else {
            return Ok(EventOutcome::Queued);
        };
        let (_, queued) = state.queued.remove(place);
        Ok(match queued {
            QueuedEvent::Cancel(reason) => do_cancel(&mut state, &known_as, &reason),
            QueuedEvent::Correct(text) => do_correct(&mut state, &known_as, &text),
        })
    }

    async fn invalidate_nfe_numbers(
        &self,
        _issuer: &FiscalSettings,
        series: u16,
        first: u32,
        last: u32,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        if let Some(refusal) = state.refuse_events.clone() {
            return Ok(EventOutcome::Refused(refusal));
        }
        if reason.trim().chars().count() < MIN_REASON_CHARS || first == 0 || first > last {
            return Ok(EventOutcome::Refused(Refusal::new(
                "invalido",
                "a range from the first to the last number, and a reason of at least 15 characters",
            )));
        }
        state.invalidated.push((series, first, last));
        Ok(EventOutcome::Accepted(AcceptedEvent {
            protocol: Some("FAKEINUT".to_owned()),
            number: None,
            xml: Some(
                format!(
                    "<?xml version=\"1.0\"?><!-- fake gateway: no fiscal value --><retInutNFe>\
                     <serie>{series}</serie><nNFIni>{first}</nNFIni><nNFFin>{last}</nNFFin></retInutNFe>"
                )
                .into_bytes(),
            ),
            pdf: None,
        }))
    }

    fn issues_nfse(&self) -> bool {
        !self.lock().without_nfse
    }

    async fn submit_nfse(
        &self,
        request: &NfseRequest,
    ) -> Result<Submission<AuthorizedNfse>, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        state.submitted_nfse.push(request.clone());
        let dps = (request.dps_series, request.dps_number);
        // The same reference again is the same document, unless it was refused: then it is another try.
        if let Some(known) = state.nfse_documents.get(&request.reference)
            && !matches!(known.stage, Stage::Rejected(_))
        {
            return Ok(Submission::of(nfse_outcome(known)));
        }
        // The national system refuses a DPS number that another reference already used, which is why the issuer never
        // gives one twice.
        let used_by_another = state.nfse_documents.iter().any(|(reference, known)| {
            *reference != request.reference
                && known.issuer_cnpj == request.issuer.cnpj()
                && known.environment == request.issuer.environment().key()
                && known.dps == dps
        });
        let refusal = if used_by_another {
            Some(Refusal::new(
                "dps_duplicada",
                "a DPS with this series and number was already received",
            ))
        } else {
            state.rejection.clone()
        };
        let issuer_cnpj = request.issuer.cnpj().to_owned();
        let total = request.total.to_string();
        if let Some(refusal) = refusal {
            let document = FakeNfse {
                stage: Stage::Rejected(refusal),
                number: 0,
                issuer_cnpj,
                environment: request.issuer.environment().key(),
                total,
                dps,
            };
            let answer = nfse_outcome(&document);
            state
                .nfse_documents
                .insert(request.reference.clone(), document);
            return Ok(Submission::of(answer));
        }
        let counter = state
            .nfse_counters
            .entry((issuer_cnpj.clone(), request.issuer.environment().key()))
            .or_insert(0);
        *counter += 1;
        let number = *counter;
        let stage = match state.denial.clone() {
            Some(refusal) => Stage::Denied(refusal),
            None if state.queries_before_authorized == 0 => Stage::Authorized,
            None => Stage::Processing {
                queries_left: state.queries_before_authorized,
            },
        };
        let document = FakeNfse {
            stage,
            number,
            issuer_cnpj,
            environment: request.issuer.environment().key(),
            total,
            dps,
        };
        let answer = nfse_outcome(&document);
        state
            .nfse_documents
            .insert(request.reference.clone(), document);
        Ok(Submission::of(answer))
    }

    async fn query_nfse(
        &self,
        _issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome<AuthorizedNfse>, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        let Some(document) = state.nfse_documents.get_mut(reference) else {
            return Ok(DocumentOutcome::Unknown);
        };
        if let Stage::Processing { queries_left } = &mut document.stage {
            *queries_left -= 1;
            if *queries_left == 0 {
                document.stage = Stage::Authorized;
            }
        }
        Ok(nfse_outcome(document))
    }

    async fn cancel_nfse(
        &self,
        _issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        if let Some(refusal) = state.refuse_events.clone() {
            return Ok(EventOutcome::Refused(refusal));
        }
        if reason.trim().chars().count() < MIN_REASON_CHARS {
            return Ok(EventOutcome::Refused(Refusal::new(
                "justificativa",
                "the reason has at least 15 characters",
            )));
        }
        // Answered at once, never queued: the port's NFS-e cancellation is synchronous.
        Ok(match state.nfse_documents.get_mut(reference) {
            Some(document) if document.stage == Stage::Authorized => {
                document.stage = Stage::Cancelled;
                let number = document.number;
                event(format!(
                    "<?xml version=\"1.0\"?><!-- fake gateway: no fiscal value --><evento>\
                     <nNFSe>{number}</nNFSe><tpEvento>101101</tpEvento></evento>"
                ))
            }
            _ => EventOutcome::Refused(Refusal::new(
                "nao_autorizada",
                "only an authorized document can be cancelled",
            )),
        })
    }
}

/// What a submission does at the fake, for the NF-e (model 55) or the NFC-e (65): the same reference is the same document,
/// a refusal takes no number, and the numbers are the gateway's own per issuer, environment, series and model.
fn submit_keyed(
    state: &mut FakeState,
    model: u8,
    series: u16,
    reference: &str,
    issuer: &FiscalSettings,
    issued_at_unix: i64,
    total: String,
) -> Result<Submission, GatewayError> {
    // The same reference again is the same document, unless it was refused: then it is another try.
    let known_as = resolve(state, reference);
    if let Some(known) = state.documents.get(&known_as)
        && !matches!(known.stage, Stage::Rejected(_))
    {
        let named = state.assigns_references.then(|| known_as.clone());
        return Ok(Submission {
            reference: named,
            outcome: outcome(known),
        });
    }
    let assigned = if state.assigns_references {
        format!("gw-{}", state.documents.len() + 1)
    } else {
        reference.to_owned()
    };
    if state.assigns_references {
        state
            .by_request
            .insert(reference.to_owned(), assigned.clone());
    }
    let named = state.assigns_references.then(|| assigned.clone());
    if let Some(refusal) = state.rejection.clone() {
        let document = refused_document(issuer, &total, Stage::Rejected(refusal));
        let answer = outcome(&document);
        state.documents.insert(assigned, document);
        return Ok(Submission {
            reference: named,
            outcome: answer,
        });
    }
    let counter = state
        .counters
        .entry((
            issuer.cnpj().to_owned(),
            issuer.environment().key(),
            series,
            model,
        ))
        .or_insert(0);
    *counter += 1;
    let number = *counter;
    // The key's month is the issuer's own day, like the document's.
    let offset = i64::from(state_utc_offset_minutes(issuer.address().state()).unwrap_or(-180));
    let day = CalendarDate::from_unix_days((issued_at_unix + offset * 60).div_euclid(86_400))
        .ok_or_else(|| GatewayError::Unavailable("the moment is out of range".into()))?;
    let state_code = state_code(issuer.address().state())
        .ok_or_else(|| GatewayError::Unavailable("the issuer's state is unknown".into()))?;
    let access_key = AccessKey::from_parts(
        state_code,
        day,
        issuer.cnpj(),
        model,
        series,
        number,
        1,
        10_000_000 + number,
    )
    .map_err(|error| GatewayError::Unavailable(error.to_string()))?;
    let stage = match state.denial.clone() {
        Some(refusal) => Stage::Denied(refusal),
        None if state.queries_before_authorized == 0 => Stage::Authorized,
        None => Stage::Processing {
            queries_left: state.queries_before_authorized,
        },
    };
    let document = FakeDocument {
        stage,
        issuer_cnpj: issuer.cnpj().to_owned(),
        series,
        number,
        access_key,
        total,
        corrections: 0,
    };
    let answer = outcome(&document);
    state.documents.insert(assigned, document);
    Ok(Submission {
        reference: named,
        outcome: answer,
    })
}

/// A document the gateway refused: it keeps no number, since a refusal before authorization consumes none.
fn refused_document(issuer: &FiscalSettings, total: &str, stage: Stage) -> FakeDocument {
    FakeDocument {
        stage,
        issuer_cnpj: issuer.cnpj().to_owned(),
        series: issuer.nfe_series(),
        number: 0,
        // Never read: a refused document has no files. Any right key will do.
        access_key: AccessKey::from_parts(
            35,
            CalendarDate::parse("2026-01-01").expect("a real day"),
            issuer.cnpj(),
            55,
            1,
            1,
            1,
            1,
        )
        .expect("a right key"),
        total: total.to_owned(),
        corrections: 0,
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_domain::FiscalEnvironment;
    use uuid::Uuid;

    use super::*;
    use crate::contract::{self, fiscal_issuer, nfe_request};

    fn issuer() -> FiscalSettings {
        fiscal_issuer(contract::ACTIVE_CNPJ, FiscalEnvironment::Homologation)
    }

    #[tokio::test]
    async fn authorizes_cancels_and_corrects() {
        contract::a_gateway_authorizes_cancels_and_corrects(
            &FakeFiscalGateway::default(),
            &issuer(),
        )
        .await;
    }

    #[tokio::test]
    async fn authorizes_and_cancels_an_nfce_and_counts_its_numbers_apart_from_the_nfes() {
        let gateway = FakeFiscalGateway::default();
        contract::a_gateway_authorizes_and_cancels_an_nfce(&gateway, &issuer()).await;
        contract::a_gateway_authorizes_and_cancels_an_nfce(
            &FakeFiscalGateway::authorizing_after(2),
            &issuer(),
        )
        .await;
        // The same series number of an NF-e is not taken from the NFC-e's: the model is part of what is counted.
        let sent = gateway
            .submit_nfe(&nfe_request(&issuer(), &Uuid::new_v4().to_string()))
            .await
            .unwrap();
        let DocumentOutcome::Authorized(nfe) = sent.outcome else {
            panic!("authorized was expected");
        };
        assert_eq!(nfe.number, 1);
        assert_eq!(nfe.access_key.model(), "55");
    }

    #[tokio::test]
    async fn authorizes_and_cancels_an_nfse() {
        let issuer =
            contract::fiscal_service_issuer(contract::ACTIVE_CNPJ, FiscalEnvironment::Homologation);
        contract::a_gateway_authorizes_and_cancels_an_nfse(&FakeFiscalGateway::default(), &issuer)
            .await;
        contract::a_gateway_authorizes_and_cancels_an_nfse(
            &FakeFiscalGateway::authorizing_after(2),
            &issuer,
        )
        .await;
    }

    #[tokio::test]
    async fn an_nfse_numbers_are_its_own_and_not_the_nfes() {
        let gateway = FakeFiscalGateway::default();
        let issuer =
            contract::fiscal_service_issuer(contract::ACTIVE_CNPJ, FiscalEnvironment::Homologation);
        gateway
            .submit_nfe(&nfe_request(&issuer, &Uuid::new_v4().to_string()))
            .await
            .unwrap();
        let sent = gateway
            .submit_nfse(&contract::nfse_request(
                &issuer,
                &Uuid::new_v4().to_string(),
                1,
            ))
            .await
            .unwrap();
        let DocumentOutcome::Authorized(nfse) = sent.outcome else {
            panic!("authorized was expected");
        };
        assert_eq!(nfse.number, 1);
        assert_eq!(gateway.submitted_nfse().len(), 1);
    }

    #[tokio::test]
    async fn authorizes_after_a_few_questions_like_the_real_one() {
        contract::a_gateway_authorizes_cancels_and_corrects(
            &FakeFiscalGateway::authorizing_after(2),
            &issuer(),
        )
        .await;
        let gateway = FakeFiscalGateway::authorizing_after(2);
        let reference = Uuid::new_v4().to_string();
        let sent = gateway
            .submit_nfe(&nfe_request(&issuer(), &reference))
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        assert_eq!(sent, DocumentOutcome::Processing);
        assert_eq!(
            gateway.query_nfe(&issuer(), &reference).await.unwrap(),
            DocumentOutcome::Processing
        );
        assert!(matches!(
            gateway.query_nfe(&issuer(), &reference).await.unwrap(),
            DocumentOutcome::Authorized(_)
        ));
    }

    #[tokio::test]
    async fn a_refusal_takes_no_number_and_the_same_reference_may_try_again() {
        let gateway = FakeFiscalGateway::default();
        let reference = Uuid::new_v4().to_string();
        gateway.reject_with(
            "539",
            "Rejeição: duplicidade de NF-e\n com diferença na chave",
        );
        let refused = gateway
            .submit_nfe(&nfe_request(&issuer(), &reference))
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        // Control characters are gone, so the text is one line a row can hold.
        assert_eq!(
            refused,
            DocumentOutcome::Rejected(Refusal {
                code: "539".into(),
                message: "Rejeição: duplicidade de NF-e com diferença na chave".into()
            })
        );
        assert_eq!(
            gateway.query_nfe(&issuer(), &reference).await.unwrap(),
            refused
        );
        gateway.accept_again();
        match gateway
            .submit_nfe(&nfe_request(&issuer(), &reference))
            .await
            .map(|submission| submission.outcome)
            .unwrap()
        {
            DocumentOutcome::Authorized(document) => assert_eq!(document.number, 1),
            other => panic!("authorized was expected, got {other:?}"),
        }
        assert_eq!(gateway.submitted().len(), 2);
    }

    #[tokio::test]
    async fn a_denial_is_final_and_events_may_be_refused_and_the_network_may_fail() {
        let gateway = FakeFiscalGateway::default();
        let reference = Uuid::new_v4().to_string();
        gateway.deny_with("301", "Uso Denegado");
        assert!(matches!(
            gateway
                .submit_nfe(&nfe_request(&issuer(), &reference))
                .await
                .map(|submission| submission.outcome)
                .unwrap(),
            DocumentOutcome::Denied(_)
        ));
        assert!(matches!(
            gateway
                .submit_nfe(&nfe_request(&issuer(), &reference))
                .await
                .map(|submission| submission.outcome)
                .unwrap(),
            DocumentOutcome::Denied(_)
        ));

        gateway.refuse_events_with("501", "Prazo de cancelamento excedido");
        assert_eq!(
            gateway
                .cancel_nfe(&issuer(), &reference, "Venda desfeita a pedido do cliente")
                .await
                .unwrap(),
            EventOutcome::Refused(Refusal::new("501", "Prazo de cancelamento excedido"))
        );

        gateway.set_unavailable(true);
        assert!(matches!(
            gateway.query_nfe(&issuer(), &reference).await,
            Err(GatewayError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn the_numbers_are_the_gateways_per_series_and_environment() {
        let gateway = FakeFiscalGateway::default();
        let number = |settings: FiscalSettings| {
            let gateway = gateway.clone();
            async move {
                let reference = Uuid::new_v4().to_string();
                match gateway
                    .submit_nfe(&nfe_request(&settings, &reference))
                    .await
                    .map(|submission| submission.outcome)
                    .unwrap()
                {
                    DocumentOutcome::Authorized(document) => document.number,
                    other => panic!("authorized was expected, got {other:?}"),
                }
            }
        };
        assert_eq!(number(issuer()).await, 1);
        assert_eq!(number(issuer()).await, 2);
        assert_eq!(
            number(fiscal_issuer(
                contract::ACTIVE_CNPJ,
                FiscalEnvironment::Production
            ))
            .await,
            1
        );
    }

    #[tokio::test]
    async fn the_off_gateway_refuses_everything_before_asking() {
        let gateway = NoFiscalGateway;
        assert!(!gateway.is_enabled());
        assert_eq!(gateway.key(), "off");
        assert_eq!(
            gateway
                .submit_nfe(&nfe_request(&issuer(), "x"))
                .await
                .map(|submission| submission.outcome),
            Err(GatewayError::Disabled)
        );
        assert_eq!(
            gateway.query_nfe(&issuer(), "x").await,
            Err(GatewayError::Disabled)
        );
        let service =
            contract::fiscal_service_issuer(contract::ACTIVE_CNPJ, FiscalEnvironment::Homologation);
        assert_eq!(
            gateway
                .submit_nfse(&contract::nfse_request(&service, "x", 1))
                .await
                .map(|submission| submission.outcome),
            Err(GatewayError::Disabled)
        );
        assert_eq!(
            gateway.query_nfse(&service, "x").await,
            Err(GatewayError::Disabled)
        );
        assert_eq!(
            gateway
                .submit_nfce(&contract::nfce_request(&service, "x"))
                .await
                .map(|submission| submission.outcome),
            Err(GatewayError::Disabled)
        );
        assert_eq!(
            gateway
                .cancel_nfse(&service, "x", "Serviço não prestado, engano")
                .await,
            Err(GatewayError::Disabled)
        );
    }
}
