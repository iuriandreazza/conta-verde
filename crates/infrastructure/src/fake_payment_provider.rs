use std::sync::{Arc, Mutex};

use conta_verde_application::{
    PaymentProvider, ProviderCharge, ProviderChargeRequest, ProviderError,
};
use conta_verde_domain::{ChargeMethod, ChargePayment};

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
}

impl FakePaymentProvider {
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
}
