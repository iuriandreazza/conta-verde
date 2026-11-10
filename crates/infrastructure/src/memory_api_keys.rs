use std::sync::{Arc, Mutex};

use conta_verde_application::{ApiKeyRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    ApiKey, ApiKeyId, AuditEntity, AuditEntry, AuditRecord, CalendarDate, CompanyId,
};

use crate::memory_audit::AuditLog;

/// Keeps API keys and their audit trail in process memory. Used by tests and for running the API without a database. Like
/// the Postgres adapter, it records an audit entry only for a write that happened.
#[derive(Default, Clone)]
pub struct InMemoryApiKeyRepository {
    state: Arc<Mutex<ApiKeyState>>,
}

#[derive(Default)]
struct ApiKeyState {
    /// The key and the digest of its secret.
    keys: Vec<(ApiKey, String)>,
    audit: AuditLog,
}

impl ApiKeyState {
    fn position(&self, company_id: CompanyId, id: ApiKeyId) -> Option<usize> {
        self.keys
            .iter()
            .position(|(key, _)| key.company_id() == company_id && key.id() == id)
    }

    fn insert(&mut self, key: &ApiKey, digest: &str) -> Result<(), RepositoryError> {
        // A digest is unique, and so is an id: the same secret never makes two keys.
        if self
            .keys
            .iter()
            .any(|(stored, stored_digest)| stored.id() == key.id() || stored_digest == digest)
        {
            return Err(RepositoryError::Storage(
                "an API key with this id or secret already exists".to_owned(),
            ));
        }
        self.keys.push((key.clone(), digest.to_owned()));
        Ok(())
    }

    fn replace(&mut self, key: &ApiKey) -> Result<(), RepositoryError> {
        let position = self
            .position(key.company_id(), key.id())
            .ok_or(RepositoryError::NotFound)?;
        self.keys[position].0 = key.clone();
        Ok(())
    }
}

impl ApiKeyRepository for InMemoryApiKeyRepository {
    async fn save(
        &self,
        key: &ApiKey,
        digest: &str,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, key)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.insert(key, digest)?;
        state.audit.record(audit);
        Ok(())
    }

    async fn rotate(
        &self,
        new: &ApiKey,
        digest: &str,
        new_audit: &AuditEntry,
        old: &ApiKey,
        old_audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(new_audit, new)?;
        ensure_audit_describes(old_audit, old)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        // Checked first, so that a refusal leaves nothing behind: the lock is held through both writes.
        if state.position(old.company_id(), old.id()).is_none() {
            return Err(RepositoryError::NotFound);
        }
        state.insert(new, digest)?;
        state.replace(old)?;
        state.audit.record(new_audit);
        state.audit.record(old_audit);
        Ok(())
    }

    async fn update(&self, key: &ApiKey, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, key)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.replace(key)?;
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
    ) -> Result<Option<ApiKey>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .position(company_id, id)
            .map(|position| state.keys[position].0.clone()))
    }

    async fn find_by_digest(&self, digest: &str) -> Result<Option<ApiKey>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .keys
            .iter()
            .find(|(_, stored)| stored == digest)
            .map(|(key, _)| key.clone()))
    }

    async fn list(&self, company_id: CompanyId) -> Result<Vec<ApiKey>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut keys: Vec<ApiKey> = state
            .keys
            .iter()
            .filter(|(key, _)| key.company_id() == company_id)
            .map(|(key, _)| key.clone())
            .collect();
        // Newest first, the id breaking a tie, like the SQL adapter.
        keys.sort_by_key(|key| std::cmp::Reverse((key.created_at(), key.id().as_uuid())));
        Ok(keys)
    }

    async fn record_use(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
        day: CalendarDate,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .position(company_id, id)
            .ok_or(RepositoryError::NotFound)?;
        let used = state.keys[position].0.used_on(day);
        state.keys[position].0 = used;
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::ApiKey, id.as_uuid()))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::ApiKeyService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::{Argon2SecretService, contract_integrations};

    #[tokio::test]
    async fn keeps_keys_by_the_digest_of_their_secret_and_never_the_secret() {
        let service = ApiKeyService::new(
            InMemoryApiKeyRepository::default(),
            Argon2SecretService::new().unwrap(),
        );
        contract_integrations::keeps_api_keys_by_the_digest_of_their_secret(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn a_rotation_and_a_revocation_end_a_key_and_are_in_the_trail() {
        let service = ApiKeyService::new(
            InMemoryApiKeyRepository::default(),
            Argon2SecretService::new().unwrap(),
        );
        contract_integrations::rotates_and_revokes_api_keys(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }
}
