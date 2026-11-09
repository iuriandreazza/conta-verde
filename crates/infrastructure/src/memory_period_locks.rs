use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{PeriodLockRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{AuditEntity, AuditEntry, AuditRecord, CompanyId, PeriodLock};

use crate::memory_audit::AuditLog;

/// Keeps the closed periods and their audit trail in process memory, for tests and for running the API without a
/// database. Clones share the same records, which is how every service of a test sees the same lock.
#[derive(Default, Clone)]
pub struct InMemoryPeriodLockRepository {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    locks: HashMap<CompanyId, PeriodLock>,
    audit: AuditLog,
}

impl PeriodLockRepository for InMemoryPeriodLockRepository {
    async fn find(&self, company_id: CompanyId) -> Result<Option<PeriodLock>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.locks.get(&company_id).copied())
    }

    async fn save(&self, lock: &PeriodLock, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, lock)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.locks.insert(lock.company_id(), *lock);
        state.audit.record(audit);
        Ok(())
    }

    async fn history(&self, company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::PeriodLock, company_id.as_uuid()))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{
        BalanceService, FinancialEntryService, PeriodLockService, PurchaseService, SaleService,
        StockService, TransferService,
    };
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        InMemoryFinancialEntryRepository, InMemoryOpeningRepository, InMemoryPaymentTermRepository,
        InMemoryProductRepository, InMemoryPurchaseOrderRepository, InMemoryQuoteRepository,
        InMemorySaleRepository, InMemoryStockMovementRepository, InMemoryStockSettingsRepository,
        InMemoryTransferRepository, RegisterDocumentReferences, contract,
    };

    #[tokio::test]
    async fn runs_the_period_lock() {
        let locks = PeriodLockService::new(InMemoryPeriodLockRepository::default());
        contract::runs_the_period_lock(&locks, CompanyId::generate(), CompanyId::generate()).await;
    }

    #[tokio::test]
    async fn a_closed_month_refuses_what_the_books_hold() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let (seed, other_seed) = (
            registers.seed(company).await,
            registers.seed(other_company).await,
        );
        let (entry_store, transfer_store) = (
            InMemoryFinancialEntryRepository::default(),
            InMemoryTransferRepository::default(),
        );
        contract::a_closed_month_refuses_what_the_books_hold(
            &PeriodLockService::new(registers.period_locks.clone()),
            &FinancialEntryService::new(entry_store.clone(), registers.references()),
            &TransferService::new(transfer_store.clone(), registers.references()),
            &BalanceService::new(
                registers.bank_accounts.clone(),
                entry_store,
                transfer_store,
                InMemoryOpeningRepository::default(),
            )
            .with_period_lock(registers.period_locks.clone()),
            company,
            other_company,
            &seed,
            &other_seed,
        )
        .await;
    }

    #[tokio::test]
    async fn a_closed_month_refuses_stock_and_documents() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let company = CompanyId::generate();
        let entry_seed = registers.seed(company).await;
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let (entry_store, ledger) = (
            InMemoryFinancialEntryRepository::default(),
            InMemoryStockMovementRepository::default(),
        );
        let documents = || {
            RegisterDocumentReferences::new(
                registers.parties.clone(),
                products.clone(),
                terms.clone(),
            )
        };
        let stock_of = || {
            StockService::new(
                ledger.clone(),
                products.clone(),
                InMemoryStockSettingsRepository::default(),
            )
            .with_period_lock(registers.period_locks.clone())
        };
        let purchases = PurchaseService::new(
            InMemoryPurchaseOrderRepository::default(),
            FinancialEntryService::new(entry_store.clone(), registers.references()),
            registers.references(),
            documents(),
            stock_of(),
        );
        let sales = SaleService::new(
            InMemorySaleRepository::default(),
            InMemoryQuoteRepository::default(),
            FinancialEntryService::new(entry_store, registers.references()),
            registers.references(),
            documents(),
            stock_of(),
        );
        contract::a_closed_month_refuses_stock_and_documents(
            &PeriodLockService::new(registers.period_locks.clone()),
            &purchases,
            &sales,
            &stock_of(),
            company,
            &entry_seed,
            &seed,
        )
        .await;
    }
}
