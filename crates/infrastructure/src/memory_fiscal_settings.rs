use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{FiscalSettingsRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{AuditEntity, AuditEntry, AuditRecord, CompanyId, FiscalSettings};

use crate::memory_audit::AuditLog;

/// Keeps the fiscal settings and their audit trail in process memory, for tests and for running the API without a
/// database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryFiscalSettingsRepository {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    settings: HashMap<CompanyId, FiscalSettings>,
    audit: AuditLog,
}

impl FiscalSettingsRepository for InMemoryFiscalSettingsRepository {
    async fn find(&self, company_id: CompanyId) -> Result<Option<FiscalSettings>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.settings.get(&company_id).cloned())
    }

    async fn save(
        &self,
        settings: &FiscalSettings,
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
            AuditEntity::FiscalSettings,
            company_id.as_uuid(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn stores_the_fiscal_settings() {
        contract::stores_the_fiscal_settings(
            &InMemoryFiscalSettingsRepository::default(),
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn replaces_and_audits_the_fiscal_settings() {
        contract::fiscal_settings_are_replaced_and_audited(
            &conta_verde_application::FiscalSettingsService::new(
                InMemoryFiscalSettingsRepository::default(),
            ),
            CompanyId::generate(),
        )
        .await;
    }
}
