use std::sync::{Arc, Mutex};

use conta_verde_application::{
    NameCursor, PaymentMethodQuery, PaymentMethodRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, PaymentMethod, PaymentMethodId,
};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

/// Keeps payment methods and their audit trail in process memory. Used by tests and for running
/// the API without a database. Like the Postgres adapter, it records an audit entry only for a
/// write that happened: a refused or missed write leaves nothing in the trail.
#[derive(Default, Clone)]
pub struct InMemoryPaymentMethodRepository {
    state: Arc<Mutex<PaymentMethodState>>,
}

#[derive(Default)]
struct PaymentMethodState {
    methods: Vec<PaymentMethod>,
    audit: AuditLog,
}

impl PaymentMethodState {
    /// Whether another payment method of the company already has the name, ignoring case.
    fn name_taken(&self, method: &PaymentMethod) -> bool {
        self.methods.iter().any(|other| {
            other.company_id() == method.company_id()
                && other.id() != method.id()
                && other.name().to_lowercase() == method.name().to_lowercase()
        })
    }
}

impl PaymentMethodRepository for InMemoryPaymentMethodRepository {
    async fn save(
        &self,
        method: &PaymentMethod,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, method)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state.name_taken(method) {
            return Err(RepositoryError::DuplicateName);
        }
        state.methods.push(method.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> Result<Option<PaymentMethod>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .methods
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        method: &PaymentMethod,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, method)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .methods
            .iter()
            .position(|stored| {
                stored.company_id() == method.company_id() && stored.id() == method.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if state.name_taken(method) {
            return Err(RepositoryError::DuplicateName);
        }
        state.methods[position] = method.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::PaymentMethod, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PaymentMethodQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<PaymentMethod>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .methods
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .cloned()
            .collect();
        Ok(keyset_page(
            matching,
            query.sort(),
            after,
            limit,
            |stored| name_key(stored.name(), stored.id().as_uuid()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::PaymentMethodService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn stores_payment_methods_with_their_trail() {
        let repository = InMemoryPaymentMethodRepository::default();
        contract::stores_payment_methods_with_their_trail(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn edits_payment_methods_and_records_only_changes() {
        let service = PaymentMethodService::new(InMemoryPaymentMethodRepository::default());
        contract::edits_payment_methods_and_records_only_changes(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn filters_and_sorts_payment_methods() {
        let service = PaymentMethodService::new(InMemoryPaymentMethodRepository::default());
        contract::filters_and_sorts_payment_methods(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }
}
