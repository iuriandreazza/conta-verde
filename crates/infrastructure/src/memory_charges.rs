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

    async fn mark_paid(&self, paid: &Charge, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, paid)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .charges
            .iter_mut()
            .find(|stored| {
                stored.company_id() == paid.company_id()
                    && stored.id() == paid.id()
                    && stored.status() != ChargeStatus::Paid
            })
            .ok_or(RepositoryError::NotFound)?;
        *stored = stored
            .clone()
            .with_status(ChargeStatus::Paid)
            .with_receipt(paid.receipt().cloned());
        state.audit.record(audit);
        Ok(())
    }

    async fn expire(&self, expired: &Charge, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, expired)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .charges
            .iter_mut()
            .find(|stored| {
                stored.company_id() == expired.company_id()
                    && stored.id() == expired.id()
                    && stored.status() == ChargeStatus::Open
            })
            .ok_or(RepositoryError::NotFound)?;
        *stored = stored.clone().with_status(ChargeStatus::Expired);
        state.audit.record(audit);
        Ok(())
    }

    async fn find_by_id(
        &self,
        company_id: CompanyId,
        id: ChargeId,
    ) -> Result<Option<Charge>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .charges
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn find_by_external(
        &self,
        company_id: CompanyId,
        provider: &str,
        external_id: &str,
    ) -> Result<Option<Charge>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .charges
            .iter()
            .find(|stored| {
                stored.company_id() == company_id
                    && stored.provider() == provider
                    && stored.external_id() == external_id
            })
            .cloned())
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
    use conta_verde_application::{
        ChargeService, CollectionService, FinancialEntryService, PaymentPageService,
        PaymentWebhookService,
    };

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        Argon2SecretService, FakePaymentProvider, InMemoryCollectionSettingsRepository,
        InMemoryFinancialEntryRepository, InMemoryPaymentEventRepository,
        InMemoryPaymentLinkRepository, contract,
    };

    #[tokio::test]
    async fn issues_and_cancels_charges() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let store = InMemoryChargeRepository::default();
        let settings_store = InMemoryCollectionSettingsRepository::default();
        let provider = FakePaymentProvider::default();
        let service = ChargeService::new(
            store.clone(),
            entry_store.clone(),
            registers.parties.clone(),
            provider.clone(),
            settings_store.clone(),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        let settings = CollectionService::new(settings_store, registers.references());
        contract::issues_and_cancels_charges(
            &service,
            &store,
            &entries,
            &settings,
            &provider,
            &registers.parties,
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn configures_collection() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let settings = CollectionService::new(
            InMemoryCollectionSettingsRepository::default(),
            registers.references(),
        );
        contract::configures_collection(&settings, company, other_company, &seed).await;
    }

    #[tokio::test]
    async fn serves_payment_pages() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let store = InMemoryChargeRepository::default();
        let settings_store = InMemoryCollectionSettingsRepository::default();
        let links = InMemoryPaymentLinkRepository::default();
        let provider = FakePaymentProvider::default();
        let charges = ChargeService::new(
            store.clone(),
            entry_store.clone(),
            registers.parties.clone(),
            provider,
            settings_store.clone(),
        );
        let pages = PaymentPageService::new(
            links.clone(),
            store,
            entry_store.clone(),
            Argon2SecretService::new().unwrap(),
            "https://app.example/",
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        let settings = CollectionService::new(settings_store, registers.references());
        contract::serves_payment_pages(
            &pages,
            &links,
            &charges,
            &entries,
            &settings,
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn settles_charges_from_webhooks() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let store = InMemoryChargeRepository::default();
        let settings_store = InMemoryCollectionSettingsRepository::default();
        let events = InMemoryPaymentEventRepository::default();
        let provider = FakePaymentProvider::default();
        let charges = ChargeService::new(
            store.clone(),
            entry_store.clone(),
            registers.parties.clone(),
            provider.clone(),
            settings_store.clone(),
        );
        let webhooks = PaymentWebhookService::new(
            provider,
            store,
            events.clone(),
            settings_store.clone(),
            FinancialEntryService::new(entry_store.clone(), registers.references()),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        let settings = CollectionService::new(settings_store, registers.references());
        contract::settles_charges_from_webhooks(
            &webhooks,
            &charges,
            &entries,
            &settings,
            &conta_verde_application::PeriodLockService::new(registers.period_locks.clone()),
            &events,
            company,
            other_company,
            &seed,
        )
        .await;
    }
}
