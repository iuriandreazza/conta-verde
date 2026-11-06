use std::sync::{Arc, Mutex};

use conta_verde_application::{
    PaymentProvider, ProviderCharge, ProviderChargeRequest, ProviderError, ProviderEvent,
    ProviderEventKind, WebhookError, WebhookHeaders,
};
use conta_verde_domain::{Amount, CalendarDate, ChargeMethod, ChargePayment};

use crate::webhook_signature::{hmac_sha256_hex, is_valid_signature};

/// The secret the fake signs its webhooks with. It is public on purpose: the fake makes charges that cannot
/// be paid, and a test or a demo signs a notice with it to play the provider.
pub const FAKE_WEBHOOK_SECRET: &str = "fake-webhook-secret";

/// The header the fake's signature is in.
pub const FAKE_SIGNATURE_HEADER: &str = "x-fake-signature";

/// The adapter for when the operator configured no provider (`PAYMENT_PROVIDER` unset or `off`):
/// charges are refused before anything is asked, the way `CNPJ_LOOKUP=off` switches the registry off.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPaymentProvider;

impl PaymentProvider for NoPaymentProvider {
    fn key(&self) -> &str {
        "off"
    }

    fn is_enabled(&self) -> bool {
        false
    }

    async fn create_charge(
        &self,
        _request: &ProviderChargeRequest,
    ) -> Result<ProviderCharge, ProviderError> {
        Err(ProviderError::Disabled)
    }

    async fn cancel_charge(&self, _external_id: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Disabled)
    }

    fn verify_webhook(
        &self,
        _headers: &WebhookHeaders,
        _body: &[u8],
    ) -> Result<Vec<ProviderEvent>, WebhookError> {
        Err(WebhookError::InvalidSignature)
    }
}

/// A provider that answers from memory, for development and tests: it never reaches a network and what it
/// makes cannot be paid. Clones share what was asked of it, so a test can look at the calls.
#[derive(Debug, Default, Clone)]
pub struct FakePaymentProvider {
    state: Arc<Mutex<FakeState>>,
}

#[derive(Debug, Default)]
struct FakeState {
    created: Vec<ProviderChargeRequest>,
    cancelled: Vec<String>,
    failure: Option<ProviderError>,
    /// What webhooks are signed with: the public one unless the operator chose another.
    secret: Option<String>,
}

impl FakePaymentProvider {
    /// A fake whose webhooks must be signed with this secret and not the public one: what a running server
    /// uses, so that `PAYMENT_PROVIDER=fake` left on by mistake does not let anyone who read the source mark
    /// charges as paid.
    pub fn with_webhook_secret(secret: String) -> Self {
        let fake = Self::default();
        fake.lock().secret = Some(secret);
        fake
    }

    /// What it was asked to make, in order.
    pub fn created(&self) -> Vec<ProviderChargeRequest> {
        self.lock().created.clone()
    }

    /// The provider ids it was asked to call off, in order.
    pub fn cancelled(&self) -> Vec<String> {
        self.lock().cancelled.clone()
    }

    /// From now on every call fails with `error`, until `works_again`.
    pub fn failing_with(&self, error: ProviderError) {
        self.lock().failure = Some(error);
    }

    pub fn works_again(&self) {
        self.lock().failure = None;
    }

    /// A notice as the fake provider would send it, signed: the headers and the body of the request. `kind` is
    /// `paid`, `cancelled` or `expired`; a payment has what was paid, the day and the fee.
    pub fn notice(
        event_id: &str,
        external_id: &str,
        kind: &str,
        paid: Option<(&str, &str, &str)>,
    ) -> (WebhookHeaders, Vec<u8>) {
        let mut body = serde_json::json!({
            "event_id": event_id,
            "charge": external_id,
            "type": kind,
        });
        if let Some((amount, paid_on, fee)) = paid {
            body["amount"] = amount.into();
            body["paid_on"] = paid_on.into();
            body["fee"] = fee.into();
        }
        let body = body.to_string().into_bytes();
        let signature = hmac_sha256_hex(FAKE_WEBHOOK_SECRET.as_bytes(), &body);
        (
            WebhookHeaders::new([(FAKE_SIGNATURE_HEADER.to_owned(), signature)]),
            body,
        )
    }

    /// The id the fake gives to the charge of `request`: derived from our own id, so asking for the
    /// same charge twice names it the same.
    pub fn external_id_of(charge_id: conta_verde_domain::ChargeId) -> String {
        format!("fake_{}", charge_id.as_uuid().simple())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state.lock().expect("fake provider lock poisoned")
    }
}

impl PaymentProvider for FakePaymentProvider {
    fn key(&self) -> &str {
        "fake"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn create_charge(
        &self,
        request: &ProviderChargeRequest,
    ) -> Result<ProviderCharge, ProviderError> {
        let mut state = self.lock();
        if let Some(error) = &state.failure {
            return Err(error.clone());
        }
        state.created.push(request.clone());
        let external_id = Self::external_id_of(request.charge_id);
        // Zero padded, so the slices below always fit.
        let digits = format!("{:039}", request.charge_id.as_uuid().as_u128());
        let payment = match request.method {
            ChargeMethod::Boleto => ChargePayment {
                digitable_line: Some(format!("00190.00009 {} {}", &digits[..5], &digits[5..10])),
                payment_url: Some(format!(
                    "https://fake-provider.invalid/boleto/{external_id}"
                )),
                ..ChargePayment::default()
            },
            ChargeMethod::Pix => ChargePayment {
                pix_payload: Some(format!(
                    "00020126580014br.gov.bcb.pix0136{external_id}5204000053039865406{}6304FAKE",
                    request.amount
                )),
                ..ChargePayment::default()
            },
            ChargeMethod::CardLink => ChargePayment {
                payment_url: Some(format!("https://fake-provider.invalid/card/{external_id}")),
                ..ChargePayment::default()
            },
        };
        Ok(ProviderCharge {
            external_id,
            payment,
        })
    }

    async fn cancel_charge(&self, external_id: &str) -> Result<(), ProviderError> {
        let mut state = self.lock();
        if let Some(error) = &state.failure {
            return Err(error.clone());
        }
        state.cancelled.push(external_id.to_owned());
        Ok(())
    }

    fn verify_webhook(
        &self,
        headers: &WebhookHeaders,
        body: &[u8],
    ) -> Result<Vec<ProviderEvent>, WebhookError> {
        let secret = self
            .lock()
            .secret
            .clone()
            .unwrap_or_else(|| FAKE_WEBHOOK_SECRET.to_owned());
        let signed = headers
            .get(FAKE_SIGNATURE_HEADER)
            .is_some_and(|signature| is_valid_signature(secret.as_bytes(), body, signature));
        if !signed {
            return Err(WebhookError::InvalidSignature);
        }
        let malformed = |what: &str| WebhookError::Malformed(what.to_owned());
        let notice: serde_json::Value =
            serde_json::from_slice(body).map_err(|_| malformed("not JSON"))?;
        let text = |key: &str| notice.get(key).and_then(serde_json::Value::as_str);
        let kind = match text("type") {
            Some("paid") => ProviderEventKind::Paid {
                paid_amount: text("amount")
                    .and_then(Amount::in_cents)
                    .ok_or_else(|| malformed("amount"))?,
                paid_on: text("paid_on")
                    .and_then(CalendarDate::parse)
                    .ok_or_else(|| malformed("paid_on"))?,
                fee: text("fee")
                    .and_then(Amount::in_cents)
                    .ok_or_else(|| malformed("fee"))?,
            },
            Some("cancelled") => ProviderEventKind::Cancelled,
            Some("expired") => ProviderEventKind::Expired,
            _ => return Err(malformed("type")),
        };
        Ok(vec![ProviderEvent {
            event_id: text("event_id")
                .ok_or_else(|| malformed("event_id"))?
                .to_owned(),
            external_id: text("charge")
                .ok_or_else(|| malformed("charge"))?
                .to_owned(),
            kind,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verified(headers: &WebhookHeaders, body: &[u8]) -> Result<Vec<ProviderEvent>, WebhookError> {
        FakePaymentProvider::default().verify_webhook(headers, body)
    }

    #[test]
    fn a_fake_with_its_own_secret_refuses_what_the_public_one_signed() {
        let (headers, body) = FakePaymentProvider::notice("evt_1", "fake_abc", "expired", None);
        let own = FakePaymentProvider::with_webhook_secret("operator-secret".to_owned());
        assert_eq!(
            own.verify_webhook(&headers, &body),
            Err(WebhookError::InvalidSignature)
        );
        let signature = hmac_sha256_hex(b"operator-secret", &body);
        let signed = WebhookHeaders::new([(FAKE_SIGNATURE_HEADER.to_owned(), signature)]);
        assert!(own.verify_webhook(&signed, &body).is_ok());
    }

    #[test]
    fn reads_a_signed_payment_and_refuses_what_is_not_signed_or_not_a_notice() {
        let (headers, body) = FakePaymentProvider::notice(
            "evt_1",
            "fake_abc",
            "paid",
            Some(("152", "2026-12-09", "1.99")),
        );
        let events = verified(&headers, &body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "evt_1");
        assert_eq!(events[0].external_id, "fake_abc");
        assert_eq!(
            events[0].kind,
            ProviderEventKind::Paid {
                paid_amount: Amount::in_cents("152").unwrap(),
                paid_on: CalendarDate::parse("2026-12-09").unwrap(),
                fee: Amount::in_cents("1.99").unwrap(),
            }
        );

        // The body changed after it was signed, or no signature at all, or a wrong one.
        let mut altered = body.clone();
        altered.extend_from_slice(b" ");
        assert_eq!(
            verified(&headers, &altered),
            Err(WebhookError::InvalidSignature)
        );
        assert_eq!(
            verified(&WebhookHeaders::default(), &body),
            Err(WebhookError::InvalidSignature)
        );
        let wrong = WebhookHeaders::new([(FAKE_SIGNATURE_HEADER.to_owned(), "00".repeat(32))]);
        assert_eq!(verified(&wrong, &body), Err(WebhookError::InvalidSignature));

        // Signed, but not a notice.
        let signed = |body: &[u8]| {
            WebhookHeaders::new([(
                FAKE_SIGNATURE_HEADER.to_owned(),
                hmac_sha256_hex(FAKE_WEBHOOK_SECRET.as_bytes(), body),
            )])
        };
        for body in [
            &b"nope"[..],
            br#"{"type":"paid"}"#,
            br#"{"event_id":"e","charge":"c","type":"lost"}"#,
        ] {
            assert!(matches!(
                verified(&signed(body), body),
                Err(WebhookError::Malformed(_))
            ));
        }
    }
}
