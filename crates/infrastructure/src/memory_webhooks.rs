use std::sync::{Arc, Mutex};

use conta_verde_application::{
    DeliveryCursor, RepositoryError, WebhookRepository, WebhookSecrets, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, DeliveryStatus, SealedSecret, WebhookDelivery,
    WebhookDeliveryId, WebhookEventKind, WebhookSubscription, WebhookSubscriptionId,
};
use uuid::Uuid;

use crate::memory_audit::AuditLog;

/// Keeps webhook subscriptions, their sealed secrets and the log of deliveries in process memory. Used by tests and for running
/// the API without a database.
#[derive(Default, Clone)]
pub struct InMemoryWebhookRepository {
    state: Arc<Mutex<WebhookState>>,
}

struct Stored {
    subscription: WebhookSubscription,
    secrets: WebhookSecrets,
}

#[derive(Default)]
struct WebhookState {
    subscriptions: Vec<Stored>,
    deliveries: Vec<WebhookDelivery>,
    audit: AuditLog,
}

impl WebhookState {
    fn position(&self, company_id: CompanyId, id: WebhookSubscriptionId) -> Option<usize> {
        self.subscriptions.iter().position(|stored| {
            stored.subscription.company_id() == company_id && stored.subscription.id() == id
        })
    }

    fn new_delivery(
        &mut self,
        subscription: &WebhookSubscription,
        event_id: Uuid,
        kind: WebhookEventKind,
        payload: &str,
        now: i64,
    ) -> Option<WebhookDelivery> {
        let made_before = self.deliveries.iter().any(|delivery| {
            delivery.subscription_id == subscription.id() && delivery.event_id == event_id
        });
        if made_before {
            return None;
        }
        let delivery = WebhookDelivery {
            id: WebhookDeliveryId::generate(),
            company_id: subscription.company_id(),
            subscription_id: subscription.id(),
            event_id,
            kind,
            payload: payload.to_owned(),
            status: DeliveryStatus::Pending,
            attempts: 0,
            last_status: None,
            last_error: None,
            created_at: now,
            updated_at: now,
            delivered_at: None,
        };
        self.deliveries.push(delivery.clone());
        Some(delivery)
    }
}

impl WebhookRepository for InMemoryWebhookRepository {
    async fn save(
        &self,
        subscription: &WebhookSubscription,
        secret: &SealedSecret,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, subscription)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.subscriptions.push(Stored {
            subscription: subscription.clone(),
            secrets: WebhookSecrets {
                current: secret.clone(),
                previous: None,
            },
        });
        state.audit.record(audit);
        Ok(())
    }

    async fn update(
        &self,
        subscription: &WebhookSubscription,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, subscription)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .position(subscription.company_id(), subscription.id())
            .ok_or(RepositoryError::NotFound)?;
        state.subscriptions[position].subscription = subscription.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn replace_secret(
        &self,
        subscription: &WebhookSubscription,
        secret: &SealedSecret,
        previous_until: i64,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, subscription)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .position(subscription.company_id(), subscription.id())
            .ok_or(RepositoryError::NotFound)?;
        let secrets = &mut state.subscriptions[position].secrets;
        let replaced = std::mem::replace(&mut secrets.current, secret.clone());
        secrets.previous = Some((replaced, previous_until));
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<Option<WebhookSubscription>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .position(company_id, id)
            .map(|position| state.subscriptions[position].subscription.clone()))
    }

    async fn secrets_of(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<Option<WebhookSecrets>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .position(company_id, id)
            .map(|position| state.subscriptions[position].secrets.clone()))
    }

    async fn list(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<WebhookSubscription>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut subscriptions: Vec<WebhookSubscription> = state
            .subscriptions
            .iter()
            .filter(|stored| stored.subscription.company_id() == company_id)
            .map(|stored| stored.subscription.clone())
            .collect();
        subscriptions.sort_by_key(|sub| (sub.created_at(), sub.id().as_uuid()));
        Ok(subscriptions)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::WebhookSubscription, id.as_uuid()))
    }

    async fn publish(
        &self,
        company_id: CompanyId,
        event_id: Uuid,
        kind: WebhookEventKind,
        payload: &str,
        now: i64,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let wanting: Vec<WebhookSubscription> = state
            .subscriptions
            .iter()
            .filter(|stored| {
                stored.subscription.company_id() == company_id && stored.subscription.wants(kind)
            })
            .map(|stored| stored.subscription.clone())
            .collect();
        Ok(wanting
            .iter()
            .filter_map(|subscription| {
                state.new_delivery(subscription, event_id, kind, payload, now)
            })
            .collect())
    }

    async fn deliver_to(
        &self,
        subscription: &WebhookSubscription,
        event_id: Uuid,
        kind: WebhookEventKind,
        payload: &str,
        now: i64,
    ) -> Result<Option<WebhookDelivery>, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        Ok(state.new_delivery(subscription, event_id, kind, payload, now))
    }

    async fn find_delivery(
        &self,
        id: WebhookDeliveryId,
    ) -> Result<Option<WebhookDelivery>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.deliveries.iter().find(|d| d.id == id).cloned())
    }

    async fn record_attempt(
        &self,
        id: WebhookDeliveryId,
        status: DeliveryStatus,
        attempts: u32,
        last_status: Option<u16>,
        last_error: Option<&str>,
        now: i64,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let delivery = state
            .deliveries
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or(RepositoryError::NotFound)?;
        delivery.status = status;
        delivery.attempts = attempts;
        delivery.last_status = last_status;
        delivery.last_error = last_error.map(str::to_owned);
        delivery.updated_at = now;
        delivery.delivered_at = (status == DeliveryStatus::Delivered).then_some(now);
        Ok(())
    }

    async fn reset_delivery(
        &self,
        company_id: CompanyId,
        subscription_id: WebhookSubscriptionId,
        id: WebhookDeliveryId,
        now: i64,
    ) -> Result<Option<WebhookDelivery>, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let Some(delivery) = state.deliveries.iter_mut().find(|d| {
            d.id == id
                && d.company_id == company_id
                && d.subscription_id == subscription_id
                && d.status != DeliveryStatus::Pending
        }) else {
            return Ok(None);
        };
        delivery.status = DeliveryStatus::Pending;
        delivery.attempts = 0;
        delivery.last_status = None;
        delivery.last_error = None;
        delivery.delivered_at = None;
        delivery.updated_at = now;
        Ok(Some(delivery.clone()))
    }

    async fn stale_pending(
        &self,
        before: i64,
        limit: usize,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut stale: Vec<WebhookDelivery> = state
            .deliveries
            .iter()
            .filter(|d| d.status == DeliveryStatus::Pending && d.updated_at < before)
            .cloned()
            .collect();
        stale.sort_by_key(|d| (d.updated_at, d.id.as_uuid()));
        stale.truncate(limit);
        Ok(stale)
    }

    async fn list_deliveries(
        &self,
        company_id: CompanyId,
        subscription_id: WebhookSubscriptionId,
        after: Option<&DeliveryCursor>,
        limit: usize,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<WebhookDelivery> = state
            .deliveries
            .iter()
            .filter(|d| d.company_id == company_id && d.subscription_id == subscription_id)
            .filter(|d| {
                after.is_none_or(|cursor| {
                    (d.created_at, d.id.as_uuid()) < (cursor.created_at, cursor.id)
                })
            })
            .cloned()
            .collect();
        found.sort_by_key(|d| std::cmp::Reverse((d.created_at, d.id.as_uuid())));
        found.truncate(limit);
        Ok(found)
    }

    async fn prune_before(&self, before: i64) -> Result<u64, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let kept = state.deliveries.len();
        state.deliveries.retain(|d| d.created_at >= before);
        Ok((kept - state.deliveries.len()) as u64)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::WebhookService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::{
        Argon2SecretService, InMemoryJobQueue, NoSecretCipher, RingSecretCipher,
        contract_integrations,
    };

    #[tokio::test]
    async fn runs_webhooks() {
        let service = WebhookService::new(
            InMemoryWebhookRepository::default(),
            RingSecretCipher::from_hex(&"11".repeat(32)).unwrap(),
            InMemoryJobQueue::default(),
            Argon2SecretService::new().unwrap(),
            false,
        );
        contract_integrations::runs_webhooks(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn webhooks_are_off_without_the_key() {
        let service = WebhookService::new(
            InMemoryWebhookRepository::default(),
            NoSecretCipher,
            InMemoryJobQueue::default(),
            Argon2SecretService::new().unwrap(),
            false,
        );
        assert!(!service.is_available());
        let draft = conta_verde_domain::WebhookSubscriptionDraft {
            url: "https://hooks.example.com/x".into(),
            events: vec![],
            active: true,
        };
        let refused = service
            .create(CompanyId::generate(), &crate::contract::actor(), draft, 0)
            .await;
        assert_eq!(
            refused.unwrap_err(),
            conta_verde_application::ApplicationError::WebhooksOff
        );
    }
}
