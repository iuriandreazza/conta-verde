use std::future::Future;

use conta_verde_domain::{CalendarDate, CompanyId};

/// An account the aggregator can see inside a connection the person made with their bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatedAccount {
    pub id: String,
    pub name: String,
    /// What the aggregator calls it (`CHECKING_ACCOUNT`, `CREDIT_CARD`).
    pub kind: String,
    pub number: Option<String>,
}

/// One posted movement of an account, as the aggregator says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatedTransaction {
    /// The aggregator's own id for it: it stands for the bank's reference in the statement line, so a pull that overlaps the
    /// last one adds nothing ([ADR 0034]).
    pub id: String,
    pub posted_on: CalendarDate,
    /// A signed decimal with at most two decimals: what came in is positive, what went out is negative.
    pub amount: String,
    pub description: String,
}

/// Where the bank's connection stands at the aggregator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemState {
    Ready,
    /// The bank needs the person (a login that failed, a consent to renew): the reason, in a few words.
    NeedsAttention(String),
}

/// Why a call to an aggregator did not give what was asked. The text is for the log and the page, in a few words, and **never
/// what the aggregator answered**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregatorError {
    /// The installation has no aggregator configured.
    Disabled,
    /// The aggregator did not answer, or answered with an error that is worth trying again.
    Unavailable(String),
    /// The aggregator refused the request (its credentials, an id that is not its own): trying again changes nothing.
    Refused(String),
}

impl std::fmt::Display for AggregatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => f.write_str("no bank aggregator is configured"),
            Self::Unavailable(reason) => write!(f, "the aggregator is unavailable: {reason}"),
            Self::Refused(reason) => write!(f, "the aggregator refused the request: {reason}"),
        }
    }
}

impl std::error::Error for AggregatorError {}

/// Port to a bank aggregator ([ADR 0011], [ADR 0089]): what connects a company's accounts at its banks and pulls their
/// statements. **Its credentials are the installation's**, in the environment, and never stored per company; what is stored is a
/// connection record. Every method says what could not be verified without an account in the adapter that implements it.
pub trait BankAggregator: Send + Sync {
    /// The key a connection names it by (`pluggy`).
    fn provider(&self) -> &'static str;

    /// Whether anything is configured: with only the off adapter nothing is connected or pulled.
    fn is_enabled(&self) -> bool;

    /// A short-lived token for the aggregator's consent widget in the browser, which gives back the id of the connection the
    /// person makes (`item`: the id of one to renew, when there is one). The company is passed so the aggregator can tell
    /// whose it is.
    fn connect_token(
        &self,
        company_id: CompanyId,
        item: Option<&str>,
    ) -> impl Future<Output = Result<String, AggregatorError>> + Send;

    /// The accounts inside a connection.
    fn accounts(
        &self,
        item: &str,
    ) -> impl Future<Output = Result<Vec<AggregatedAccount>, AggregatorError>> + Send;

    fn item_state(
        &self,
        item: &str,
    ) -> impl Future<Output = Result<ItemState, AggregatorError>> + Send;

    /// The posted movements of an account from a day through a day, both included, all pages of them.
    fn transactions(
        &self,
        account: &str,
        from: CalendarDate,
        to: CalendarDate,
    ) -> impl Future<Output = Result<Vec<AggregatedTransaction>, AggregatorError>> + Send;

    /// Ends the connection at the aggregator.
    fn disconnect(&self, item: &str) -> impl Future<Output = Result<(), AggregatorError>> + Send;
}
