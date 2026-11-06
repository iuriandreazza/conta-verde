use std::future::Future;

/// A text message to one phone number ([ADR 0014], [ADR 0048]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMessage {
    /// The number as digits with the country code and no `+` (`5581999990000`).
    pub to: String,
    pub text: String,
}

/// Why a message could not be sent. Adapters log the cause; callers only need to know it was not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageError;

/// Port for sending text messages by WhatsApp through a provider ([ADR 0014]). Like `EmailSender`, a send
/// that fails never fails what asked for it.
pub trait MessageSender: Send + Sync {
    /// Whether the operator configured one. When not, reminders by this channel are not tried.
    fn is_enabled(&self) -> bool;

    fn send(&self, message: &TextMessage) -> impl Future<Output = Result<(), MessageError>> + Send;
}
