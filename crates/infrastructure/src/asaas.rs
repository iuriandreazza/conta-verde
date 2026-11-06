use std::time::Duration;

use conta_verde_application::{
    PaymentProvider, ProviderCharge, ProviderChargeRequest, ProviderError, ProviderEvent,
    ProviderEventKind, WebhookError, WebhookHeaders,
};
use conta_verde_domain::{Amount, CalendarDate, ChargeMethod, ChargePayment, Decimal};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::brasil_api::client;
use crate::webhook_signature::constant_time_eq;

const DEFAULT_API_URL: &str = "https://api.asaas.com";
const TIMEOUT: Duration = Duration::from_secs(15);
/// The header Asaas puts the webhook's `authToken` in.
const TOKEN_HEADER: &str = "asaas-access-token";
/// What Asaas accepts as a description.
const MAX_DESCRIPTION_CHARS: usize = 500;
/// The number a JSON amount is written with is put in by hand, so that it is the exact decimal text and never
/// a float: this stands in the body until then.
const VALUE_PLACEHOLDER: &str = "@@value@@";

/// Why a call did not give an answer. A missing thing is told apart so that calling off one that is gone is
/// not an error, without reading it from a message.
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

/// A payment provider that charges through [Asaas](https://www.asaas.com): registered boleto, Pix with its
/// copy-and-paste text, and a card page ([ADR 0049]). The money is the account holder's at Asaas, never ours.
/// The API key and the webhook token come from the environment and are never logged or stored.
#[derive(Clone)]
pub struct AsaasProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    webhook_token: String,
}

impl AsaasProvider {
    /// `base_url` is `https://api.asaas.com` for production and `https://api-sandbox.asaas.com` for the sandbox.
    pub fn new(
        base_url: Option<&str>,
        api_key: &str,
        webhook_token: &str,
    ) -> Result<Self, reqwest::Error> {
        Self::with_timeout(base_url, api_key, webhook_token, TIMEOUT)
    }

    pub fn with_timeout(
        base_url: Option<&str>,
        api_key: &str,
        webhook_token: &str,
        timeout: Duration,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client(timeout)?,
            base_url: base_url
                .unwrap_or(DEFAULT_API_URL)
                .trim_end_matches('/')
                .to_owned(),
            api_key: api_key.to_owned(),
            webhook_token: webhook_token.to_owned(),
        })
    }

    /// One call: the answer's JSON, or why not. Only the status and the field names Asaas gives in its
    /// `errors` are logged: a body can carry the payer's data.
    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> Result<Value, Failed> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url))
            .header("access_token", &self.api_key);
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body);
        }
        let response = request
            .send()
            .await
            // Without the address: it holds the customer's document in a query.
            .map_err(|error| {
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
        let why = answer["errors"][0]["description"]
            .as_str()
            .unwrap_or("no reason given")
            .chars()
            .take(200)
            .collect::<String>();
        match status {
            StatusCode::NOT_FOUND => Err(Failed::NotFound),
            // Our key being refused, a slow answer and the rate limit are not the payer's fault: trying
            // again later, or the operator fixing the key, is the remedy.
            StatusCode::UNAUTHORIZED
            | StatusCode::FORBIDDEN
            | StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_MANY_REQUESTS => Err(Failed::Provider(ProviderError::Unavailable(
                format!("{status}"),
            ))),
            status if status.is_client_error() => Err(Failed::Provider(ProviderError::Rejected(
                format!("{status}: {why}"),
            ))),
            status => Err(Failed::Provider(ProviderError::Unavailable(format!(
                "{status}"
            )))),
        }
    }

    /// The payer's id at Asaas: the customer with that CPF/CNPJ if there is one, otherwise a new one.
    async fn customer_for(&self, request: &ProviderChargeRequest) -> Result<String, ProviderError> {
        self.find_or_make_customer(request)
            .await
            .map_err(ProviderError::from)
    }

    async fn find_or_make_customer(
        &self,
        request: &ProviderChargeRequest,
    ) -> Result<String, Failed> {
        let Some(document) = request.payer.document.as_deref() else {
            return Err(Failed::Provider(ProviderError::Rejected(
                "Asaas bills a CPF or a CNPJ and the customer has none".to_owned(),
            )));
        };
        let found = self
            .call(
                Method::GET,
                &format!("/v3/customers?cpfCnpj={document}"),
                None,
            )
            .await?;
        if let Some(id) = found["data"][0]["id"].as_str() {
            return Ok(id.to_owned());
        }
        let mut body = json!({ "name": request.payer.name, "cpfCnpj": document });
        if let Some(email) = &request.payer.email {
            body["email"] = json!(email);
        }
        let made = self
            .call(Method::POST, "/v3/customers", Some(body.to_string()))
            .await?;
        text(&made, "id").map_err(Failed::Provider)
    }
}

fn text(value: &Value, key: &str) -> Result<String, ProviderError> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ProviderError::Unavailable(format!("the answer has no {key}")))
}

impl PaymentProvider for AsaasProvider {
    fn key(&self) -> &str {
        "asaas"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn create_charge(
        &self,
        request: &ProviderChargeRequest,
    ) -> Result<ProviderCharge, ProviderError> {
        let customer = self.customer_for(request).await?;
        let billing_type = match request.method {
            ChargeMethod::Boleto => "BOLETO",
            ChargeMethod::Pix => "PIX",
            // Without card data the payment comes with a page (`invoiceUrl`) where the payer types it.
            ChargeMethod::CardLink => "CREDIT_CARD",
        };
        let body = json!({
            "customer": customer,
            "billingType": billing_type,
            "value": VALUE_PLACEHOLDER,
            "dueDate": request.due_on.to_string(),
            "description": request.description.chars().take(MAX_DESCRIPTION_CHARS).collect::<String>(),
            "externalReference": request.charge_id.as_uuid().to_string(),
        })
        .to_string()
        // The whole pair, so a description that says the same words (its quotes are escaped) is left alone.
        .replace(
            &format!("\"value\":\"{VALUE_PLACEHOLDER}\""),
            &format!("\"value\":{}", request.amount),
        );
        let made = self
            .call(Method::POST, "/v3/payments", Some(body))
            .await
            .map_err(ProviderError::from)?;
        let id = text(&made, "id")?;
        match self.payment_details(request.method, &id, &made).await {
            Ok(payment) => Ok(ProviderCharge {
                external_id: id,
                payment,
            }),
            Err(error) => {
                // A payment nobody can see how to pay is called off, so it cannot be paid in vain.
                if let Err(cleanup) = self.cancel_charge(&id).await {
                    tracing::warn!(%cleanup, "an Asaas payment could not be taken back");
                }
                Err(error)
            }
        }
    }

    async fn cancel_charge(&self, external_id: &str) -> Result<(), ProviderError> {
        match self
            .call(Method::DELETE, &format!("/v3/payments/{external_id}"), None)
            .await
        {
            // Already gone is what was asked for.
            Ok(_) | Err(Failed::NotFound) => Ok(()),
            Err(Failed::Provider(error)) => Err(error),
        }
    }

    fn verify_webhook(
        &self,
        headers: &WebhookHeaders,
        body: &[u8],
    ) -> Result<Vec<ProviderEvent>, WebhookError> {
        let authentic = headers
            .get(TOKEN_HEADER)
            .is_some_and(|token| constant_time_eq(token.as_bytes(), self.webhook_token.as_bytes()));
        if !authentic {
            return Err(WebhookError::InvalidSignature);
        }
        let malformed = |what: &str| WebhookError::Malformed(what.to_owned());
        let notice: Value = serde_json::from_slice(body).map_err(|_| malformed("not JSON"))?;
        let payment = &notice["payment"];
        let event = notice["event"].as_str().ok_or_else(|| malformed("event"))?;
        let kind = match event {
            // A boleto or a Pix is paid when it is received; a card is confirmed first and received weeks
            // later, and the payment is the confirmation.
            "PAYMENT_RECEIVED" => paid(payment).map_err(malformed)?,
            "PAYMENT_CONFIRMED" if payment["billingType"] == "CREDIT_CARD" => {
                paid(payment).map_err(malformed)?
            }
            "PAYMENT_DELETED" => ProviderEventKind::Cancelled,
            // The rest (overdue, updated, refunded...) changes nothing here: acknowledged and left.
            _ => return Ok(Vec::new()),
        };
        Ok(vec![ProviderEvent {
            event_id: notice["id"]
                .as_str()
                .ok_or_else(|| malformed("id"))?
                .to_owned(),
            external_id: payment["id"]
                .as_str()
                .ok_or_else(|| malformed("payment.id"))?
                .to_owned(),
            kind,
        }])
    }
}

impl AsaasProvider {
    /// What the customer pays with, read from the payment just made and, for a Pix and a boleto, from the
    /// payment's own pages.
    async fn payment_details(
        &self,
        method: ChargeMethod,
        id: &str,
        made: &Value,
    ) -> Result<ChargePayment, ProviderError> {
        Ok(match method {
            ChargeMethod::Pix => {
                let qr = self
                    .call(Method::GET, &format!("/v3/payments/{id}/pixQrCode"), None)
                    .await?;
                ChargePayment {
                    pix_payload: Some(text(&qr, "payload")?),
                    ..ChargePayment::default()
                }
            }
            ChargeMethod::Boleto => {
                let line = self
                    .call(
                        Method::GET,
                        &format!("/v3/payments/{id}/identificationField"),
                        None,
                    )
                    .await?;
                ChargePayment {
                    digitable_line: Some(text(&line, "identificationField")?),
                    payment_url: made["bankSlipUrl"].as_str().map(str::to_owned),
                    ..ChargePayment::default()
                }
            }
            ChargeMethod::CardLink => ChargePayment {
                payment_url: Some(text(made, "invoiceUrl")?),
                ..ChargePayment::default()
            },
        })
    }
}

/// What was paid, from the payment object of a notice: the day, the amount, and the fee as the part of it
/// Asaas did not pass on (`value` less `netValue`). The amounts are JSON numbers; serde_json reads them as f64 and
/// the shortest text that gives that float back is what is parsed, which is exact for any amount in cents of
/// a realistic size (and an amount with more decimals is refused as malformed, never rounded).
fn paid(payment: &Value) -> Result<ProviderEventKind, &'static str> {
    let decimal =
        |key: &str| -> Option<Decimal> { payment[key].as_number()?.to_string().parse().ok() };
    let value = decimal("value").ok_or("payment.value")?;
    let net = decimal("netValue").unwrap_or(value);
    let day = payment["paymentDate"]
        .as_str()
        .or_else(|| payment["confirmedDate"].as_str())
        .and_then(CalendarDate::parse)
        .ok_or("payment.paymentDate")?;
    let cents = |amount: Decimal| Amount::in_cents(&amount.normalize().to_string());
    Ok(ProviderEventKind::Paid {
        paid_amount: cents(value).ok_or("payment.value")?,
        paid_on: day,
        fee: cents((value - net).max(Decimal::ZERO)).ok_or("payment.netValue")?,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::{Path, Query, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{delete, get, post};
    use conta_verde_application::Payer;
    use conta_verde_domain::ChargeId;

    use super::*;

    const KEY: &str = "$aact_test_key";
    const TOKEN: &str = "a-webhook-token-of-at-least-32-characters";

    // A local stand-in for the Asaas API: the tests never reach the real one. It checks the API key and
    // keeps what it was sent. Documents ending in 9 are refused, 8 make the service fail.
    #[derive(Default)]
    struct Fake {
        customers: HashMap<String, String>,
        created_customers: Vec<Value>,
        created_payments: Vec<String>,
        deleted: Vec<String>,
        qr_fails: bool,
    }

    type Shared = Arc<Mutex<Fake>>;

    fn authorized(headers: &HeaderMap) -> bool {
        headers
            .get("access_token")
            .is_some_and(|value| value == KEY)
            && headers.contains_key("user-agent")
    }

    async fn serve(fake: Shared) -> SocketAddr {
        let router = Router::new()
            .route(
                "/v3/customers",
                get(
                    |State(fake): State<Shared>,
                     headers: HeaderMap,
                     Query(query): Query<HashMap<String, String>>| async move {
                        if !authorized(&headers) {
                            return (StatusCode::UNAUTHORIZED, axum::Json(json!({}))).into_response_pair();
                        }
                        let document = query.get("cpfCnpj").cloned().unwrap_or_default();
                        let found = fake.lock().unwrap().customers.get(&document).cloned();
                        let data: Vec<Value> = found.into_iter().map(|id| json!({ "id": id })).collect();
                        (StatusCode::OK, axum::Json(json!({ "data": data }))).into_response_pair()
                    },
                )
                .post(
                    |State(fake): State<Shared>, headers: HeaderMap, body: String| async move {
                        if !authorized(&headers) {
                            return (StatusCode::UNAUTHORIZED, axum::Json(json!({}))).into_response_pair();
                        }
                        let body: Value = serde_json::from_str(&body).unwrap();
                        let document = body["cpfCnpj"].as_str().unwrap().to_owned();
                        if document.ends_with('9') {
                            return (
                                StatusCode::BAD_REQUEST,
                                axum::Json(json!({"errors": [{"description": "CPF/CNPJ inválido"}]})),
                            )
                                .into_response_pair();
                        }
                        if document.ends_with('8') {
                            return (StatusCode::BAD_GATEWAY, axum::Json(json!({}))).into_response_pair();
                        }
                        let mut fake = fake.lock().unwrap();
                        let id = format!("cus_{}", fake.customers.len() + 1);
                        fake.customers.insert(document, id.clone());
                        fake.created_customers.push(body);
                        (StatusCode::OK, axum::Json(json!({ "id": id }))).into_response_pair()
                    },
                ),
            )
            .route(
                "/v3/payments",
                post(|State(fake): State<Shared>, headers: HeaderMap, body: String| async move {
                    if !authorized(&headers) {
                        return (StatusCode::UNAUTHORIZED, axum::Json(json!({}))).into_response_pair();
                    }
                    let mut fake = fake.lock().unwrap();
                    fake.created_payments.push(body);
                    let id = format!("pay_{}", fake.created_payments.len());
                    (
                        StatusCode::OK,
                        axum::Json(json!({
                            "id": id,
                            "status": "PENDING",
                            "invoiceUrl": format!("https://sandbox.asaas.com/i/{id}"),
                            "bankSlipUrl": format!("https://sandbox.asaas.com/b/{id}"),
                        })),
                    )
                        .into_response_pair()
                }),
            )
            .route(
                "/v3/payments/{id}/pixQrCode",
                get(|State(fake): State<Shared>, Path(id): Path<String>| async move {
                    if fake.lock().unwrap().qr_fails {
                        return (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({}))).into_response_pair();
                    }
                    (
                        StatusCode::OK,
                        axum::Json(json!({ "payload": format!("00020126580014br.gov.bcb.pix{id}"), "encodedImage": "x" })),
                    )
                        .into_response_pair()
                }),
            )
            .route(
                "/v3/payments/{id}/identificationField",
                get(|Path(_id): Path<String>| async move {
                    (
                        StatusCode::OK,
                        axum::Json(json!({ "identificationField": "00190.00009 01234.567890 12345.678901 1 23450000010050" })),
                    )
                        .into_response_pair()
                }),
            )
            .route(
                "/v3/payments/{id}",
                delete(|State(fake): State<Shared>, Path(id): Path<String>| async move {
                    if id == "pay_gone" {
                        return (StatusCode::NOT_FOUND, axum::Json(json!({}))).into_response_pair();
                    }
                    if id == "pay_paid" {
                        return (
                            StatusCode::BAD_REQUEST,
                            axum::Json(json!({"errors": [{"description": "cobrança já recebida"}]})),
                        )
                            .into_response_pair();
                    }
                    fake.lock().unwrap().deleted.push(id.clone());
                    (StatusCode::OK, axum::Json(json!({ "deleted": true, "id": id }))).into_response_pair()
                }),
            )
            .with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        addr
    }

    trait Pair {
        fn into_response_pair(self) -> (StatusCode, axum::Json<Value>);
    }

    impl Pair for (StatusCode, axum::Json<Value>) {
        fn into_response_pair(self) -> (StatusCode, axum::Json<Value>) {
            self
        }
    }

    fn provider(addr: SocketAddr, key: &str) -> AsaasProvider {
        AsaasProvider::with_timeout(
            Some(&format!("http://{addr}/")),
            key,
            TOKEN,
            Duration::from_secs(2),
        )
        .unwrap()
    }

    fn request(method: ChargeMethod, document: Option<&str>) -> ProviderChargeRequest {
        ProviderChargeRequest {
            charge_id: ChargeId::generate(),
            method,
            amount: Amount::in_cents("150.50").unwrap(),
            due_on: CalendarDate::parse("2026-12-10").unwrap(),
            payer: Payer {
                name: "Mercado Boa Compra".into(),
                document: document.map(str::to_owned),
                email: Some("compras@example.com".into()),
            },
            description: "Venda 12 (1/2)".into(),
        }
    }

    // Homologation: run by hand against the sandbox with `ASAAS_SANDBOX_API_KEY` set. CI runs the ignored tests
    // but has no key, so without it this does nothing. It makes a Pix, a boleto and a card page for a valid test
    // CPF, then calls them off.
    #[tokio::test]
    #[ignore = "needs ASAAS_SANDBOX_API_KEY and the network"]
    async fn the_sandbox_makes_and_calls_off_each_kind_of_charge() {
        let Ok(key) = std::env::var("ASAAS_SANDBOX_API_KEY") else {
            eprintln!("ASAAS_SANDBOX_API_KEY is not set: the sandbox is not asked");
            return;
        };
        let asaas = AsaasProvider::new(Some("https://api-sandbox.asaas.com"), &key, TOKEN).unwrap();
        for method in [
            ChargeMethod::Pix,
            ChargeMethod::Boleto,
            ChargeMethod::CardLink,
        ] {
            let mut asked = request(method, Some("52998224725"));
            asked.due_on = CalendarDate::parse("2099-12-31").unwrap();
            let made = asaas.create_charge(&asked).await.unwrap();
            asaas.cancel_charge(&made.external_id).await.unwrap();
        }
    }

    #[tokio::test]
    async fn makes_a_pix_with_a_new_customer_and_the_exact_amount() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let asked = request(ChargeMethod::Pix, Some("52998224725"));
        let made = provider(addr, KEY).create_charge(&asked).await.unwrap();
        assert_eq!(made.external_id, "pay_1");
        assert!(made.payment.pix_payload.unwrap().ends_with("pay_1"));

        let fake = fake.lock().unwrap();
        assert_eq!(fake.created_customers.len(), 1);
        assert_eq!(fake.created_customers[0]["cpfCnpj"], "52998224725");
        assert_eq!(fake.created_customers[0]["email"], "compras@example.com");
        // The amount is the decimal text in the body, not a float, and our id travels as the reference.
        let body = &fake.created_payments[0];
        assert!(
            body.contains("\"value\":150.50") || body.contains("\"value\":150.5"),
            "{body}"
        );
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["customer"], "cus_1");
        assert_eq!(body["billingType"], "PIX");
        assert_eq!(body["dueDate"], "2026-12-10");
        assert_eq!(
            body["externalReference"],
            asked.charge_id.as_uuid().to_string()
        );
        assert_eq!(body["description"], "Venda 12 (1/2)");
    }

    #[tokio::test]
    async fn reuses_the_customer_that_has_the_document_and_makes_a_boleto_and_a_card_page() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let asaas = provider(addr, KEY);
        let boleto = asaas
            .create_charge(&request(ChargeMethod::Boleto, Some("52998224725")))
            .await
            .unwrap();
        assert_eq!(
            boleto.payment.digitable_line.as_deref(),
            Some("00190.00009 01234.567890 12345.678901 1 23450000010050")
        );
        assert_eq!(
            boleto.payment.payment_url.as_deref(),
            Some("https://sandbox.asaas.com/b/pay_1")
        );
        let card = asaas
            .create_charge(&request(ChargeMethod::CardLink, Some("52998224725")))
            .await
            .unwrap();
        assert_eq!(
            card.payment.payment_url.as_deref(),
            Some("https://sandbox.asaas.com/i/pay_2")
        );
        let fake = fake.lock().unwrap();
        assert_eq!(
            fake.created_customers.len(),
            1,
            "the customer is found by the document"
        );
        let card_body: Value = serde_json::from_str(&fake.created_payments[1]).unwrap();
        assert_eq!(card_body["billingType"], "CREDIT_CARD");
    }

    #[tokio::test]
    async fn refuses_without_asking_a_payer_with_no_document_and_says_why_otherwise() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let asaas = provider(addr, KEY);
        assert!(matches!(
            asaas.create_charge(&request(ChargeMethod::Pix, None)).await,
            Err(ProviderError::Rejected(_))
        ));
        assert!(fake.lock().unwrap().created_customers.is_empty());

        // A refusal of the provider is a rejection; its own failure, a wrong key and a dead server are not.
        assert!(matches!(
            asaas.create_charge(&request(ChargeMethod::Pix, Some("00000000009"))).await,
            Err(ProviderError::Rejected(why)) if why.contains("CPF/CNPJ inválido")
        ));
        assert!(matches!(
            asaas
                .create_charge(&request(ChargeMethod::Pix, Some("00000000008")))
                .await,
            Err(ProviderError::Unavailable(_))
        ));
        assert!(matches!(
            provider(addr, "wrong")
                .create_charge(&request(ChargeMethod::Pix, Some("52998224725")))
                .await,
            Err(ProviderError::Unavailable(_))
        ));
        let gone = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        assert!(matches!(
            provider(gone, KEY)
                .create_charge(&request(ChargeMethod::Pix, Some("52998224725")))
                .await,
            Err(ProviderError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn takes_back_a_payment_whose_pix_text_could_not_be_read() {
        let fake = Shared::default();
        fake.lock().unwrap().qr_fails = true;
        let addr = serve(fake.clone()).await;
        let result = provider(addr, KEY)
            .create_charge(&request(ChargeMethod::Pix, Some("52998224725")))
            .await;
        assert!(matches!(result, Err(ProviderError::Unavailable(_))));
        assert_eq!(fake.lock().unwrap().deleted, ["pay_1"]);
    }

    #[tokio::test]
    async fn calls_a_payment_off_and_one_that_is_gone_is_called_off_too() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let asaas = provider(addr, KEY);
        asaas.cancel_charge("pay_7").await.unwrap();
        asaas.cancel_charge("pay_gone").await.unwrap();
        assert_eq!(fake.lock().unwrap().deleted, ["pay_7"]);
        assert!(matches!(
            asaas.cancel_charge("pay_paid").await,
            Err(ProviderError::Rejected(_))
        ));
    }

    fn webhook(token: Option<&str>) -> WebhookHeaders {
        WebhookHeaders::new(
            token
                .into_iter()
                .map(|token| (TOKEN_HEADER.to_owned(), token.to_owned())),
        )
    }

    fn notice(event: &str, billing: &str) -> Vec<u8> {
        json!({
            "id": "evt_1",
            "event": event,
            "payment": {
                "id": "pay_1", "billingType": billing, "value": 100, "netValue": 94.51,
                "paymentDate": "2026-12-09"
            }
        })
        .to_string()
        .into_bytes()
    }

    fn verify(token: Option<&str>, body: &[u8]) -> Result<Vec<ProviderEvent>, WebhookError> {
        AsaasProvider::with_timeout(None, KEY, TOKEN, Duration::from_secs(1))
            .unwrap()
            .verify_webhook(&webhook(token), body)
    }

    #[test]
    fn reads_what_was_paid_with_the_fee_as_what_asaas_did_not_pass_on() {
        let events = verify(Some(TOKEN), &notice("PAYMENT_RECEIVED", "PIX")).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "evt_1");
        assert_eq!(events[0].external_id, "pay_1");
        assert_eq!(
            events[0].kind,
            ProviderEventKind::Paid {
                paid_amount: Amount::in_cents("100").unwrap(),
                paid_on: CalendarDate::parse("2026-12-09").unwrap(),
                fee: Amount::in_cents("5.49").unwrap(),
            }
        );
    }

    #[test]
    fn a_card_is_paid_when_confirmed_and_a_boleto_only_when_received() {
        assert_eq!(
            verify(Some(TOKEN), &notice("PAYMENT_CONFIRMED", "CREDIT_CARD"))
                .unwrap()
                .len(),
            1
        );
        assert!(
            verify(Some(TOKEN), &notice("PAYMENT_CONFIRMED", "BOLETO"))
                .unwrap()
                .is_empty()
        );
        let deleted = verify(Some(TOKEN), &notice("PAYMENT_DELETED", "BOLETO")).unwrap();
        assert_eq!(deleted[0].kind, ProviderEventKind::Cancelled);
        // What changes nothing is acknowledged and left.
        for event in [
            "PAYMENT_OVERDUE",
            "PAYMENT_UPDATED",
            "PAYMENT_REFUNDED",
            "PAYMENT_CREATED",
        ] {
            assert!(
                verify(Some(TOKEN), &notice(event, "BOLETO"))
                    .unwrap()
                    .is_empty(),
                "{event}"
            );
        }
    }

    #[test]
    fn refuses_a_notice_without_the_token_or_with_another_and_one_that_is_not_a_notice() {
        let body = notice("PAYMENT_RECEIVED", "PIX");
        for token in [
            None,
            Some("wrong"),
            Some(""),
            Some("a-webhook-token-of-at-least-32-character"),
        ] {
            assert_eq!(
                verify(token, &body),
                Err(WebhookError::InvalidSignature),
                "{token:?}"
            );
        }
        for body in [
            &b"nope"[..],
            br#"{"event":"PAYMENT_RECEIVED","id":"e","payment":{"id":"p"}}"#,
            br#"{"id":"e"}"#,
        ] {
            assert!(matches!(
                verify(Some(TOKEN), body),
                Err(WebhookError::Malformed(_))
            ));
        }
    }
}
