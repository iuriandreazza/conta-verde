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
    fn send(&self, message: &EmailMessage) -> impl Future<Output = Result<(), EmailError>> + Send;
}
