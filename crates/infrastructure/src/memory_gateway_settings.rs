use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{GatewaySettingsRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{AuditEntity, AuditEntry, AuditRecord, CompanyId, GatewaySettings};

use crate::memory_audit::AuditLog;

/// Keeps the gateway settings (their secrets sealed) and their audit trail in process memory, for tests and for running the API
/// without a database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryGatewaySettingsRepository {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    settings: HashMap<CompanyId, GatewaySettings>,
    audit: AuditLog,
}

impl GatewaySettingsRepository for InMemoryGatewaySettingsRepository {
    async fn find(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<GatewaySettings>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.settings.get(&company_id).cloned())
    }

    async fn save(
        &self,
        settings: &GatewaySettings,
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
            AuditEntity::GatewaySettings,
            company_id.as_uuid(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::{RingSecretCipher, contract};

    #[tokio::test]
    async fn keeps_gateway_settings_sealed_and_audited_without_a_secret() {
        // One store, two servers: the one with the key and the one without.
        let store = InMemoryGatewaySettingsRepository::default();
        contract::gateway_settings_are_sealed_audited_and_never_shown(
            &conta_verde_application::GatewaySettingsService::new(
                store.clone(),
                RingSecretCipher::from_hex(contract::SECRETS_KEY).unwrap(),
            ),
            &conta_verde_application::GatewaySettingsService::new(store, crate::NoSecretCipher),
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }
}
