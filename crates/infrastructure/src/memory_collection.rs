use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{
    CollectionSettingsRepository, LinkTarget, PaymentEventRepository, PaymentLinkRepository,
    Recorded, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, ChargeId, CollectionSettings, CompanyId, PaymentEvent,
    PaymentEventId, PaymentEventOutcome,
};

use crate::memory_audit::AuditLog;

/// Keeps the collection settings and their audit trail in process memory, for tests and for running the
/// API without a database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryCollectionSettingsRepository {
    state: Arc<Mutex<SettingsState>>,
}

#[derive(Default)]
struct SettingsState {
    settings: HashMap<CompanyId, CollectionSettings>,
    audit: AuditLog,
}

impl CollectionSettingsRepository for InMemoryCollectionSettingsRepository {
    async fn find(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<CollectionSettings>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.settings.get(&company_id).cloned())
    }

    async fn save(
        &self,
        settings: &CollectionSettings,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, settings)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state
            .settings
            .insert(settings.company_id(), settings.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn history(&self, company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.audit.history(
            company_id,
            AuditEntity::CollectionSettings,
            company_id.as_uuid(),
        ))
    }
}

/// Keeps the notices of payment providers in process memory, for tests and for running the API without a
/// database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryPaymentEventRepository {
    events: Arc<Mutex<Vec<PaymentEvent>>>,
}

impl PaymentEventRepository for InMemoryPaymentEventRepository {
    async fn record(&self, event: &PaymentEvent) -> Result<Recorded, RepositoryError> {
        let mut events = self.events.lock().expect("repository lock poisoned");
        if let Some(stored) = events.iter().find(|stored| {
            stored.company_id() == event.company_id()
                && stored.provider() == event.provider()
                && stored.event_id() == event.event_id()
        }) {
            return Ok(Recorded::Existing(stored.clone()));
        }
        events.push(event.clone());
        Ok(Recorded::New(event.clone()))
    }

    async fn finish(
        &self,
        company_id: CompanyId,
        id: PaymentEventId,
        outcome: PaymentEventOutcome,
        note: Option<&str>,
    ) -> Result<(), RepositoryError> {
        let mut events = self.events.lock().expect("repository lock poisoned");
        let stored = events
            .iter_mut()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .ok_or(RepositoryError::NotFound)?;
        *stored = stored.clone().with_outcome(outcome, note);
        Ok(())
    }
}

/// Keeps the payment links in process memory, for tests and for running the API without a database.
/// Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryPaymentLinkRepository {
    links: Arc<Mutex<HashMap<String, LinkTarget>>>,
}

impl PaymentLinkRepository for InMemoryPaymentLinkRepository {
    async fn save(
        &self,
        company_id: CompanyId,
        charge_id: ChargeId,
        digest: &str,
    ) -> Result<(), RepositoryError> {
        self.links.lock().expect("repository lock poisoned").insert(
            digest.to_owned(),
            LinkTarget {
                company_id,
                charge_id,
            },
        );
        Ok(())
    }

    async fn find(&self, digest: &str) -> Result<Option<LinkTarget>, RepositoryError> {
        Ok(self
            .links
            .lock()
            .expect("repository lock poisoned")
            .get(digest)
            .copied())
    }
}
