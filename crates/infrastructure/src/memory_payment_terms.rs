use std::sync::Mutex;

use conta_verde_application::{
    NameCursor, PaymentTermQuery, PaymentTermRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, PaymentTerm, PaymentTermId,
};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

/// Keeps payment terms and their audit trail in process memory. Used by tests and for running the
/// API without a database. Like the Postgres adapter, it records an audit entry only for a write
/// that happened: a refused or missed write leaves nothing in the trail, and the installments of a
/// term it could not change stay as they were.
#[derive(Default)]
pub struct InMemoryPaymentTermRepository {
    state: Mutex<PaymentTermState>,
}

#[derive(Default)]
struct PaymentTermState {
    terms: Vec<PaymentTerm>,
    audit: AuditLog,
}

impl PaymentTermState {
    /// Whether another payment term of the company already has the name, ignoring case.
    fn name_taken(&self, term: &PaymentTerm) -> bool {
        self.terms.iter().any(|other| {
            other.company_id() == term.company_id()
                && other.id() != term.id()
                && other.name().to_lowercase() == term.name().to_lowercase()
        })
    }
}

impl PaymentTermRepository for InMemoryPaymentTermRepository {
    async fn save(&self, term: &PaymentTerm, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, term)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state.name_taken(term) {
            return Err(RepositoryError::DuplicateName);
        }
        state.terms.push(term.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<Option<PaymentTerm>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .terms
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(&self, term: &PaymentTerm, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, term)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .terms
            .iter()
            .position(|stored| stored.company_id() == term.company_id() && stored.id() == term.id())
            .ok_or(RepositoryError::NotFound)?;
        if state.name_taken(term) {
            return Err(RepositoryError::DuplicateName);
        }
        state.terms[position] = term.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::PaymentTerm, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PaymentTermQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<PaymentTerm>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .terms
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
    use conta_verde_application::PaymentTermService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn stores_payment_terms_with_their_trail() {
        let repository = InMemoryPaymentTermRepository::default();
        contract::stores_payment_terms_with_their_trail(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn edits_payment_terms_and_records_only_changes() {
        let service = PaymentTermService::new(InMemoryPaymentTermRepository::default());
        contract::edits_payment_terms_and_records_only_changes(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn filters_and_sorts_payment_terms() {
        let service = PaymentTermService::new(InMemoryPaymentTermRepository::default());
        contract::filters_and_sorts_payment_terms(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }
}
