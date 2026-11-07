use std::future::Future;

use conta_verde_domain::Email;

/// A message to one person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailMessage {
    pub to: Email,
    pub subject: String,
    pub body: String,
}

/// Why a message could not be sent. Adapters log the cause; callers only need to know it was not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailError;

/// Port for sending e-mail ([ADR 0014]). A send that fails must never fail what asked for it: the
/// invitation exists, and its link is shown to the person who made it.
pub trait EmailSender: Send + Sync {
    /// Whether a message sent here reaches anyone: false for an adapter that only logs, which is what makes a page
    /// show an invitation's link to pass on.
    fn is_enabled(&self) -> bool {
        true
    }

    fn send(&self, message: &EmailMessage) -> impl Future<Output = Result<(), EmailError>> + Send;
}
