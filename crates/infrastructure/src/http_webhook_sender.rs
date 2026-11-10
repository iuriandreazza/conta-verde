use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use conta_verde_application::{Outgoing, SendOutcome, WebhookSender};
use conta_verde_domain::{is_blocked_address, split_host_port};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// How long a subscriber has to answer before the try is a failure.
const TIMEOUT: Duration = Duration::from_secs(10);
/// How long a name has to resolve.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// The `Webhook-Signature` value for one secret: `v1,` and the base64 of the HMAC-SHA256 of `"{id}.{timestamp}.{body}"`, the
/// scheme of the Standard Webhooks specification (its libraries verify it, with the secret's own text as the key). While a
/// replaced secret still counts, the header carries one such value for each, separated by a space.
pub fn webhook_signature(secret: &str, event_id: &str, timestamp: i64, body: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes a key of any length");
    mac.update(format!("{event_id}.{timestamp}.{body}").as_bytes());
    format!("v1,{}", base64(&mac.finalize().into_bytes()))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, byte)| {
            acc | (u32::from(*byte) << (16 - 8 * i))
        });
        for position in 0..4 {
            if position <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * position) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The header values of a delivery, in the order they are sent.
pub fn webhook_headers(outgoing: &Outgoing<'_>) -> Vec<(&'static str, String)> {
    let id = outgoing.event_id.to_string();
    let signature = outgoing
        .secrets
        .iter()
        .map(|secret| webhook_signature(secret.expose(), &id, outgoing.timestamp, outgoing.body))
        .collect::<Vec<_>>()
        .join(" ");
    vec![
        ("webhook-id", id),
        ("webhook-timestamp", outgoing.timestamp.to_string()),
        ("webhook-signature", signature),
    ]
}

/// Posts a delivery to its subscriber over HTTP ([ADR 0088]). The target is checked **at every try**, at the address the
/// connection is made to: the name is resolved here, **every** address it resolves to must be public, and the connection goes to
/// the one that was checked (never resolved again, so a name that changes its answer between the check and the call changes
/// nothing). No redirect is followed, no proxy of the environment is used, and the answer is never read: only its status.
///
/// `allow_private_targets` is the operator's development and test opt-in: it lets a listener on this machine be the target.
pub struct HttpWebhookSender {
    allow_private_targets: bool,
}

impl HttpWebhookSender {
    pub fn new(allow_private_targets: bool) -> Self {
        Self {
            allow_private_targets,
        }
    }

    /// The address to connect to, or why not.
    async fn resolve(&self, host: &str, port: u16) -> Result<SocketAddr, &'static str> {
        let addresses: Vec<SocketAddr> = match host.parse::<IpAddr>() {
            Ok(ip) => vec![SocketAddr::new(ip, port)],
            Err(_) => tokio::time::timeout(RESOLVE_TIMEOUT, tokio::net::lookup_host((host, port)))
                .await
                .map_err(|_| "the name did not resolve in time")?
                .map_err(|_| "the name did not resolve")?
                .collect(),
        };
        let first = *addresses.first().ok_or("the name did not resolve")?;
        if !self.allow_private_targets
            && addresses
                .iter()
                .any(|address| is_blocked_address(address.ip()))
        {
            return Err("the address is not on the public internet");
        }
        Ok(first)
    }
}

impl WebhookSender for HttpWebhookSender {
    async fn send(&self, outgoing: &Outgoing<'_>) -> SendOutcome {
        let Some((scheme, rest)) = outgoing.url.split_once("://") else {
            return SendOutcome::failed("the address is not a URL");
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let Some((host, port)) = split_host_port(authority) else {
            return SendOutcome::failed("the address is not a URL");
        };
        let default_port = if scheme == "https" { 443 } else { 80 };
        let port = port.and_then(|p| p.parse().ok()).unwrap_or(default_port);
        if scheme != "https" && !self.allow_private_targets {
            return SendOutcome::failed("the address is not https");
        }
        let address = match self.resolve(host, port).await {
            Ok(address) => address,
            Err(reason) => return SendOutcome::failed(reason),
        };
        let client = match reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(TIMEOUT)
            .user_agent("conta-verde-webhooks/1")
            // The connection goes to the address that was checked, whatever the name says now.
            .resolve(host, address)
            .build()
        {
            Ok(client) => client,
            Err(_) => return SendOutcome::failed("the client could not be made"),
        };
        let mut request = client
            .post(outgoing.url)
            .header("content-type", "application/json")
            .body(outgoing.body.to_owned());
        for (name, value) in webhook_headers(outgoing) {
            request = request.header(name, value);
        }
        match request.send().await {
            Ok(response) if response.status().is_success() => {
                SendOutcome::delivered(response.status().as_u16())
            }
            Ok(response) => SendOutcome::answered(response.status().as_u16()),
            Err(error) if error.is_timeout() => SendOutcome::failed("timed out"),
            Err(error) if error.is_connect() => SendOutcome::failed("could not connect"),
            Err(_) => SendOutcome::failed("the request failed"),
        }
    }
}

/// What a delivery looked like when the fake sender got it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub url: String,
    pub event_id: uuid::Uuid,
    pub timestamp: i64,
    pub body: String,
    /// The `Webhook-Signature` header it would have carried.
    pub signature: String,
}

/// A sender for tests: it records what it was given and answers what it was told to, `delivered(200)` when nothing is left.
#[derive(Default, Clone)]
pub struct InMemoryWebhookSender {
    sent: Arc<Mutex<Vec<Sent>>>,
    outcomes: Arc<Mutex<Vec<SendOutcome>>>,
}

impl InMemoryWebhookSender {
    /// The answers it gives, one for each try, in order.
    pub fn answering(outcomes: Vec<SendOutcome>) -> Self {
        let mut outcomes = outcomes;
        outcomes.reverse();
        Self {
            sent: Arc::default(),
            outcomes: Arc::new(Mutex::new(outcomes)),
        }
    }

    pub fn sent(&self) -> Vec<Sent> {
        self.sent.lock().expect("sender lock poisoned").clone()
    }
}

impl WebhookSender for InMemoryWebhookSender {
    async fn send(&self, outgoing: &Outgoing<'_>) -> SendOutcome {
        let signature = webhook_headers(outgoing)
            .into_iter()
            .find(|(name, _)| *name == "webhook-signature")
            .map(|(_, value)| value)
            .unwrap_or_default();
        self.sent.lock().expect("sender lock poisoned").push(Sent {
            url: outgoing.url.to_owned(),
            event_id: outgoing.event_id,
            timestamp: outgoing.timestamp,
            body: outgoing.body.to_owned(),
            signature,
        });
        self.outcomes
            .lock()
            .expect("sender lock poisoned")
            .pop()
            .unwrap_or_else(|| SendOutcome::delivered(200))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_domain::SecretText;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn base64_is_the_standard_alphabet_with_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn the_signature_is_v1_and_the_base64_of_the_hmac_of_id_timestamp_and_body() {
        // The vector of the Standard Webhooks specification's reference implementation, with the key as plain text.
        let signature = webhook_signature("secret-key", "msg_1", 1_700_000_000, "{\"a\":1}");
        assert!(signature.starts_with("v1,"));
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret-key").unwrap();
        mac.update(b"msg_1.1700000000.{\"a\":1}");
        assert_eq!(
            signature,
            format!("v1,{}", base64(&mac.finalize().into_bytes()))
        );
        // Another body, another timestamp or another id is another signature.
        for other in [
            webhook_signature("secret-key", "msg_1", 1_700_000_000, "{\"a\":2}"),
            webhook_signature("secret-key", "msg_1", 1_700_000_001, "{\"a\":1}"),
            webhook_signature("secret-key", "msg_2", 1_700_000_000, "{\"a\":1}"),
            webhook_signature("other-key!", "msg_1", 1_700_000_000, "{\"a\":1}"),
        ] {
            assert_ne!(other, signature);
        }
    }

    #[test]
    fn while_a_replaced_secret_counts_the_header_carries_both_signatures() {
        let secrets = [
            SecretText::new("current-secret").unwrap(),
            SecretText::new("previous-secret").unwrap(),
        ];
        let headers = webhook_headers(&Outgoing {
            url: "https://example.com/x",
            event_id: Uuid::nil(),
            timestamp: 5,
            body: "{}",
            secrets: &secrets,
        });
        let signature = &headers
            .iter()
            .find(|(name, _)| *name == "webhook-signature")
            .unwrap()
            .1;
        let values: Vec<&str> = signature.split(' ').collect();
        assert_eq!(values.len(), 2);
        assert!(values.iter().all(|value| value.starts_with("v1,")));
        assert_eq!(headers[0], ("webhook-id", Uuid::nil().to_string()));
        assert_eq!(headers[1], ("webhook-timestamp", "5".to_owned()));
    }

    #[tokio::test]
    async fn a_target_that_is_not_public_is_refused_before_any_connection() {
        let strict = HttpWebhookSender::new(false);
        for url in [
            "https://127.0.0.1:9/x",
            "https://[::1]:9/x",
            "https://10.0.0.5/x",
            "https://169.254.169.254/latest/meta-data",
            "https://[::ffff:127.0.0.1]/x",
            "http://example.com/x",
            "https://localhost:9/x",
        ] {
            let outcome = strict
                .send(&Outgoing {
                    url,
                    event_id: Uuid::nil(),
                    timestamp: 0,
                    body: "{}",
                    secrets: &[SecretText::new("secret-value").unwrap()],
                })
                .await;
            assert!(!outcome.is_success(), "{url}");
            assert_eq!(outcome.status, None, "{url}");
        }
    }
}
