use std::fmt;
use std::future::Future;

use conta_verde_domain::{Amount, CalendarDate, ChargeId, ChargeMethod, ChargePayment};

/// The headers of a webhook request, with their names in lower case: what a signature may be in.
#[derive(Debug, Clone, Default)]
pub struct WebhookHeaders(Vec<(String, String)>);

impl WebhookHeaders {
    pub fn new(headers: impl IntoIterator<Item = (String, String)>) -> Self {
        Self(
            headers
                .into_iter()
                .map(|(name, value)| (name.to_ascii_lowercase(), value))
                .collect(),
        )
    }

    /// The header with this name, whatever its case.
    pub fn get(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.0
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, value)| value.as_str())
    }
}

/// What a provider's notice says happened to a charge ([ADR 0046]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderEventKind {
    /// The customer paid: what, the day, and the fee the provider kept.
    Paid {
        paid_amount: Amount,
        paid_on: CalendarDate,
        fee: Amount,
    },
    Cancelled,
    Expired,
}

/// One thing a provider said, once its signature was checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEvent {
    /// The provider's own id for this notice: the same notice twice has the same id.
    pub event_id: String,
    /// The provider's id of the charge it is about.
    pub external_id: String,
    pub kind: ProviderEventKind,
}

/// Why a webhook request was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookError {
    /// The signature is missing or is not the provider's: nothing of the body is believed.
    InvalidSignature,
    /// Signed, but not a notice this adapter understands.
    Malformed(String),
}

/// Who is asked to pay, as the provider needs to know them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payer {
    pub name: String,
    /// The CPF or CNPJ, stored without punctuation.
    pub document: Option<String>,
    pub email: Option<String>,
}

/// What a provider is asked to make ([ADR 0045]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderChargeRequest {
    /// Our id of the charge, which the provider keeps as its reference so a webhook can be traced back.
    pub charge_id: ChargeId,
    pub method: ChargeMethod,
    pub amount: Amount,
    pub due_on: CalendarDate,
    pub payer: Payer,
    pub description: String,
}

/// What a provider made: its own id for the charge and what the customer needs to pay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCharge {
    pub external_id: String,
    pub payment: ChargePayment,
}

/// Why a provider did not do what it was asked. The text is for the server's log: it is never sent to
/// a client, since a provider's message can carry what the person should not see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// The operator has not configured a provider (`PAYMENT_PROVIDER`), so there is nobody to ask.
    Disabled,
    /// The provider answered and refused: a payer it cannot bill, an amount it does not take.
    Rejected(String),
    /// The provider did not give a usable answer: network, timeout, rate limit, error.
    Unavailable(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("no payment provider is configured"),
            Self::Rejected(reason) => write!(f, "the payment provider refused: {reason}"),
            Self::Unavailable(reason) => write!(f, "the payment provider is unavailable: {reason}"),
        }
    }
}

/// Port to a payment provider: the one place charges are made and called off ([ADR 0010], [ADR 0045]).
/// The provider keeps the money; this only asks it to collect and learns when it did. Futures are `Send`
/// so use cases can run on a multi-threaded runtime.
pub trait PaymentProvider: Send + Sync {
    /// The adapter's name as a charge records it: lower case letters, digits and `_`.
    fn key(&self) -> &str;

    /// Whether the operator configured one. When not, issuing a charge is refused before asking.
    fn is_enabled(&self) -> bool;

    fn create_charge(
        &self,
        request: &ProviderChargeRequest,
    ) -> impl Future<Output = Result<ProviderCharge, ProviderError>> + Send;

    /// Calls a charge off at the provider. Calling off one that is already off is not an error.
    fn cancel_charge(
        &self,
        external_id: &str,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;

    /// Checks the signature of a webhook over the body **as it came**, before anything of it is parsed, and
    /// reads the notices in it. Never calls the provider: what the body says is what the signature vouches for.
    fn verify_webhook(
        &self,
        headers: &WebhookHeaders,
        body: &[u8],
    ) -> Result<Vec<ProviderEvent>, WebhookError>;
}
