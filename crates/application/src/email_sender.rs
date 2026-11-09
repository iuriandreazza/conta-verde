use std::future::Future;

use conta_verde_domain::Email;

use crate::EmailContent;

/// A message to one person: its text, and, when it was drawn in the layout of ADR 0068, the same message as HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailMessage {
    pub to: Email,
    pub subject: String,
    pub body: String,
    pub html: Option<String>,
}

impl EmailMessage {
    /// A message of text only: what a reader that shows no HTML gets, and what is kept when nothing draws the layout.
    pub fn from_content(to: Email, subject: String, content: &EmailContent) -> Self {
        Self {
            to,
            subject,
            body: content.to_text(),
            html: None,
        }
    }
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

    /// Sends what a use case says. By default it goes as text; [`BrandedEmailSender`] draws it in the layout.
    fn send_content(
        &self,
        to: Email,
        subject: String,
        content: &EmailContent,
    ) -> impl Future<Output = Result<(), EmailError>> + Send {
        async move {
            self.send(&EmailMessage::from_content(to, subject, content))
                .await
        }
    }
}

/// Draws the content of each message in the HTML layout before it goes on to the sender behind it, so a use case says
/// what to write and never how it looks, and the footer's link to the source code is set in one place.
pub struct BrandedEmailSender<S> {
    inner: S,
    source_url: String,
}

impl<S: EmailSender> BrandedEmailSender<S> {
    pub fn new(inner: S, source_url: String) -> Self {
        Self { inner, source_url }
    }
}

impl<S: EmailSender> EmailSender for BrandedEmailSender<S> {
    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    fn send(&self, message: &EmailMessage) -> impl Future<Output = Result<(), EmailError>> + Send {
        self.inner.send(message)
    }

    async fn send_content(
        &self,
        to: Email,
        subject: String,
        content: &EmailContent,
    ) -> Result<(), EmailError> {
        let message = EmailMessage {
            html: Some(content.to_html(&self.source_url)),
            ..EmailMessage::from_content(to, subject, content)
        };
        self.inner.send(&message).await
    }
}
