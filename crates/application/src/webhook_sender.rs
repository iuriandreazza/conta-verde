use std::future::Future;

use conta_verde_domain::SecretText;
use uuid::Uuid;

fn valid_status(status: u16) -> Option<u16> {
    (100..=599).contains(&status).then_some(status)
}

/// What a delivery says when it is sent: the adapter signs it with the secrets and posts it ([ADR 0088]).
pub struct Outgoing<'a> {
    pub url: &'a str,
    /// The event's id, the same on every try and for every subscription.
    pub event_id: Uuid,
    /// Unix seconds, new on every try: a receiver refuses one that is too old.
    pub timestamp: i64,
    pub body: &'a str,
    /// The secrets that sign: the current one and, while it still counts, the one that was replaced.
    pub secrets: &'a [SecretText],
}

impl std::fmt::Debug for Outgoing<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outgoing")
            .field("url", &self.url)
            .field("event_id", &self.event_id)
            .field("secrets", &"<hidden>")
            .finish_non_exhaustive()
    }
}

/// How a try went. The subscriber's answer is never read: only whether it was a success and its status code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendOutcome {
    /// The status code, when the subscriber answered.
    pub status: Option<u16>,
    /// Why it failed, in a few words, when it did: a refused address, a timeout, a status that is not a success.
    pub error: Option<String>,
}

impl SendOutcome {
    pub fn delivered(status: u16) -> Self {
        Self {
            status: valid_status(status),
            error: None,
        }
    }

    pub fn answered(status: u16) -> Self {
        match valid_status(status) {
            Some(status) => Self {
                status: Some(status),
                error: Some(format!("the subscriber answered {status}")),
            },
            // A code outside 100 to 599 is not HTTP: the log has no place for it, and the try still counts.
            None => Self::failed("the subscriber answered with a status that is not HTTP"),
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            status: None,
            error: Some(error.into()),
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// Port to what posts a signed delivery to a subscriber. The real adapter refuses a target that is not on the public internet
/// (checked at every try, at the address it connects to), follows no redirect and reads no answer.
pub trait WebhookSender: Send + Sync {
    fn send(&self, outgoing: &Outgoing<'_>) -> impl Future<Output = SendOutcome> + Send;
}
