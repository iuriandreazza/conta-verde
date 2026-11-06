use std::sync::{Arc, Mutex};
use std::time::Duration;

use conta_verde_application::{MessageError, MessageSender, TextMessage};
use serde_json::json;

use crate::brasil_api::client;

const DEFAULT_API_URL: &str = "https://graph.facebook.com/v21.0";
// A reminder is sent by a background pass: a slow provider must not hold the others up for long.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The adapter for when no WhatsApp provider is configured: reminders by WhatsApp are not tried at all.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoMessageSender;

impl MessageSender for NoMessageSender {
    fn is_enabled(&self) -> bool {
        false
    }

    async fn send(&self, _message: &TextMessage) -> Result<(), MessageError> {
        Err(MessageError)
    }
}

/// Keeps what it was asked to send, for tests; it can be made to fail. Clones share the same list.
#[derive(Debug, Default, Clone)]
pub struct InMemoryMessageSender {
    sent: Arc<Mutex<Vec<TextMessage>>>,
    broken: bool,
}

impl InMemoryMessageSender {
    /// One whose sends all fail.
    pub fn broken() -> Self {
        Self {
            broken: true,
            ..Self::default()
        }
    }

    pub fn sent(&self) -> Vec<TextMessage> {
        self.sent.lock().expect("sender lock poisoned").clone()
    }
}

impl MessageSender for InMemoryMessageSender {
    fn is_enabled(&self) -> bool {
        true
    }

    async fn send(&self, message: &TextMessage) -> Result<(), MessageError> {
        if self.broken {
            return Err(MessageError);
        }
        self.sent
            .lock()
            .expect("sender lock poisoned")
            .push(message.clone());
        Ok(())
    }
}

/// Sends WhatsApp messages through the WhatsApp Business Cloud API (Meta). A message the business starts
/// must be a **template** the provider approved, so the text goes in as the one parameter of a template the
/// operator named; the credentials come from the environment and are never logged or stored.
pub struct WhatsAppCloudSender {
    client: reqwest::Client,
    base_url: String,
    phone_number_id: String,
    token: String,
    template: String,
}

impl WhatsAppCloudSender {
    pub fn new(phone_number_id: &str, token: &str, template: &str) -> Result<Self, reqwest::Error> {
        Self::with_endpoint(DEFAULT_API_URL, phone_number_id, token, template, TIMEOUT)
    }

    pub fn with_endpoint(
        base_url: &str,
        phone_number_id: &str,
        token: &str,
        template: &str,
        timeout: Duration,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client(timeout)?,
            base_url: base_url.trim_end_matches('/').to_owned(),
            phone_number_id: phone_number_id.to_owned(),
            token: token.to_owned(),
            template: template.to_owned(),
        })
    }
}

impl MessageSender for WhatsAppCloudSender {
    fn is_enabled(&self) -> bool {
        true
    }

    async fn send(&self, message: &TextMessage) -> Result<(), MessageError> {
        // A template parameter is one line: no line breaks, tabs or runs of spaces.
        let text = message
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let body = json!({
            "messaging_product": "whatsapp",
            "to": message.to,
            "type": "template",
            "template": {
                "name": self.template,
                "language": { "code": "pt_BR" },
                "components": [{
                    "type": "body",
                    "parameters": [{ "type": "text", "text": text }]
                }]
            }
        });
        let response = self
            .client
            .post(format!(
                "{}/{}/messages",
                self.base_url, self.phone_number_id
            ))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, "the WhatsApp provider could not be reached");
                MessageError
            })?;
        if response.status().is_success() {
            Ok(())
        } else {
            // The body can say why, and can echo what was sent: only the status is logged.
            tracing::warn!(status = %response.status(), "the WhatsApp provider refused a message");
            Err(MessageError)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::Router;
    use axum::extract::Path;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;

    use super::*;

    // A local stand-in for the WhatsApp Cloud API: the tests never reach the real one. It answers 200 only to
    // the right path, token and shape, and keeps what it was sent.
    type Received = Arc<Mutex<Vec<serde_json::Value>>>;

    async fn serve(received: Received) -> SocketAddr {
        let router = Router::new().route(
            "/v21.0/{phone}/messages",
            post(
                move |Path(phone): Path<String>,
                      headers: HeaderMap,
                      axum::Json(body): axum::Json<serde_json::Value>| {
                    let received = received.clone();
                    async move {
                        let authorized = headers
                            .get("authorization")
                            .is_some_and(|value| value == "Bearer secret-token");
                        if phone != "123" || !authorized {
                            return StatusCode::UNAUTHORIZED;
                        }
                        if body["to"] == "5500000000000" {
                            return StatusCode::BAD_REQUEST;
                        }
                        received.lock().unwrap().push(body);
                        StatusCode::OK
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        addr
    }

    fn sender(addr: SocketAddr, token: &str) -> WhatsAppCloudSender {
        WhatsAppCloudSender::with_endpoint(
            &format!("http://{addr}/v21.0/"),
            "123",
            token,
            "lembrete_cobranca",
            Duration::from_secs(2),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn sends_the_text_as_the_parameter_of_the_template_on_one_line() {
        let received = Received::default();
        let addr = serve(received.clone()).await;
        sender(addr, "secret-token")
            .send(&TextMessage {
                to: "5581999990000".into(),
                text: "Olá,\n  Ana!\tParcela vence hoje.".into(),
            })
            .await
            .unwrap();
        let body = received.lock().unwrap()[0].clone();
        assert_eq!(body["to"], "5581999990000");
        assert_eq!(body["template"]["name"], "lembrete_cobranca");
        assert_eq!(body["template"]["language"]["code"], "pt_BR");
        assert_eq!(
            body["template"]["components"][0]["parameters"][0]["text"],
            "Olá, Ana! Parcela vence hoje."
        );
    }

    #[tokio::test]
    async fn a_refusal_a_wrong_token_or_no_server_is_a_failure() {
        let received = Received::default();
        let addr = serve(received.clone()).await;
        let message = |to: &str| TextMessage {
            to: to.into(),
            text: "oi".into(),
        };
        assert_eq!(
            sender(addr, "secret-token")
                .send(&message("5500000000000"))
                .await,
            Err(MessageError)
        );
        assert_eq!(
            sender(addr, "wrong").send(&message("5581999990000")).await,
            Err(MessageError)
        );
        assert!(received.lock().unwrap().is_empty());
        // Nobody listening: the port of a server that is gone.
        let gone = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        assert_eq!(
            sender(gone, "secret-token")
                .send(&message("5581999990000"))
                .await,
            Err(MessageError)
        );
    }

    #[tokio::test]
    async fn the_fakes_say_whether_they_are_enabled() {
        assert!(!NoMessageSender.is_enabled());
        let fake = InMemoryMessageSender::default();
        fake.send(&TextMessage {
            to: "1".into(),
            text: "oi".into(),
        })
        .await
        .unwrap();
        assert_eq!(fake.sent().len(), 1);
        assert_eq!(
            InMemoryMessageSender::broken()
                .send(&TextMessage {
                    to: "1".into(),
                    text: "oi".into()
                })
                .await,
            Err(MessageError)
        );
    }
}
