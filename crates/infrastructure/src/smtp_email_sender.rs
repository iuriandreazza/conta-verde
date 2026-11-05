use std::time::Duration;

use conta_verde_application::{EmailError, EmailMessage, EmailSender};
use lettre::message::{Mailbox, header::ContentType};
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

// A request waits on the send, so a mail server that does not answer must not hold it for long.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// `smtp://` without `?tls=required` talks in the clear (the library does not even try STARTTLS), so a
/// URL that carries credentials and does not encrypt would send the password to the network as
/// typed. That is refused unless the server is this machine, where nothing travels: a provider's
/// instructions pasted as `smtp://user:pw@host:587` must not start and work.
fn refuse_a_password_in_the_clear(url: &str) -> Result<(), String> {
    let Some((scheme, rest)) = url.trim().split_once("://") else {
        return Ok(()); // not a URL: the transport says so, in its own words
    };
    let (location, query) = rest.split_once('?').unwrap_or((rest, ""));
    let authority = location.split('/').next().unwrap_or_default();
    let Some((_credentials, host_and_port)) = authority.rsplit_once('@') else {
        return Ok(()); // no credentials, nothing secret to protect
    };
    let host = host_and_port
        .strip_prefix('[')
        .and_then(|bracketed| bracketed.split_once(']').map(|(host, _)| host))
        .unwrap_or_else(|| host_and_port.split(':').next().unwrap_or_default());
    let encrypted =
        scheme.eq_ignore_ascii_case("smtps") || query.split('&').any(|pair| pair == "tls=required");
    let this_machine = matches!(host, "localhost" | "127.0.0.1" | "::1");
    if encrypted || this_machine {
        Ok(())
    } else {
        Err(
            "SMTP_URL carries a password but does not encrypt the connection: \
             use smtps://user:password@host:465 or smtp://user:password@host:587?tls=required"
                .to_owned(),
        )
    }
}

/// Sends plain-text e-mail through an SMTP server ([ADR 0014]). The server is given as a URL, which
/// says how to reach it and how to trust it: `smtp://host:1025` talks in the clear (for a local
/// mail catcher), `smtp://user:password@host:587?tls=required` upgrades the connection with
/// STARTTLS and refuses to go on without it, and `smtps://user:password@host:465` is TLS from the
/// first byte. **A server on the internet needs one of the last two**, or the password and the
/// message travel in the clear.
pub struct SmtpEmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    timeout: Duration,
}

impl SmtpEmailSender {
    /// `from` is who the messages come from, like `conta-verde <no-reply@example.com>`.
    pub fn new(url: &str, from: &str) -> Result<Self, String> {
        Self::with_timeout(url, from, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(url: &str, from: &str, timeout: Duration) -> Result<Self, String> {
        refuse_a_password_in_the_clear(url)?;
        let from: Mailbox = from
            .parse()
            .map_err(|error| format!("SMTP_FROM is not an address ({error})"))?;
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
            // The URL carries the password: it is never put in the message.
            .map_err(|_| "SMTP_URL is not an SMTP URL (smtp:// or smtps://host:port)".to_owned())?
            .timeout(Some(timeout))
            .build();
        Ok(Self {
            transport,
            from,
            timeout,
        })
    }

    fn build(&self, message: &EmailMessage) -> Result<Message, String> {
        let to: Address = message
            .to
            .as_str()
            .parse()
            .map_err(|error| format!("the recipient is not a sendable address ({error})"))?;
        Message::builder()
            .from(self.from.clone())
            .to(Mailbox::new(None, to))
            .subject(message.subject.clone())
            .header(ContentType::TEXT_PLAIN)
            .body(message.body.clone())
            .map_err(|error| format!("the message could not be built ({error})"))
    }
}

impl EmailSender for SmtpEmailSender {
    async fn send(&self, message: &EmailMessage) -> Result<(), EmailError> {
        let built = self.build(message).map_err(|cause| {
            tracing::warn!(to = message.to.as_str(), %cause, "e-mail not sent");
            EmailError
        })?;
        // The transport's own timeout only bounds opening the TCP connection: not the greeting, the TLS
        // handshake or any command (a server that takes the connection and says nothing held a send
        // for as long as it liked), so the whole send is bounded here, and this is the real limit.
        let sent = tokio::time::timeout(self.timeout, self.transport.send(built)).await;
        match sent {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => {
                // The body, which holds a link, is never logged.
                tracing::warn!(to = message.to.as_str(), %error, "e-mail not sent");
                Err(EmailError)
            }
            Err(_) => {
                tracing::warn!(
                    to = message.to.as_str(),
                    "e-mail not sent: the server did not answer in time"
                );
                Err(EmailError)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use conta_verde_domain::Email;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    use super::*;

    /// What a server saw of one conversation.
    #[derive(Default, Clone)]
    struct Seen {
        mail_from: Vec<String>,
        rcpt_to: Vec<String>,
        data: Vec<String>,
    }

    /// How the fake server answers.
    #[derive(Clone, Copy)]
    enum Mood {
        Accepting,
        /// Refuses every recipient, like a server that does not relay.
        RefusingRecipients,
        /// Takes the connection and never says a word.
        Silent,
    }

    /// A local stand-in for a mail server: just enough SMTP to take one message, so the tests never
    /// reach a real one.
    async fn serve(mood: Mood) -> (u16, Arc<Mutex<Seen>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Seen::default()));
        let record = seen.clone();
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let record = record.clone();
                tokio::spawn(async move {
                    if matches!(mood, Mood::Silent) {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        return;
                    }
                    let (reader, mut writer) = socket.into_split();
                    let mut lines = BufReader::new(reader).lines();
                    writer.write_all(b"220 fake ESMTP\r\n").await.unwrap();
                    let mut in_data = false;
                    let mut data = String::new();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if in_data {
                            if line == "." {
                                in_data = false;
                                record.lock().unwrap().data.push(std::mem::take(&mut data));
                                writer.write_all(b"250 queued\r\n").await.unwrap();
                            } else {
                                data.push_str(&line);
                                data.push_str("\r\n");
                            }
                            continue;
                        }
                        let upper = line.to_ascii_uppercase();
                        let reply: &[u8] = if upper.starts_with("EHLO") || upper.starts_with("HELO")
                        {
                            b"250 fake\r\n"
                        } else if upper.starts_with("MAIL FROM") {
                            record.lock().unwrap().mail_from.push(line.clone());
                            b"250 ok\r\n"
                        } else if upper.starts_with("RCPT TO") {
                            record.lock().unwrap().rcpt_to.push(line.clone());
                            match mood {
                                Mood::RefusingRecipients => b"550 relay denied\r\n",
                                _ => b"250 ok\r\n",
                            }
                        } else if upper.starts_with("DATA") {
                            in_data = true;
                            b"354 go ahead\r\n"
                        } else if upper.starts_with("QUIT") {
                            let _ = writer.write_all(b"221 bye\r\n").await;
                            return;
                        } else {
                            b"250 ok\r\n"
                        };
                        writer.write_all(reply).await.unwrap();
                    }
                });
            }
        });
        (port, seen)
    }

    fn message() -> EmailMessage {
        EmailMessage {
            to: Email::parse("bia@example.com").unwrap(),
            subject: "Ana convidou você para Padaria no conta-verde".into(),
            body: "Você foi convidado. Para aceitar, abra o link:\nhttps://erp.example/invitations/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n".into(),
        }
    }

    /// Quoted-printable undone, which is how a message with accents may travel.
    fn unquote(data: &str) -> String {
        let joined = data.replace("=\r\n", "");
        let mut bytes = Vec::new();
        let raw = joined.as_bytes();
        let mut index = 0;
        while index < raw.len() {
            let escaped = raw
                .get(index + 1..index + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            if raw[index] == b'='
                && let Some(byte) = escaped
            {
                bytes.push(byte);
                index += 3;
            } else {
                bytes.push(raw[index]);
                index += 1;
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// The Subject header with its RFC 2047 encoded words (`=?utf-8?b?...?=` and `?q?`) decoded.
    fn decode_subject(data: &str) -> String {
        let mut header = String::new();
        let mut lines = data.split("\r\n");
        while let Some(line) = lines.next() {
            if let Some(first) = line.strip_prefix("Subject:") {
                header.push_str(first.trim());
                for folded in lines.by_ref() {
                    match folded
                        .strip_prefix(' ')
                        .or_else(|| folded.strip_prefix('\t'))
                    {
                        Some(more) => header.push_str(more.trim()),
                        None => break,
                    }
                }
                break;
            }
        }
        // Plain words keep the space around them; between two encoded words it is dropped (RFC 2047).
        let mut decoded = String::new();
        let mut after_encoded_word = false;
        for word in header.split_whitespace() {
            let parts: Vec<&str> = word
                .trim_start_matches("=?")
                .trim_end_matches("?=")
                .splitn(3, '?')
                .collect();
            let text = match parts.as_slice() {
                [_charset, encoding, text]
                    if word.starts_with("=?") && encoding.eq_ignore_ascii_case("b") =>
                {
                    Some(String::from_utf8_lossy(&base64_decode(text)).into_owned())
                }
                [_charset, encoding, text]
                    if word.starts_with("=?") && encoding.eq_ignore_ascii_case("q") =>
                {
                    Some(unquote(&text.replace('_', " ")))
                }
                _ => None,
            };
            if !(after_encoded_word && text.is_some()) && !decoded.is_empty() {
                decoded.push(' ');
            }
            after_encoded_word = text.is_some();
            decoded.push_str(text.as_deref().unwrap_or(word));
        }
        decoded
    }

    fn base64_decode(text: &str) -> Vec<u8> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bytes = Vec::new();
        let (mut buffer, mut bits) = (0u32, 0);
        for symbol in text.bytes().filter(|byte| *byte != b'=') {
            let value = ALPHABET.iter().position(|c| *c == symbol).unwrap() as u32;
            buffer = (buffer << 6) | value;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                bytes.push((buffer >> bits) as u8);
                buffer &= (1 << bits) - 1;
            }
        }
        bytes
    }

    fn sender(port: u16) -> SmtpEmailSender {
        SmtpEmailSender::with_timeout(
            &format!("smtp://127.0.0.1:{port}"),
            "conta-verde <no-reply@erp.example>",
            // Roomy: a loaded CI runner must not turn a slow exchange into a failure. The silent
            // server is still given up on at this.
            Duration::from_secs(2),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn delivers_the_message_from_the_configured_address_to_the_invited_one() {
        let (port, seen) = serve(Mood::Accepting).await;
        sender(port).send(&message()).await.unwrap();

        let seen = seen.lock().unwrap().clone();
        assert!(
            seen.mail_from[0].contains("no-reply@erp.example"),
            "{:?}",
            seen.mail_from
        );
        assert!(
            seen.rcpt_to[0].contains("bia@example.com"),
            "{:?}",
            seen.rcpt_to
        );
        assert_eq!(seen.data.len(), 1);
        let data = &seen.data[0];
        assert!(
            data.contains("From: conta-verde <no-reply@erp.example>"),
            "{data}"
        );
        assert!(data.contains("To: bia@example.com"), "{data}");
        assert!(data.contains("text/plain"), "{data}");
        // The link survives whatever encoding the body travelled in, and so do the accents.
        let body = unquote(data);
        assert!(body.contains("https://erp.example/invitations/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"), "{body}");
        assert!(
            body.contains("Você foi convidado. Para aceitar, abra o link:"),
            "{body}"
        );
        // The subject has accents, so it travels as RFC 2047 encoded words: they round-trip.
        assert_eq!(
            decode_subject(data),
            "Ana convidou você para Padaria no conta-verde",
            "{data}"
        );
    }

    #[tokio::test]
    async fn a_server_that_refuses_the_recipient_is_an_error_not_a_success() {
        let (port, _) = serve(Mood::RefusingRecipients).await;
        assert_eq!(sender(port).send(&message()).await, Err(EmailError));
    }

    #[tokio::test]
    async fn a_server_that_does_not_answer_is_given_up_on_at_the_timeout() {
        let (port, _) = serve(Mood::Silent).await;
        let started = std::time::Instant::now();
        assert_eq!(sender(port).send(&message()).await, Err(EmailError));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn an_unreachable_server_is_an_error() {
        // A port that was just free and has nobody on it: the connection is refused.
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().port()
        };
        assert_eq!(sender(port).send(&message()).await, Err(EmailError));
    }

    #[test]
    fn a_wrong_setting_is_refused_at_startup_without_echoing_the_password() {
        let from = "conta-verde <no-reply@erp.example>";
        for url in ["not a url", "http://host", ""] {
            assert!(SmtpEmailSender::new(url, from).is_err(), "{url}");
        }
        // A password is never echoed, whatever is wrong with the URL.
        for url in [
            "smtp://user:secret-pw@",
            "smtp://user:secret-pw@host:bad",
            "smtp://:secret-pw@/",
        ] {
            let error = SmtpEmailSender::new(url, from).err().expect(url);
            assert!(!error.contains("secret-pw"), "{error}");
        }
        for bad_from in ["", "no-at-sign", "<>"] {
            assert!(
                SmtpEmailSender::new("smtp://localhost:1025", bad_from).is_err(),
                "{bad_from}"
            );
        }
        assert!(SmtpEmailSender::new("smtp://localhost:1025", from).is_ok());
        // A server that is not this machine does not get a password in the clear, in any spelling.
        for url in [
            "smtp://user:pw@mail.example:587",
            "smtp://user:pw@mail.example",
            "smtp://user:pw@mail.example:587?tls=opportunistic",
            "smtp://user:pw@10.0.0.5:25",
        ] {
            let error = SmtpEmailSender::new(url, from).err().expect(url);
            assert!(error.contains("does not encrypt"), "{url}: {error}");
            assert!(!error.contains("pw@"), "{error}");
        }
        // ... but this machine may have one (a catcher that wants a login), and without a password
        // nothing secret travels.
        assert!(SmtpEmailSender::new("smtp://user:pw@localhost:1025", from).is_ok());
        assert!(SmtpEmailSender::new("smtp://user:pw@127.0.0.1:1025", from).is_ok());
        assert!(SmtpEmailSender::new("smtp://mail.example:25", from).is_ok());
        assert!(SmtpEmailSender::new("smtps://user:pw@mail.example:465", from).is_ok());
        assert!(SmtpEmailSender::new("smtp://user:pw@mail.example:587?tls=required", from).is_ok());
    }
}
