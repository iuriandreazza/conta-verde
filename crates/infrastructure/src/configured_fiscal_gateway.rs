use conta_verde_application::{
    AuthorizedNfse, DocumentOutcome, EventOutcome, EventQuery, FiscalGateway, GatewayError,
    NfceRequest, NfeRequest, NfseRequest, Submission,
};
use conta_verde_domain::FiscalSettings;

use crate::{FakeFiscalGateway, FocusNfeGateway, NfeIoGateway, NoFiscalGateway};

/// One of the fiscal gateways the product speaks to (ADR 0073, ADR 0075, ADR 0077): the one the operator chose with
/// `FISCAL_GATEWAY` (`fake` for development, `focus`, `nfeio`, off when it is not set), or one built for a company from its own
/// settings (ADR 0085).
#[derive(Clone)]
pub enum ConfiguredFiscalGateway {
    Off(NoFiscalGateway),
    Fake(FakeFiscalGateway),
    Focus(Box<FocusNfeGateway>),
    NfeIo(Box<NfeIoGateway>),
}

/// Calls the same method of whichever gateway is configured.
macro_rules! on_gateway {
    ($self:expr, $gateway:ident => $call:expr) => {
        match $self {
            ConfiguredFiscalGateway::Off($gateway) => $call,
            ConfiguredFiscalGateway::Fake($gateway) => $call,
            ConfiguredFiscalGateway::Focus($gateway) => $call,
            ConfiguredFiscalGateway::NfeIo($gateway) => $call,
        }
    };
}

impl FiscalGateway for ConfiguredFiscalGateway {
    fn key(&self) -> &str {
        on_gateway!(self, gateway => gateway.key())
    }

    fn is_enabled(&self) -> bool {
        on_gateway!(self, gateway => gateway.is_enabled())
    }

    async fn submit_nfe(&self, request: &NfeRequest) -> Result<Submission, GatewayError> {
        on_gateway!(self, gateway => gateway.submit_nfe(request).await)
    }

    async fn query_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway.query_nfe(issuer, reference).await)
    }

    async fn cancel_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway.cancel_nfe(issuer, reference, reason).await)
    }

    async fn correct_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        text: &str,
    ) -> Result<EventOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway.correct_nfe(issuer, reference, text).await)
    }

    async fn query_event(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        event: EventQuery,
    ) -> Result<EventOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway.query_event(issuer, reference, event).await)
    }

    async fn invalidate_nfe_numbers(
        &self,
        issuer: &FiscalSettings,
        series: u16,
        first: u32,
        last: u32,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway
            .invalidate_nfe_numbers(issuer, series, first, last, reason)
            .await)
    }

    fn issues_nfse(&self) -> bool {
        on_gateway!(self, gateway => gateway.issues_nfse())
    }

    fn issues_nfce(&self) -> bool {
        on_gateway!(self, gateway => gateway.issues_nfce())
    }

    fn issues_manifestation(&self) -> bool {
        on_gateway!(self, gateway => gateway.issues_manifestation())
    }

    async fn list_received_nfe(
        &self,
        issuer: &FiscalSettings,
        since_version: u64,
    ) -> Result<Vec<conta_verde_domain::ReceivedInvoiceDraft>, GatewayError> {
        on_gateway!(self, gateway => gateway.list_received_nfe(issuer, since_version).await)
    }

    async fn manifest_received_nfe(
        &self,
        issuer: &FiscalSettings,
        access_key: &conta_verde_domain::AccessKey,
        word: conta_verde_domain::Manifestation,
        justification: Option<&str>,
    ) -> Result<EventOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway
            .manifest_received_nfe(issuer, access_key, word, justification)
            .await)
    }

    async fn submit_nfce(&self, request: &NfceRequest) -> Result<Submission, GatewayError> {
        on_gateway!(self, gateway => gateway.submit_nfce(request).await)
    }

    async fn query_nfce(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway.query_nfce(issuer, reference).await)
    }

    async fn cancel_nfce(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway.cancel_nfce(issuer, reference, reason).await)
    }

    async fn submit_nfse(
        &self,
        request: &NfseRequest,
    ) -> Result<Submission<AuthorizedNfse>, GatewayError> {
        on_gateway!(self, gateway => gateway.submit_nfse(request).await)
    }

    async fn query_nfse(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome<AuthorizedNfse>, GatewayError> {
        on_gateway!(self, gateway => gateway.query_nfse(issuer, reference).await)
    }

    async fn cancel_nfse(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        on_gateway!(self, gateway => gateway.cancel_nfse(issuer, reference, reason).await)
    }
}
