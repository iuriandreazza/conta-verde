use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use conta_verde_application::{
    AcceptedEvent, AuthorizedDocument, DocumentOutcome, EventOutcome, FiscalGateway, GatewayError,
    NfeRequest, Refusal,
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

    async fn submit_nfe(&self, _request: &NfeRequest) -> Result<DocumentOutcome, GatewayError> {
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

#[derive(Debug, Default)]
struct FakeState {
    documents: HashMap<String, FakeDocument>,
    /// The last number given, per issuer, environment and series: the gateway's own counter, like the real one's.
    counters: HashMap<(String, &'static str, u16), u32>,
    submitted: Vec<NfeRequest>,
    invalidated: Vec<(u16, u32, u32)>,
    rejection: Option<Refusal>,
    denial: Option<Refusal>,
    refuse_events: Option<Refusal>,
    queries_before_authorized: u32,
    unavailable: bool,
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

impl FiscalGateway for FakeFiscalGateway {
    fn key(&self) -> &str {
        "fake"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn submit_nfe(&self, request: &NfeRequest) -> Result<DocumentOutcome, GatewayError> {
        let mut state = self.lock();
        if state.unavailable {
            return Err(GatewayError::Unavailable("the fake is switched off".into()));
        }
        state.submitted.push(request.clone());
        // The same reference again is the same document, unless it was refused: then it is another try.
        if let Some(known) = state.documents.get(&request.reference)
            && !matches!(known.stage, Stage::Rejected(_))
        {
            return Ok(outcome(known));
        }
        if let Some(refusal) = state.rejection.clone() {
            let document = refused_document(request, Stage::Rejected(refusal));
            let answer = outcome(&document);
            state.documents.insert(request.reference.clone(), document);
            return Ok(answer);
        }
        let issuer = &request.issuer;
        let series = issuer.nfe_series();
        let counter = state
            .counters
            .entry((issuer.cnpj().to_owned(), issuer.environment().key(), series))
            .or_insert(0);
        *counter += 1;
        let number = *counter;
        // The key's month is the issuer's own day, like the document's.
        let offset = i64::from(state_utc_offset_minutes(issuer.address().state()).unwrap_or(-180));
        let day =
            CalendarDate::from_unix_days((request.issued_at_unix + offset * 60).div_euclid(86_400))
                .ok_or_else(|| GatewayError::Unavailable("the moment is out of range".into()))?;
        let state_code = state_code(issuer.address().state())
            .ok_or_else(|| GatewayError::Unavailable("the issuer's state is unknown".into()))?;
        let access_key = AccessKey::from_parts(
            state_code,
            day,
            issuer.cnpj(),
            55,
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
            total: request.total.to_string(),
            corrections: 0,
        };
        let answer = outcome(&document);
        state.documents.insert(request.reference.clone(), document);
        Ok(answer)
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
        let Some(document) = state.documents.get_mut(reference) else {
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
        if let Some(refusal) = state.refuse_events.clone() {
            return Ok(EventOutcome::Refused(refusal));
        }
        let length = reason.trim().chars().count();
        if !(MIN_REASON_CHARS..=MAX_CANCEL_REASON_CHARS).contains(&length) {
            return Ok(EventOutcome::Refused(Refusal::new(
                "justificativa",
                "the reason has from 15 to 255 characters",
            )));
        }
        match state.documents.get_mut(reference) {
            Some(document) if document.stage == Stage::Authorized => {
                document.stage = Stage::Cancelled;
                let key = document.access_key.as_str().to_owned();
                Ok(event(format!(
                    "<?xml version=\"1.0\"?><!-- fake gateway: no fiscal value --><procEventoNFe>\
                     <chNFe>{key}</chNFe><tpEvento>110111</tpEvento></procEventoNFe>"
                )))
            }
            _ => Ok(EventOutcome::Refused(Refusal::new(
                "nao_autorizada",
                "only an authorized document can be cancelled",
            ))),
        }
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
        if let Some(refusal) = state.refuse_events.clone() {
            return Ok(EventOutcome::Refused(refusal));
        }
        let length = text.trim().chars().count();
        if !(MIN_REASON_CHARS..=MAX_CORRECTION_CHARS).contains(&length) {
            return Ok(EventOutcome::Refused(Refusal::new(
                "correcao",
                "the correction has from 15 to 1000 characters",
            )));
        }
        match state.documents.get_mut(reference) {
            Some(document)
                if document.stage == Stage::Authorized
                    && document.corrections < MAX_CORRECTIONS =>
            {
                document.corrections += 1;
                let number = document.corrections;
                let key = document.access_key.as_str().to_owned();
                Ok(EventOutcome::Accepted(AcceptedEvent {
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
                }))
            }
            _ => Ok(EventOutcome::Refused(Refusal::new(
                "nao_corrigivel",
                "only an authorized document can be corrected, up to 20 times",
            ))),
        }
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
}

/// A document the gateway refused: it keeps no number, since a refusal before authorization consumes none.
fn refused_document(request: &NfeRequest, stage: Stage) -> FakeDocument {
    FakeDocument {
        stage,
        issuer_cnpj: request.issuer.cnpj().to_owned(),
        series: request.issuer.nfe_series(),
        number: 0,
        // Never read: a refused document has no files. Any right key will do.
        access_key: AccessKey::from_parts(
            35,
            CalendarDate::parse("2026-01-01").expect("a real day"),
            request.issuer.cnpj(),
            55,
            1,
            1,
            1,
            1,
        )
        .expect("a right key"),
        total: request.total.to_string(),
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
                .unwrap(),
            DocumentOutcome::Denied(_)
        ));
        assert!(matches!(
            gateway
                .submit_nfe(&nfe_request(&issuer(), &reference))
                .await
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
            gateway.submit_nfe(&nfe_request(&issuer(), "x")).await,
            Err(GatewayError::Disabled)
        );
        assert_eq!(
            gateway.query_nfe(&issuer(), "x").await,
            Err(GatewayError::Disabled)
        );
    }
}
