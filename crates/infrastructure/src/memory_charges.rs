use std::sync::{Arc, Mutex};

use conta_verde_application::{ChargeRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, Charge, ChargeId, ChargeStatus, CompanyId, EntryId,
};

use crate::memory_audit::AuditLog;

/// Keeps charges and their audit trail in process memory, for tests and for running the API without a
/// database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryChargeRepository {
    state: Arc<Mutex<ChargeState>>,
}

#[derive(Default)]
struct ChargeState {
    /// In the order they were issued.
    charges: Vec<Charge>,
    audit: AuditLog,
}

impl ChargeRepository for InMemoryChargeRepository {
    async fn save(&self, charge: &Charge, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, charge)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state.charges.iter().any(|stored| {
            stored.company_id() == charge.company_id()
                && stored.entry_id() == charge.entry_id()
                && stored.installment() == charge.installment()
                && stored.status() == ChargeStatus::Open
        }) {
            return Err(RepositoryError::ChargeAlreadyOpen);
        }
        state.charges.push(charge.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn cancel(&self, cancelled: &Charge, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, cancelled)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .charges
            .iter_mut()
            .find(|stored| {
                stored.company_id() == cancelled.company_id()
                    && stored.entry_id() == cancelled.entry_id()
                    && stored.id() == cancelled.id()
                    && stored.status() == ChargeStatus::Open
            })
            .ok_or(RepositoryError::NotFound)?;
        *stored = stored.clone().with_status(ChargeStatus::Cancelled);
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: ChargeId,
    ) -> Result<Option<Charge>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .charges
            .iter()
            .find(|stored| {
                stored.company_id() == company_id
                    && stored.entry_id() == entry_id
                    && stored.id() == id
            })
            .cloned())
    }

    async fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Charge>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .charges
            .iter()
            .filter(|stored| stored.company_id() == company_id && stored.entry_id() == entry_id)
            .cloned()
            .collect())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ChargeId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Charge, id.as_uuid()))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{ChargeService, FinancialEntryService};

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{FakePaymentProvider, InMemoryFinancialEntryRepository, contract};

    #[tokio::test]
    async fn issues_and_cancels_charges() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let store = InMemoryChargeRepository::default();
        let provider = FakePaymentProvider::default();
        let service = ChargeService::new(
            store.clone(),
            entry_store.clone(),
            registers.parties.clone(),
            provider.clone(),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        contract::issues_and_cancels_charges(
            &service,
            &store,
            &entries,
            &provider,
            company,
            other_company,
            &seed,
        )
        .await;
    }
}
