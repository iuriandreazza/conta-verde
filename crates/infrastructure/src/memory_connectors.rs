use std::sync::{Arc, Mutex};

use conta_verde_application::{ConnectorRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{AuditEntity, AuditEntry, AuditRecord, CompanyId, Connector, ConnectorId};

use crate::memory_audit::AuditLog;

/// Keeps connectors and their audit trail in process memory. Used by tests and for running the API without a database.
#[derive(Default, Clone)]
pub struct InMemoryConnectorRepository {
    state: Arc<Mutex<ConnectorState>>,
}

#[derive(Default)]
struct ConnectorState {
    connectors: Vec<Connector>,
    audit: AuditLog,
}

impl ConnectorRepository for InMemoryConnectorRepository {
    async fn save(&self, connector: &Connector, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, connector)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let taken = state.connectors.iter().any(|stored| {
            stored.company_id() == connector.company_id()
                && stored.name().to_lowercase() == connector.name().to_lowercase()
        });
        if taken {
            return Err(RepositoryError::DuplicateName);
        }
        state.connectors.push(connector.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn switch(
        &self,
        connector: &Connector,
        was_active: bool,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, connector)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .connectors
            .iter_mut()
            .find(|stored| {
                stored.company_id() == connector.company_id() && stored.id() == connector.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if stored.is_active() != was_active {
            return Err(RepositoryError::ConnectorChanged);
        }
        *stored = connector.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ConnectorId,
    ) -> Result<Option<Connector>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .connectors
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn list(&self, company_id: CompanyId) -> Result<Vec<Connector>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<Connector> = state
            .connectors
            .iter()
            .filter(|stored| stored.company_id() == company_id)
            .cloned()
            .collect();
        found.sort_by_key(|stored| std::cmp::Reverse((stored.created_at(), stored.id().as_uuid())));
        Ok(found)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ConnectorId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Connector, id.as_uuid()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use conta_verde_application::{ApiKeyService, ConnectorService, WebhookService};
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::{
        Argon2SecretService, InMemoryApiKeyRepository, InMemoryJobQueue, InMemoryWebhookRepository,
        RingSecretCipher, contract_integrations,
    };

    #[tokio::test]
    async fn runs_connectors() {
        let keys = Arc::new(ApiKeyService::new(
            InMemoryApiKeyRepository::default(),
            Argon2SecretService::new().unwrap(),
        ));
        let hooks = Arc::new(WebhookService::new(
            InMemoryWebhookRepository::default(),
            RingSecretCipher::from_hex(&"11".repeat(32)).unwrap(),
            InMemoryJobQueue::default(),
            Argon2SecretService::new().unwrap(),
            false,
        ));
        let repository = InMemoryConnectorRepository::default();
        let service = ConnectorService::new(repository.clone(), keys.clone(), hooks.clone());
        contract_integrations::runs_connectors(
            &service,
            &repository,
            &keys,
            &hooks,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }
}
