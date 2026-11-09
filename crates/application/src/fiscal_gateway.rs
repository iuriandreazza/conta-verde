use std::fmt;
use std::future::Future;

use conta_verde_domain::{AccessKey, Address, Amount, Decimal, FiscalSettings};

/// Where the goods of an operation go, as the NF-e's layout asks (`local_destino`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    InState,
    Interstate,
    Foreign,
}

/// How the buyer was present, as the NF-e's layout codes it (`presenca_comprador`). The person chooses it: which one
/// suits an operation is a fiscal fact the product does not know (verify with an accountant, [ADR 0075]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyerPresence {
    NotApplicable,
    InPerson,
    Internet,
    Telephone,
    HomeDelivery,
    OutsideTheEstablishment,
    Other,
}

impl BuyerPresence {
    pub fn code(self) -> u8 {
        match self {
            Self::NotApplicable => 0,
            Self::InPerson => 1,
            Self::Internet => 2,
            Self::Telephone => 3,
            Self::HomeDelivery => 4,
            Self::OutsideTheEstablishment => 5,
            Self::Other => 9,
        }
    }
}

/// How a document was paid, by the layout's two-digit table. Only the codes the authors are sure of are here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentForm {
    Cash,
    Cheque,
    CreditCard,
    DebitCard,
    StoreCredit,
    BankSlip,
    BankDeposit,
    Pix,
    Transfer,
    NoPayment,
    Other,
}

impl PaymentForm {
    pub fn code(self) -> &'static str {
        match self {
            Self::Cash => "01",
            Self::Cheque => "02",
            Self::CreditCard => "03",
            Self::DebitCard => "04",
            Self::StoreCredit => "05",
            Self::BankSlip => "15",
            Self::BankDeposit => "16",
            Self::Pix => "17",
            Self::Transfer => "18",
            Self::NoPayment => "90",
            Self::Other => "99",
        }
    }
}

/// Who receives the goods, as the NF-e says them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfeRecipient {
    pub name: String,
    /// The CPF or CNPJ, stored without punctuation.
    pub document: Option<String>,
    pub email: Option<String>,
    pub address: Option<Address>,
    /// The recipient's state registration when it is a taxpayer of the ICMS.
    pub state_registration: Option<String>,
}

/// One item of an NF-e: what is sold and how it is classified. The tax **values** (bases, amounts) are not here: for
/// the Simples Nacional the classification is all there is, and the calculation for the regular regimes comes with
/// slice 8 ([ADR 0073]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfeItem {
    pub number: u16,
    pub code: String,
    pub description: String,
    pub ncm: String,
    pub cfop: String,
    pub unit: String,
    /// At most three decimals.
    pub quantity: Decimal,
    /// At most six decimals.
    pub unit_price: Decimal,
    pub gross: Amount,
    pub discount: Amount,
    /// The product's origin, 0 to 8.
    pub origin: u8,
    /// The CSOSN (Simples Nacional) or the CST.
    pub icms_code: String,
    pub pis_code: String,
    pub cofins_code: String,
}

/// An NF-e to be authorized. The issuer is the company's fiscal settings **as they are now**: a snapshot the
/// document keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfeRequest {
    /// Our id of the document, which the gateway keeps as its reference: asking again with it is safe.
    pub reference: String,
    pub issuer: FiscalSettings,
    /// What the operation is called, as printed on the document.
    pub nature: String,
    /// The moment of issue, in Unix seconds: the caller's clock, so a test moves it.
    pub issued_at_unix: i64,
    pub destination: Destination,
    pub final_consumer: bool,
    pub presence: BuyerPresence,
    pub recipient: NfeRecipient,
    pub items: Vec<NfeItem>,
    pub payments: Vec<(PaymentForm, Amount)>,
    pub total_products: Amount,
    pub total_discount: Amount,
    pub total: Amount,
}

/// A document the tax authority authorized, with its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedDocument {
    pub access_key: AccessKey,
    pub number: u32,
    pub series: u16,
    pub protocol: Option<String>,
    pub xml: Vec<u8>,
    /// The printed form (the DANFE), when the gateway made one.
    pub pdf: Option<Vec<u8>>,
}

/// Why the authority or the gateway refused, as the person must read it to fix the document: the code and the text,
/// cut to a length a row can hold and without control characters ([ADR 0073]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: String,
    pub message: String,
}

const MAX_REFUSAL_CHARS: usize = 500;

impl Refusal {
    pub fn new(code: &str, message: &str) -> Self {
        let clean = |text: &str, max: usize| -> String {
            text.chars()
                .filter(|c| !c.is_control())
                .take(max)
                .collect::<String>()
                .trim()
                .to_owned()
        };
        Self {
            code: clean(code, 50),
            message: clean(message, MAX_REFUSAL_CHARS),
        }
    }
}

/// What the gateway says about a document at this moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentOutcome {
    /// Sent to the authority, not answered yet: ask again.
    Processing,
    Authorized(AuthorizedDocument),
    /// Issued while the authority was out, and still waiting to be authorized: ask again.
    Contingency,
    Rejected(Refusal),
    /// The authority denied it for a reason about the issuer or the recipient: final.
    Denied(Refusal),
    /// The document was authorized and then cancelled; the XML of the cancellation.
    Cancelled {
        document: AuthorizedDocument,
        cancellation_xml: Vec<u8>,
    },
    /// The gateway has no document with this reference.
    Unknown,
}

/// A cancellation, a correction letter or an invalidation that the authority took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedEvent {
    pub protocol: Option<String>,
    /// The correction letter's sequential number, for a correction.
    pub number: Option<u32>,
    pub xml: Option<Vec<u8>>,
    pub pdf: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventOutcome {
    Accepted(AcceptedEvent),
    Refused(Refusal),
}

/// Why a call did not give an answer. The text is for the server's log: it is never sent to a client, and it never
/// holds a body (a body carries the customer's data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayError {
    /// The operator has not configured a gateway (`FISCAL_GATEWAY`), so there is nobody to ask.
    Disabled,
    /// The gateway is configured for another CNPJ: a document never goes out under another company's token.
    WrongIssuer,
    /// The gateway does not do this (yet) for this document: the regime needs the tax calculation of slice 8.
    Unsupported(&'static str),
    /// No usable answer: network, timeout, the token refused, an error of the gateway, a reply this adapter does
    /// not understand. Asking again later is the remedy.
    Unavailable(String),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("no fiscal gateway is configured"),
            Self::WrongIssuer => {
                f.write_str("the fiscal gateway is configured for another CNPJ than the company's")
            }
            Self::Unsupported(what) => write!(f, "the fiscal gateway does not support {what}"),
            Self::Unavailable(reason) => write!(f, "the fiscal gateway is unavailable: {reason}"),
        }
    }
}

/// Port to a fiscal gateway: the one place fiscal documents are authorized, cancelled, corrected and numbers
/// invalidated ([ADR 0006], [ADR 0073], [ADR 0075]). The gateway signs with the company's certificate, which it keeps,
/// talks to the tax authority and **assigns the numbers**; this only asks and learns. Every call is about the issuer in
/// the settings it is given. Futures are `Send` so use cases can run on a multi-threaded runtime. The port grows by
/// slice: NF-e here, NFS-e and NFC-e with theirs.
pub trait FiscalGateway: Send + Sync {
    /// The adapter's name: lower case letters, digits and `_`.
    fn key(&self) -> &str;

    /// Whether the operator configured one. When not, issuing is refused before asking.
    fn is_enabled(&self) -> bool;

    /// Asks the gateway to authorize an NF-e. The reference makes it idempotent: the same reference again gives the
    /// state of the document it made, never a second one.
    fn submit_nfe(
        &self,
        request: &NfeRequest,
    ) -> impl Future<Output = Result<DocumentOutcome, GatewayError>> + Send;

    /// The state of the document with this reference.
    fn query_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> impl Future<Output = Result<DocumentOutcome, GatewayError>> + Send;

    /// Cancels an authorized NF-e. The deadline is the authority's and the gateway's, never ours.
    ///
    /// The events (this, the correction letter and the invalidation) are **not idempotent by reference**: after an
    /// `Unavailable` the caller does not know whether the authority took it. It asks `query_nfe` first (a cancelled
    /// document says so) and for the others records what it sent before sending, so it never repeats one blindly.
    fn cancel_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> impl Future<Output = Result<EventOutcome, GatewayError>> + Send;

    /// Sends a correction letter (CC-e) for an authorized NF-e.
    fn correct_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        text: &str,
    ) -> impl Future<Output = Result<EventOutcome, GatewayError>> + Send;

    /// Declares a range of numbers of a series unused (inutilização).
    fn invalidate_nfe_numbers(
        &self,
        issuer: &FiscalSettings,
        series: u16,
        first: u32,
        last: u32,
        reason: &str,
    ) -> impl Future<Output = Result<EventOutcome, GatewayError>> + Send;
}
