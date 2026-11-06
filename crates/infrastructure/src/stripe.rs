use std::time::{Duration, SystemTime, UNIX_EPOCH};

use conta_verde_application::{
    PaymentProvider, ProviderCharge, ProviderChargeRequest, ProviderError, ProviderEvent,
    ProviderEventKind, WebhookError, WebhookHeaders,
};
use conta_verde_domain::{Amount, CalendarDate, ChargeMethod, ChargePayment};
use reqwest::{Method, StatusCode};
use serde_json::Value;

use crate::brasil_api::client;
use crate::webhook_signature::is_valid_signature;

const DEFAULT_API_URL: &str = "https://api.stripe.com";
const TIMEOUT: Duration = Duration::from_secs(20);
/// How far a webhook's timestamp may be from now: Stripe's own default, against replays.
const TOLERANCE_SECONDS: i64 = 300;
/// Brazil is three hours behind UTC all year (no daylight saving since 2019): the day a payment is dated.
const BRAZIL_OFFSET_SECONDS: i64 = 3 * 3600;
const MAX_BOLETO_DAYS: i64 = 60;
const MIN_PIX_SECONDS: i64 = 60;
const MAX_PIX_SECONDS: i64 = 1_209_600;
/// A Checkout Session lasts a day at most; a margin keeps a clock a little ahead of Stripe's from making the
/// request refused.
const CHECKOUT_SECONDS: i64 = 23 * 3600;

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(0))
}

/// Why a call did not give an answer: a missing thing is told apart, and so is Stripe's reason for refusing.
enum Failed {
    NotFound,
    Provider(ProviderError),
}

impl From<Failed> for ProviderError {
    fn from(failed: Failed) -> Self {
        match failed {
            Failed::NotFound => ProviderError::Rejected("not found".to_owned()),
            Failed::Provider(error) => error,
        }
    }
}

/// A payment provider that charges through [Stripe](https://stripe.com) in Brazil: a boleto and a Pix by a
/// PaymentIntent, a card by a Checkout Session ([ADR 0050]). The money is the account holder's at Stripe. The
/// secret key and the webhook secret come from the environment and are never logged or stored.
#[derive(Clone)]
pub struct StripeProvider {
    client: reqwest::Client,
    base_url: String,
    secret_key: String,
    webhook_secret: String,
    /// Where a customer lands after the card page.
    return_url: String,
    /// The clock, which Stripe's relative expiries and the webhook's tolerance need; a test sets its own.
    now: fn() -> i64,
}

impl StripeProvider {
    pub fn new(
        base_url: Option<&str>,
        secret_key: &str,
        webhook_secret: &str,
        return_url: &str,
    ) -> Result<Self, reqwest::Error> {
        Self::with_clock(
            base_url,
            secret_key,
            webhook_secret,
            return_url,
            TIMEOUT,
            unix_now,
        )
    }

    pub fn with_clock(
        base_url: Option<&str>,
        secret_key: &str,
        webhook_secret: &str,
        return_url: &str,
        timeout: Duration,
        now: fn() -> i64,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client(timeout)?,
            base_url: base_url
                .unwrap_or(DEFAULT_API_URL)
                .trim_end_matches('/')
                .to_owned(),
            secret_key: secret_key.to_owned(),
            webhook_secret: webhook_secret.to_owned(),
            return_url: return_url.to_owned(),
            now,
        })
    }

    /// One call: Stripe's JSON answer, or why not. Only the status and Stripe's own error code are logged:
    /// the message can echo what was sent.
    async fn call(
        &self,
        method: Method,
        path: &str,
        form: Option<Vec<(String, String)>>,
        idempotency_key: Option<&str>,
    ) -> Result<Value, Failed> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.secret_key);
        if let Some(key) = idempotency_key {
            request = request.header("idempotency-key", key);
        }
        if let Some(form) = form {
            request = request
                .header("content-type", "application/x-www-form-urlencoded")
                .body(urlencode(&form));
        }
        let response = request.send().await.map_err(|error| {
            Failed::Provider(ProviderError::Unavailable(format!(
                "request failed: {}",
                error.without_url()
            )))
        })?;
        let status = response.status();
        let answer: Value = response.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(answer);
        }
        let code = answer["error"]["code"].as_str().unwrap_or("none");
        let message = answer["error"]["message"]
            .as_str()
            .unwrap_or("no reason given")
            .chars()
            .take(200)
            .collect::<String>();
        match status {
            StatusCode::NOT_FOUND => Err(Failed::NotFound),
            StatusCode::UNAUTHORIZED
            | StatusCode::FORBIDDEN
            | StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_MANY_REQUESTS => Err(Failed::Provider(ProviderError::Unavailable(
                format!("{status} ({code})"),
            ))),
            status if status.is_client_error() => Err(Failed::Provider(ProviderError::Rejected(
                format!("{status} ({code}): {message}"),
            ))),
            status => Err(Failed::Provider(ProviderError::Unavailable(format!(
                "{status}"
            )))),
        }
    }

    async fn payment_intent(
        &self,
        request: &ProviderChargeRequest,
    ) -> Result<ProviderCharge, ProviderError> {
        let boleto = request.method == ChargeMethod::Boleto;
        let kind = if boleto { "boleto" } else { "pix" };
        let Some(email) = request.payer.email.as_deref() else {
            return Err(ProviderError::Rejected(format!(
                "a Stripe {kind} needs the customer's e-mail"
            )));
        };
        let mut form = pairs(&[
            ("amount", cents(request.amount)?),
            ("currency", "brl".to_owned()),
            ("confirm", "true".to_owned()),
            ("payment_method_types[0]", kind.to_owned()),
            ("payment_method_data[type]", kind.to_owned()),
            (
                "payment_method_data[billing_details][name]",
                request.payer.name.clone(),
            ),
            (
                "payment_method_data[billing_details][email]",
                email.to_owned(),
            ),
            (
                "description",
                request.description.chars().take(500).collect(),
            ),
            (
                "metadata[charge_id]",
                request.charge_id.as_uuid().to_string(),
            ),
        ]);
        let now = (self.now)();
        if boleto {
            let (Some(document), Some(address)) = (&request.payer.document, &request.payer.address)
            else {
                return Err(ProviderError::Rejected(
                    "a Stripe boleto needs the customer's CPF or CNPJ and address".to_owned(),
                ));
            };
            // Stripe takes a boleto up to 60 days ahead; a later due day is refused rather than quietly cut, which
            // would expire the charge before it is due. One already past expires at the end of today.
            if !document.chars().all(|c| c.is_ascii_digit()) {
                return Err(ProviderError::Rejected(
                    "Stripe takes a CPF or CNPJ of digits only".to_owned(),
                ));
            }
            let days = (request.due_on.to_unix_days() - brazil_day(now)).max(0);
            if days > MAX_BOLETO_DAYS {
                return Err(ProviderError::Rejected(
                    "a Stripe boleto is for at most 60 days ahead".to_owned(),
                ));
            }
            form.extend(pairs(&[
                ("payment_method_data[boleto][tax_id]", document.clone()),
                (
                    "payment_method_data[billing_details][address][line1]",
                    address.street.clone(),
                ),
                (
                    "payment_method_data[billing_details][address][city]",
                    address.city.clone(),
                ),
                (
                    "payment_method_data[billing_details][address][state]",
                    address.state.clone(),
                ),
                (
                    "payment_method_data[billing_details][address][postal_code]",
                    address.postal_code.clone(),
                ),
                (
                    "payment_method_data[billing_details][address][country]",
                    "BR".to_owned(),
                ),
                (
                    "payment_method_options[boleto][expires_after_days]",
                    days.to_string(),
                ),
            ]));
        } else {
            // Until the end of the due day, in Brazil.
            let end_of_due_day =
                (request.due_on.to_unix_days() + 1) * 86_400 + BRAZIL_OFFSET_SECONDS;
            let seconds = (end_of_due_day - now).max(MIN_PIX_SECONDS);
            if seconds > MAX_PIX_SECONDS {
                return Err(ProviderError::Rejected(
                    "a Stripe Pix is for at most 14 days ahead".to_owned(),
                ));
            }
            form.push((
                "payment_method_options[pix][expires_after_seconds]".to_owned(),
                seconds.to_string(),
            ));
        }
        let made = self
            .call(
                Method::POST,
                "/v1/payment_intents",
                Some(form),
                Some(&request.charge_id.as_uuid().to_string()),
            )
            .await
            .map_err(ProviderError::from)?;
        let id = text(&made, "id")?;
        let payment = if boleto {
            let details = &made["next_action"]["boleto_display_details"];
            ChargePayment {
                digitable_line: Some(text(details, "number")?),
                payment_url: details["hosted_voucher_url"].as_str().map(str::to_owned),
                ..ChargePayment::default()
            }
        } else {
            ChargePayment {
                pix_payload: Some(text(&made["next_action"]["pix_display_qr_code"], "data")?),
                ..ChargePayment::default()
            }
        };
        Ok(ProviderCharge {
            external_id: id,
            payment,
        })
    }

    async fn checkout_session(
        &self,
        request: &ProviderChargeRequest,
    ) -> Result<ProviderCharge, ProviderError> {
        let charge_id = request.charge_id.as_uuid().to_string();
        let mut form = pairs(&[
            ("mode", "payment".to_owned()),
            ("success_url", self.return_url.clone()),
            ("cancel_url", self.return_url.clone()),
            ("payment_method_types[0]", "card".to_owned()),
            ("line_items[0][quantity]", "1".to_owned()),
            ("line_items[0][price_data][currency]", "brl".to_owned()),
            (
                "line_items[0][price_data][unit_amount]",
                cents(request.amount)?,
            ),
            (
                "line_items[0][price_data][product_data][name]",
                request.description.chars().take(250).collect(),
            ),
            ("client_reference_id", charge_id.clone()),
            ("metadata[charge_id]", charge_id.clone()),
            (
                "payment_intent_data[metadata][charge_id]",
                charge_id.clone(),
            ),
            ("expires_at", ((self.now)() + CHECKOUT_SECONDS).to_string()),
        ]);
        if let Some(email) = &request.payer.email {
            form.push(("customer_email".to_owned(), email.clone()));
        }
        let made = self
            .call(
                Method::POST,
                "/v1/checkout/sessions",
                Some(form),
                Some(&charge_id),
            )
            .await
            .map_err(ProviderError::from)?;
        Ok(ProviderCharge {
            external_id: text(&made, "id")?,
            payment: ChargePayment {
                payment_url: Some(text(&made, "url")?),
                ..ChargePayment::default()
            },
        })
    }
}

fn text(value: &Value, key: &str) -> Result<String, ProviderError> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ProviderError::Unavailable(format!("the answer has no {key}")))
}

fn pairs(list: &[(&str, String)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

/// An amount in centavos, the whole number Stripe takes.
fn cents(amount: Amount) -> Result<String, ProviderError> {
    let value = amount.as_decimal();
    // An amount is in cents (at most two decimals), so scaling it to cents is exact.
    let scaled = value * conta_verde_domain::Decimal::from(100);
    if scaled.fract().is_zero() {
        Ok(scaled.trunc().to_string())
    } else {
        Err(ProviderError::Rejected("an amount is in cents".to_owned()))
    }
}

/// The calendar day, in Brazil, of a Unix time, as days since the epoch.
fn brazil_day(unix_seconds: i64) -> i64 {
    (unix_seconds - BRAZIL_OFFSET_SECONDS).div_euclid(86_400)
}

fn urlencode(form: &[(String, String)]) -> String {
    fn escape(text: &str) -> String {
        text.bytes()
            .map(|byte| match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    char::from(byte).to_string()
                }
                other => format!("%{other:02X}"),
            })
            .collect()
    }
    form.iter()
        .map(|(key, value)| format!("{}={}", escape(key), escape(value)))
        .collect::<Vec<_>>()
        .join("&")
}

impl PaymentProvider for StripeProvider {
    fn key(&self) -> &str {
        "stripe"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn create_charge(
        &self,
        request: &ProviderChargeRequest,
    ) -> Result<ProviderCharge, ProviderError> {
        match request.method {
            ChargeMethod::Boleto | ChargeMethod::Pix => self.payment_intent(request).await,
            ChargeMethod::CardLink => self.checkout_session(request).await,
        }
    }

    async fn cancel_charge(&self, external_id: &str) -> Result<(), ProviderError> {
        let (cancel, read) = if external_id.starts_with("cs_") {
            (
                format!("/v1/checkout/sessions/{external_id}/expire"),
                format!("/v1/checkout/sessions/{external_id}"),
            )
        } else {
            (
                format!("/v1/payment_intents/{external_id}/cancel"),
                format!("/v1/payment_intents/{external_id}"),
            )
        };
        match self.call(Method::POST, &cancel, None, None).await {
            Ok(_) | Err(Failed::NotFound) => Ok(()),
            Err(Failed::Provider(ProviderError::Rejected(why))) => {
                // Already called off (or expired) is what was asked for; anything else (a paid one, a boleto,
                // which Stripe does not cancel before it expires) is Stripe's refusal.
                let status = self
                    .call(Method::GET, &read, None, None)
                    .await
                    .ok()
                    .and_then(|state| state["status"].as_str().map(str::to_owned));
                match status.as_deref() {
                    Some("canceled" | "expired") => Ok(()),
                    _ => Err(ProviderError::Rejected(why)),
                }
            }
            Err(Failed::Provider(other)) => Err(other),
        }
    }

    fn verify_webhook(
        &self,
        headers: &WebhookHeaders,
        body: &[u8],
    ) -> Result<Vec<ProviderEvent>, WebhookError> {
        let header = headers
            .get("stripe-signature")
            .ok_or(WebhookError::InvalidSignature)?;
        let mut timestamp = None;
        let mut signatures = Vec::new();
        for part in header.split(',') {
            match part.trim().split_once('=') {
                Some(("t", value)) => timestamp = value.parse::<i64>().ok(),
                // Only `v1`: the `v0` of a test event is ignored, so a downgrade gets nowhere.
                Some(("v1", value)) => signatures.push(value),
                _ => {}
            }
        }
        let Some(timestamp) = timestamp else {
            return Err(WebhookError::InvalidSignature);
        };
        let mut signed = format!("{timestamp}.").into_bytes();
        signed.extend_from_slice(body);
        let authentic = signatures.iter().any(|signature| {
            is_valid_signature(self.webhook_secret.as_bytes(), &signed, signature)
        });
        // The timestamp is part of what is signed, so it cannot be moved; one too old is a replay.
        if !authentic || ((self.now)() - timestamp).abs() > TOLERANCE_SECONDS {
            return Err(WebhookError::InvalidSignature);
        }
        let malformed = |what: &str| WebhookError::Malformed(what.to_owned());
        let event: Value = serde_json::from_slice(body).map_err(|_| malformed("not JSON"))?;
        let object = &event["data"]["object"];
        let kind = match event["type"].as_str().ok_or_else(|| malformed("type"))? {
            "payment_intent.succeeded" => paid(
                object["amount_received"].as_i64(),
                event["created"].as_i64(),
            )
            .ok_or_else(|| malformed("amount_received"))?,
            "checkout.session.completed" if object["payment_status"] == "paid" => {
                paid(object["amount_total"].as_i64(), event["created"].as_i64())
                    .ok_or_else(|| malformed("amount_total"))?
            }
            "payment_intent.canceled" => ProviderEventKind::Cancelled,
            // A boleto or a Pix that was not paid by its time.
            "payment_intent.payment_failed" | "checkout.session.expired" => {
                ProviderEventKind::Expired
            }
            _ => return Ok(Vec::new()),
        };
        Ok(vec![ProviderEvent {
            event_id: event["id"]
                .as_str()
                .ok_or_else(|| malformed("id"))?
                .to_owned(),
            external_id: object["id"]
                .as_str()
                .ok_or_else(|| malformed("data.object.id"))?
                .to_owned(),
            kind,
        }])
    }
}

/// A payment: what was received in centavos, and the day it was, in Brazil. Stripe's notice does not carry its
/// fee (that is on the balance transaction), so it is recorded as none.
fn paid(centavos: Option<i64>, created: Option<i64>) -> Option<ProviderEventKind> {
    let centavos = centavos.filter(|amount| *amount > 0)?;
    let day = CalendarDate::from_unix_days(brazil_day(created?))?;
    Some(ProviderEventKind::Paid {
        paid_amount: Amount::in_cents(&format!("{}.{:02}", centavos / 100, centavos % 100))?,
        paid_on: day,
        fee: Amount::ZERO,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::{Form, Path, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{get, post};
    use conta_verde_application::{Payer, PayerAddress};
    use conta_verde_domain::ChargeId;
    use serde_json::json;

    use super::*;
    use crate::webhook_signature::hmac_sha256_hex;

    const KEY: &str = "sk_test_key";
    const SECRET: &str = "whsec_test_secret";
    /// 2026-12-05 12:00 UTC.
    const NOW: i64 = 1_796_472_000;

    fn clock() -> i64 {
        NOW
    }

    // A local stand-in for the Stripe API: the tests never reach the real one. It checks the key and keeps
    // the forms it was sent. A name ending in `!` is refused; payment intents that are `pi_cancelled` read
    // canceled.
    #[derive(Default)]
    struct Fake {
        forms: Vec<HashMap<String, String>>,
        idempotency: Vec<String>,
        cancelled: Vec<String>,
    }

    type Shared = Arc<Mutex<Fake>>;

    fn authorized(headers: &HeaderMap) -> bool {
        headers
            .get("authorization")
            .is_some_and(|value| value == &format!("Bearer {KEY}"))
    }

    async fn serve(fake: Shared) -> SocketAddr {
        let router = Router::new()
            .route(
                "/v1/payment_intents",
                post(
                    |State(fake): State<Shared>,
                     headers: HeaderMap,
                     Form(form): Form<HashMap<String, String>>| async move {
                        if !authorized(&headers) {
                            return (StatusCode::UNAUTHORIZED, axum::Json(json!({})));
                        }
                        let mut fake = fake.lock().unwrap();
                        let kind = form["payment_method_data[type]"].clone();
                        if form["payment_method_data[billing_details][name]"].ends_with('!') {
                            fake.forms.push(form);
                            return (
                                StatusCode::BAD_REQUEST,
                                axum::Json(json!({"error": {"code": "invalid", "message": "name"}})),
                            );
                        }
                        fake.idempotency.push(
                            headers
                                .get("idempotency-key")
                                .map(|value| value.to_str().unwrap().to_owned())
                                .unwrap_or_default(),
                        );
                        fake.forms.push(form);
                        let id = format!("pi_{}", fake.forms.len());
                        let next = if kind == "boleto" {
                            json!({"boleto_display_details": {
                                "number": "23790.12345 60000.000002 00000.000006 1 99990000010050",
                                "hosted_voucher_url": format!("https://payments.stripe.com/boleto/voucher/{id}"),
                                "expires_at": 1_797_000_000
                            }})
                        } else {
                            json!({"pix_display_qr_code": {
                                "data": format!("00020126580014br.gov.bcb.pix{id}"),
                                "image_url_png": "https://x"
                            }})
                        };
                        (
                            StatusCode::OK,
                            axum::Json(json!({"id": id, "status": "requires_action", "next_action": next})),
                        )
                    },
                ),
            )
            .route(
                "/v1/payment_intents/{id}/cancel",
                post(|State(fake): State<Shared>, Path(id): Path<String>| async move {
                    if id == "pi_gone" {
                        return (StatusCode::NOT_FOUND, axum::Json(json!({})));
                    }
                    if id == "pi_cancelled" {
                        return (
                            StatusCode::BAD_REQUEST,
                            axum::Json(json!({"error": {"code": "payment_intent_unexpected_state", "message": "canceled"}})),
                        );
                    }
                    if id == "pi_paid" {
                        return (
                            StatusCode::BAD_REQUEST,
                            axum::Json(json!({"error": {"code": "payment_intent_unexpected_state", "message": "succeeded"}})),
                        );
                    }
                    fake.lock().unwrap().cancelled.push(id.clone());
                    (StatusCode::OK, axum::Json(json!({"id": id, "status": "canceled"})))
                }),
            )
            .route(
                "/v1/payment_intents/{id}",
                get(|Path(id): Path<String>| async move {
                    let status = if id == "pi_cancelled" { "canceled" } else { "succeeded" };
                    (StatusCode::OK, axum::Json(json!({"id": id, "status": status})))
                }),
            )
            .route(
                "/v1/checkout/sessions",
                post(
                    |State(fake): State<Shared>,
                     headers: HeaderMap,
                     Form(form): Form<HashMap<String, String>>| async move {
                        if !authorized(&headers) {
                            return (StatusCode::UNAUTHORIZED, axum::Json(json!({})));
                        }
                        let mut fake = fake.lock().unwrap();
                        fake.forms.push(form);
                        let id = format!("cs_{}", fake.forms.len());
                        (
                            StatusCode::OK,
                            axum::Json(json!({"id": id, "url": format!("https://checkout.stripe.com/c/pay/{id}")})),
                        )
                    },
                ),
            )
            .route(
                "/v1/checkout/sessions/{id}/expire",
                post(|State(fake): State<Shared>, Path(id): Path<String>| async move {
                    fake.lock().unwrap().cancelled.push(id.clone());
                    (StatusCode::OK, axum::Json(json!({"id": id, "status": "expired"})))
                }),
            )
            .with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        addr
    }

    fn provider(addr: SocketAddr, key: &str) -> StripeProvider {
        StripeProvider::with_clock(
            Some(&format!("http://{addr}/")),
            key,
            SECRET,
            "https://app.example/thanks",
            Duration::from_secs(2),
            clock,
        )
        .unwrap()
    }

    fn request(method: ChargeMethod) -> ProviderChargeRequest {
        ProviderChargeRequest {
            charge_id: ChargeId::generate(),
            method,
            amount: Amount::in_cents("150.50").unwrap(),
            due_on: CalendarDate::parse("2026-12-10").unwrap(),
            payer: Payer {
                name: "Mercado Boa Compra".into(),
                document: Some("52998224725".into()),
                email: Some("compras@example.com".into()),
                address: Some(PayerAddress {
                    street: "Av. Paulista, 1000".into(),
                    city: "São Paulo".into(),
                    state: "SP".into(),
                    postal_code: "01310100".into(),
                }),
            },
            description: "Venda 12 (1/2)".into(),
        }
    }

    #[tokio::test]
    async fn makes_a_boleto_with_the_address_and_the_days_to_its_due_day() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let asked = request(ChargeMethod::Boleto);
        let made = provider(addr, KEY).create_charge(&asked).await.unwrap();
        assert_eq!(made.external_id, "pi_1");
        assert_eq!(
            made.payment.digitable_line.as_deref(),
            Some("23790.12345 60000.000002 00000.000006 1 99990000010050")
        );
        assert_eq!(
            made.payment.payment_url.as_deref(),
            Some("https://payments.stripe.com/boleto/voucher/pi_1")
        );
        let fake = fake.lock().unwrap();
        let form = &fake.forms[0];
        assert_eq!(form["amount"], "15050");
        assert_eq!(form["currency"], "brl");
        assert_eq!(form["confirm"], "true");
        assert_eq!(form["payment_method_data[boleto][tax_id]"], "52998224725");
        assert_eq!(
            form["payment_method_data[billing_details][address][city]"],
            "São Paulo"
        );
        assert_eq!(
            form["payment_method_data[billing_details][address][country]"],
            "BR"
        );
        // 2026-12-05 09:00 in Brazil to the 10th: five days.
        assert_eq!(
            form["payment_method_options[boleto][expires_after_days]"],
            "5"
        );
        assert_eq!(
            form["metadata[charge_id]"],
            asked.charge_id.as_uuid().to_string()
        );
        assert_eq!(fake.idempotency[0], asked.charge_id.as_uuid().to_string());
    }

    #[tokio::test]
    async fn makes_a_pix_that_lasts_until_the_end_of_its_due_day() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let made = provider(addr, KEY)
            .create_charge(&request(ChargeMethod::Pix))
            .await
            .unwrap();
        assert!(made.payment.pix_payload.unwrap().ends_with("pi_1"));
        let form = fake.lock().unwrap().forms[0].clone();
        // From 2026-12-05 12:00 UTC to the end of the 10th in Brazil (2026-12-11 03:00 UTC): 5 days 15 hours.
        assert_eq!(
            form["payment_method_options[pix][expires_after_seconds]"],
            (5 * 86_400 + 15 * 3600).to_string()
        );
        assert!(!form.contains_key("payment_method_data[boleto][tax_id]"));
    }

    #[tokio::test]
    async fn makes_a_card_page_that_lasts_a_day() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let asked = request(ChargeMethod::CardLink);
        let made = provider(addr, KEY).create_charge(&asked).await.unwrap();
        assert_eq!(made.external_id, "cs_1");
        assert_eq!(
            made.payment.payment_url.as_deref(),
            Some("https://checkout.stripe.com/c/pay/cs_1")
        );
        let form = fake.lock().unwrap().forms[0].clone();
        assert_eq!(form["mode"], "payment");
        assert_eq!(form["line_items[0][price_data][unit_amount]"], "15050");
        assert_eq!(form["success_url"], "https://app.example/thanks");
        assert_eq!(
            form["client_reference_id"],
            asked.charge_id.as_uuid().to_string()
        );
        assert_eq!(form["expires_at"], (NOW + 23 * 3600).to_string());
    }

    #[tokio::test]
    async fn refuses_without_asking_what_stripe_needs_and_says_why_otherwise() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let stripe = provider(addr, KEY);
        let mut no_address = request(ChargeMethod::Boleto);
        no_address.payer.address = None;
        let mut no_email = request(ChargeMethod::Pix);
        no_email.payer.email = None;
        for asked in [no_address, no_email] {
            assert!(matches!(
                stripe.create_charge(&asked).await,
                Err(ProviderError::Rejected(_))
            ));
        }
        assert!(fake.lock().unwrap().forms.is_empty());

        let mut refused = request(ChargeMethod::Pix);
        refused.payer.name = "Nome!".into();
        assert!(matches!(
            stripe.create_charge(&refused).await,
            Err(ProviderError::Rejected(why)) if why.contains("invalid")
        ));
        assert!(matches!(
            provider(addr, "wrong")
                .create_charge(&request(ChargeMethod::Pix))
                .await,
            Err(ProviderError::Unavailable(_))
        ));
        let gone = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        assert!(matches!(
            provider(gone, KEY)
                .create_charge(&request(ChargeMethod::Pix))
                .await,
            Err(ProviderError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn refuses_a_due_day_further_than_stripe_takes_and_a_document_that_is_not_digits() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let stripe = provider(addr, KEY);
        let mut far_boleto = request(ChargeMethod::Boleto);
        far_boleto.due_on = CalendarDate::parse("2027-03-01").unwrap();
        let mut far_pix = request(ChargeMethod::Pix);
        far_pix.due_on = CalendarDate::parse("2026-12-25").unwrap();
        let mut alphanumeric = request(ChargeMethod::Boleto);
        alphanumeric.payer.document = Some("12ABC34501DE35".into());
        for asked in [far_boleto, far_pix, alphanumeric] {
            assert!(matches!(
                stripe.create_charge(&asked).await,
                Err(ProviderError::Rejected(_))
            ));
        }
        assert!(fake.lock().unwrap().forms.is_empty());
    }

    #[tokio::test]
    async fn calls_off_what_it_can_and_says_so_when_it_cannot() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let stripe = provider(addr, KEY);
        stripe.cancel_charge("pi_7").await.unwrap();
        stripe.cancel_charge("cs_7").await.unwrap();
        stripe.cancel_charge("pi_gone").await.unwrap();
        // Already canceled is what was asked for; one that was paid (or a boleto, which is not cancelled
        // before it expires) is Stripe's refusal.
        stripe.cancel_charge("pi_cancelled").await.unwrap();
        assert!(matches!(
            stripe.cancel_charge("pi_paid").await,
            Err(ProviderError::Rejected(_))
        ));
        assert_eq!(fake.lock().unwrap().cancelled, ["pi_7", "cs_7"]);
    }

    fn signed(body: &str, timestamp: i64, secret: &str) -> WebhookHeaders {
        let signature =
            hmac_sha256_hex(secret.as_bytes(), format!("{timestamp}.{body}").as_bytes());
        WebhookHeaders::new([(
            "Stripe-Signature".to_owned(),
            format!("t={timestamp},v1={signature},v0=0000"),
        )])
    }

    fn event(kind: &str, object: Value) -> String {
        json!({"id": "evt_1", "type": kind, "created": NOW, "data": {"object": object}}).to_string()
    }

    fn verify(headers: &WebhookHeaders, body: &str) -> Result<Vec<ProviderEvent>, WebhookError> {
        StripeProvider::with_clock(
            None,
            KEY,
            SECRET,
            "https://x",
            Duration::from_secs(1),
            clock,
        )
        .unwrap()
        .verify_webhook(headers, body.as_bytes())
    }

    #[test]
    fn reads_a_payment_with_the_day_in_brazil_and_no_fee() {
        let body = event(
            "payment_intent.succeeded",
            json!({"id": "pi_1", "amount_received": 15050, "currency": "brl"}),
        );
        let events = verify(&signed(&body, NOW, SECRET), &body).unwrap();
        assert_eq!(events[0].event_id, "evt_1");
        assert_eq!(events[0].external_id, "pi_1");
        assert_eq!(
            events[0].kind,
            ProviderEventKind::Paid {
                paid_amount: Amount::in_cents("150.50").unwrap(),
                paid_on: CalendarDate::parse("2026-12-05").unwrap(),
                fee: Amount::ZERO,
            }
        );
        // A card page is a payment when its session is paid.
        let card = event(
            "checkout.session.completed",
            json!({"id": "cs_1", "payment_status": "paid", "amount_total": 10000}),
        );
        assert_eq!(
            verify(&signed(&card, NOW, SECRET), &card).unwrap()[0].external_id,
            "cs_1"
        );
        let unpaid = event(
            "checkout.session.completed",
            json!({"id": "cs_1", "payment_status": "unpaid", "amount_total": 10000}),
        );
        assert!(
            verify(&signed(&unpaid, NOW, SECRET), &unpaid)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_boleto_that_was_not_paid_expires_and_a_canceled_intent_cancels() {
        for (kind, expected) in [
            ("payment_intent.payment_failed", ProviderEventKind::Expired),
            ("checkout.session.expired", ProviderEventKind::Expired),
            ("payment_intent.canceled", ProviderEventKind::Cancelled),
        ] {
            let body = event(kind, json!({"id": "pi_1"}));
            assert_eq!(
                verify(&signed(&body, NOW, SECRET), &body).unwrap()[0].kind,
                expected,
                "{kind}"
            );
        }
        for kind in [
            "payment_intent.created",
            "charge.succeeded",
            "payment_intent.requires_action",
        ] {
            let body = event(kind, json!({"id": "pi_1"}));
            assert!(
                verify(&signed(&body, NOW, SECRET), &body)
                    .unwrap()
                    .is_empty(),
                "{kind}"
            );
        }
    }

    #[test]
    fn refuses_what_is_not_signed_by_the_secret_or_is_a_replay_and_what_is_not_a_notice() {
        let body = event(
            "payment_intent.succeeded",
            json!({"id": "pi_1", "amount_received": 100}),
        );
        let good = signed(&body, NOW, SECRET);
        assert!(verify(&good, &body).is_ok());
        for headers in [
            WebhookHeaders::default(),
            signed(&body, NOW, "whsec_other"),
            signed("{}", NOW, SECRET),
            // Signed long ago, and signed in the future: neither is now.
            signed(&body, NOW - 301, SECRET),
            signed(&body, NOW + 301, SECRET),
            WebhookHeaders::new([("stripe-signature".to_owned(), "v1=abc".to_owned())]),
            WebhookHeaders::new([("stripe-signature".to_owned(), "t=x,v1=abc".to_owned())]),
        ] {
            assert_eq!(verify(&headers, &body), Err(WebhookError::InvalidSignature));
        }
        // A timestamp that is changed after signing does not match.
        let moved = WebhookHeaders::new([(
            "stripe-signature".to_owned(),
            format!(
                "t={},v1={}",
                NOW + 1,
                hmac_sha256_hex(SECRET.as_bytes(), format!("{NOW}.{body}").as_bytes())
            ),
        )]);
        assert_eq!(verify(&moved, &body), Err(WebhookError::InvalidSignature));
        // Signed, but not a notice.
        for bad in [
            "nope",
            &event("payment_intent.succeeded", json!({"id": "pi_1"})),
        ] {
            assert!(matches!(
                verify(&signed(bad, NOW, SECRET), bad),
                Err(WebhookError::Malformed(_))
            ));
        }
    }

    #[test]
    fn encodes_forms_and_amounts_exactly() {
        assert_eq!(
            urlencode(&[("a[b]".into(), "x y&z=é".into())]),
            "a%5Bb%5D=x%20y%26z%3D%C3%A9"
        );
        assert_eq!(cents(Amount::in_cents("150.50").unwrap()).unwrap(), "15050");
        assert_eq!(cents(Amount::in_cents("0.01").unwrap()).unwrap(), "1");
        assert_eq!(
            cents(Amount::in_cents("1000000").unwrap()).unwrap(),
            "100000000"
        );
        assert_eq!(
            brazil_day(NOW),
            CalendarDate::parse("2026-12-05").unwrap().to_unix_days()
        );
        assert_eq!(
            brazil_day(NOW + 15 * 3600),
            CalendarDate::parse("2026-12-06").unwrap().to_unix_days()
        );
    }

    // Homologation: run by hand against Stripe in test mode with `STRIPE_TEST_SECRET_KEY` set. CI runs the
    // ignored tests but has no key, so without it this does nothing.
    #[tokio::test]
    #[ignore = "needs STRIPE_TEST_SECRET_KEY and the network"]
    async fn the_test_mode_makes_a_pix_and_a_card_page() {
        let Ok(key) = std::env::var("STRIPE_TEST_SECRET_KEY") else {
            eprintln!("STRIPE_TEST_SECRET_KEY is not set: Stripe is not asked");
            return;
        };
        let stripe = StripeProvider::new(None, &key, SECRET, "https://example.com/thanks").unwrap();
        let mut asked = request(ChargeMethod::Pix);
        asked.due_on = CalendarDate::from_unix_days(brazil_day(unix_now()) + 3).unwrap();
        let made = stripe.create_charge(&asked).await.unwrap();
        stripe.cancel_charge(&made.external_id).await.unwrap();
        let card = stripe
            .create_charge(&request(ChargeMethod::CardLink))
            .await
            .unwrap();
        stripe.cancel_charge(&card.external_id).await.unwrap();
    }
}
