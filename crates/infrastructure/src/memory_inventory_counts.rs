use std::sync::{Arc, Mutex};

use conta_verde_application::{
    InventoryCountQuery, InventoryCountRepository, NumberCursor, NumberSort, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, CountStatus, InventoryCount, InventoryCountId,
};

use crate::memory_audit::AuditLog;

/// Keeps inventory counts and their audit trail in process memory. Used by tests and for running the API without a
/// database. Like the Postgres adapter, it records an audit entry only for a write that happened. Clones share the
/// same counts.
#[derive(Default, Clone)]
pub struct InMemoryInventoryCountRepository {
    state: Arc<Mutex<CountState>>,
}

#[derive(Default)]
struct CountState {
    counts: Vec<InventoryCount>,
    audit: AuditLog,
}

impl InventoryCountRepository for InMemoryInventoryCountRepository {
    async fn save(
        &self,
        count: &InventoryCount,
        audit: &AuditEntry,
    ) -> Result<i64, RepositoryError> {
        ensure_audit_describes(audit, count)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let number = state
            .counts
            .iter()
            .filter(|stored| stored.company_id() == count.company_id())
            .map(InventoryCount::number)
            .max()
            .unwrap_or(0)
            + 1;
        state.counts.push(count.clone().with_number(number));
        state.audit.record(audit);
        Ok(number)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> Result<Option<InventoryCount>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .counts
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        count: &InventoryCount,
        expected: CountStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, count)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .counts
            .iter_mut()
            .find(|stored| stored.company_id() == count.company_id() && stored.id() == count.id())
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != expected {
            return Err(RepositoryError::CountChanged);
        }
        // The number is the store's, not the draft's.
        *stored = count.clone().with_number(stored.number());
        state.audit.record(audit);
        Ok(())
    }

    async fn set_status(
        &self,
        count: &InventoryCount,
        expected: CountStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, count)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .counts
            .iter_mut()
            .find(|stored| stored.company_id() == count.company_id() && stored.id() == count.id())
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != expected {
            return Err(RepositoryError::CountChanged);
        }
        // Only the status moves: what an edit wrote meanwhile stays.
        *stored = stored.with_status(count.status());
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::InventoryCount, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &InventoryCountQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> Result<Vec<InventoryCount>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let descending = query.sort() == NumberSort::NumberDesc;
        let key = |count: &InventoryCount| (count.number(), count.id().as_uuid());
        let mut rows: Vec<InventoryCount> = state
            .counts
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .filter(|stored| {
                after.is_none_or(|cursor| {
                    let position = (cursor.number, cursor.id);
                    if descending {
                        key(stored) < position
                    } else {
                        key(stored) > position
                    }
                })
            })
            .cloned()
            .collect();
        rows.sort_by_key(key);
        if descending {
            rows.reverse();
        }
        rows.truncate(limit);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{InventoryCountService, StockService};

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        InMemoryPaymentTermRepository, InMemoryProductRepository, InMemoryStockMovementRepository,
        InMemoryStockSettingsRepository, contract,
    };

    #[tokio::test]
    async fn runs_the_inventory_count() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let (ledger, settings) = (
            InMemoryStockMovementRepository::default(),
            InMemoryStockSettingsRepository::default(),
        );
        let stock_of = || StockService::new(ledger.clone(), products.clone(), settings.clone());
        let counts =
            InventoryCountService::new(InMemoryInventoryCountRepository::default(), stock_of());
        contract::runs_the_inventory_count(&counts, &stock_of(), company, other_company, &seed)
            .await;
    }
}
