use conta_verde_application::{EmailError, EmailMessage, EmailSender};

/// Writes the message to the server's log and **sends nothing**, so it says so: its answer is that the
/// message was not sent, which is what makes the web page show the invitation's link to pass on
/// instead of claiming an e-mail went out. For an installation without a mail server, and for
/// development. The log then holds the link, which accepts nothing for anyone but the address it
/// names (ADR 0027), but is still not for the public.
#[derive(Clone, Copy, Default)]
pub struct LogEmailSender;

impl EmailSender for LogEmailSender {
    async fn send(&self, message: &EmailMessage) -> Result<(), EmailError> {
        tracing::info!(
            to = message.to.as_str(),
            subject = %message.subject,
            "e-mail not sent, only logged (no mail server is configured):\n{}",
            message.body
        );
        Err(EmailError)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_domain::Email;

    use super::*;

    #[tokio::test]
    async fn logging_is_not_sending_and_says_so() {
        let message = EmailMessage {
            to: Email::parse("bia@example.com").unwrap(),
            subject: "Convite".into(),
            body: "https://erp.example/invitations/abc".into(),
        };
        assert_eq!(LogEmailSender.send(&message).await, Err(EmailError));
    }
}
