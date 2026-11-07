use std::sync::{Arc, Mutex};

use conta_verde_application::{
    NumberCursor, NumberSort, PurchaseOrderQuery, PurchaseOrderRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AccessKey, AuditEntity, AuditEntry, AuditRecord, CompanyId, Decimal, PurchaseOrder,
    PurchaseOrderId, PurchaseReceipt, PurchaseReceiptId, PurchaseStatus,
};

use crate::memory_audit::AuditLog;

/// Keeps purchase orders, their receipts and their audit trail in process memory. Used by tests and for
/// running the API without a database. Like the Postgres adapter, it records an audit entry only for a write
/// that happened. Clones share the same orders.
#[derive(Default, Clone)]
pub struct InMemoryPurchaseOrderRepository {
    state: Arc<Mutex<PurchaseState>>,
}

#[derive(Default)]
struct PurchaseState {
    orders: Vec<PurchaseOrder>,
    receipts: Vec<PurchaseReceipt>,
    audit: AuditLog,
}

impl PurchaseOrderRepository for InMemoryPurchaseOrderRepository {
    async fn save(
        &self,
        order: &PurchaseOrder,
        audit: &AuditEntry,
    ) -> Result<i64, RepositoryError> {
        ensure_audit_describes(audit, order)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if order.invoice_key().is_some_and(|key| {
            state.orders.iter().any(|stored| {
                stored.company_id() == order.company_id()
                    && stored.invoice_key() == Some(key)
                    && stored.status() != PurchaseStatus::Cancelled
            })
        }) {
            return Err(RepositoryError::DuplicateInvoice);
        }
        let number = state
            .orders
            .iter()
            .filter(|stored| stored.company_id() == order.company_id())
            .map(PurchaseOrder::number)
            .max()
            .unwrap_or(0)
            + 1;
        state.orders.push(order.clone().with_number(number));
        state.audit.record(audit);
        Ok(number)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> Result<Option<PurchaseOrder>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .orders
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn find_by_invoice_key(
        &self,
        company_id: CompanyId,
        key: &AccessKey,
    ) -> Result<Option<PurchaseOrder>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .orders
            .iter()
            .find(|stored| {
                stored.company_id() == company_id
                    && stored.invoice_key() == Some(key)
                    && stored.status() != PurchaseStatus::Cancelled
            })
            .cloned())
    }

    async fn update(
        &self,
        order: &PurchaseOrder,
        expected: PurchaseStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, order)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .orders
            .iter_mut()
            .find(|stored| stored.company_id() == order.company_id() && stored.id() == order.id())
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != expected {
            return Err(RepositoryError::PurchaseChanged);
        }
        // The number is the store's, not the draft's.
        *stored = order.clone().with_number(stored.number());
        state.audit.record(audit);
        Ok(())
    }

    async fn set_status(
        &self,
        order: &PurchaseOrder,
        expected: PurchaseStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, order)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .orders
            .iter_mut()
            .find(|stored| stored.company_id() == order.company_id() && stored.id() == order.id())
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != expected {
            return Err(RepositoryError::PurchaseChanged);
        }
        // Only the status moves: what an edit wrote meanwhile stays.
        *stored = stored.with_status(order.status());
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::PurchaseOrder, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PurchaseOrderQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> Result<Vec<PurchaseOrder>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let descending = query.sort() == NumberSort::NumberDesc;
        let key = |order: &PurchaseOrder| (order.number(), order.id().as_uuid());
        let mut rows: Vec<PurchaseOrder> = state
            .orders
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

    async fn count_by_status(
        &self,
        company_id: CompanyId,
        query: &PurchaseOrderQuery,
    ) -> Result<Vec<(PurchaseStatus, u64)>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let query = query.clone().any_status();
        let mut counts: Vec<(PurchaseStatus, u64)> = Vec::new();
        for order in state
            .orders
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
        {
            match counts
                .iter_mut()
                .find(|(status, _)| *status == order.status())
            {
                Some((_, count)) => *count += 1,
                None => counts.push((order.status(), 1)),
            }
        }
        Ok(counts)
    }

    async fn save_receipt(
        &self,
        receipt: &PurchaseReceipt,
        order_after: &PurchaseOrder,
        known_receipts: usize,
        receipt_audit: &AuditEntry,
        order_audit: Option<&AuditEntry>,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(receipt_audit, receipt)?;
        if let Some(audit) = order_audit {
            ensure_audit_describes(audit, order_after)?;
        }
        let mut state = self.state.lock().expect("repository lock poisoned");
        let order = state
            .orders
            .iter()
            .find(|stored| {
                stored.company_id() == receipt.company_id() && stored.id() == receipt.purchase_id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if !matches!(
            order.status(),
            PurchaseStatus::Approved | PurchaseStatus::PartiallyReceived
        ) {
            return Err(RepositoryError::PurchaseChanged);
        }
        let stored_receipts = state
            .receipts
            .iter()
            .filter(|made| made.purchase_id() == receipt.purchase_id())
            .count();
        if stored_receipts != known_receipts {
            return Err(RepositoryError::PurchaseChanged);
        }
        for item in receipt.items() {
            let ordered = usize::from(item.line)
                .checked_sub(1)
                .and_then(|index| order.lines().get(index))
                .map(|line| line.quantity().as_decimal())
                .ok_or(RepositoryError::ReceiptTooMuch)?;
            let received: Decimal = state
                .receipts
                .iter()
                .filter(|made| made.purchase_id() == receipt.purchase_id())
                .flat_map(|made| made.items().iter())
                .filter(|other| other.line == item.line)
                .map(|other| other.quantity.as_decimal())
                .sum();
            if received + item.quantity.as_decimal() > ordered {
                return Err(RepositoryError::ReceiptTooMuch);
            }
        }
        state.receipts.push(receipt.clone());
        state.audit.record(receipt_audit);
        if let Some(audit) = order_audit {
            let stored = state
                .orders
                .iter_mut()
                .find(|stored| {
                    stored.company_id() == order_after.company_id()
                        && stored.id() == order_after.id()
                })
                .ok_or(RepositoryError::NotFound)?;
            *stored = stored.with_status(order_after.status());
            state.audit.record(audit);
        }
        Ok(())
    }

    async fn receipts_of(
        &self,
        company_id: CompanyId,
        order_id: PurchaseOrderId,
    ) -> Result<Vec<PurchaseReceipt>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .receipts
            .iter()
            .filter(|made| made.company_id() == company_id && made.purchase_id() == order_id)
            .cloned()
            .collect())
    }

    async fn find_receipt(
        &self,
        company_id: CompanyId,
        order_id: PurchaseOrderId,
        id: PurchaseReceiptId,
    ) -> Result<Option<PurchaseReceipt>, RepositoryError> {
        Ok(self
            .receipts_of(company_id, order_id)
            .await?
            .into_iter()
            .find(|made| made.id() == id))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{FinancialEntryService, PurchaseService};

    use conta_verde_application::StockService;

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        InMemoryFinancialEntryRepository, InMemoryPaymentTermRepository, InMemoryProductRepository,
        RegisterDocumentReferences, contract,
    };
    use crate::{InMemoryStockMovementRepository, InMemoryStockSettingsRepository};

    #[tokio::test]
    async fn runs_the_purchase_lifecycle() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let entry_seed = registers.seed(company).await;
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let store = InMemoryPurchaseOrderRepository::default();
        let purchases = PurchaseService::new(
            store.clone(),
            FinancialEntryService::new(entry_store.clone(), registers.references()),
            registers.references(),
            RegisterDocumentReferences::new(
                registers.parties.clone(),
                products.clone(),
                terms.clone(),
            ),
            StockService::new(
                InMemoryStockMovementRepository::default(),
                products.clone(),
                InMemoryStockSettingsRepository::default(),
            ),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        contract::runs_the_purchase_lifecycle(
            &purchases,
            &store,
            &entries,
            company,
            other_company,
            &entry_seed,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn enters_a_supplier_invoice_once() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let company = CompanyId::generate();
        let entry_seed = registers.seed(company).await;
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let (entry_store, ledger, settings) = (
            InMemoryFinancialEntryRepository::default(),
            InMemoryStockMovementRepository::default(),
            InMemoryStockSettingsRepository::default(),
        );
        let stock_of = || StockService::new(ledger.clone(), products.clone(), settings.clone());
        let purchases = PurchaseService::new(
            InMemoryPurchaseOrderRepository::default(),
            FinancialEntryService::new(entry_store.clone(), registers.references()),
            registers.references(),
            RegisterDocumentReferences::new(
                registers.parties.clone(),
                products.clone(),
                terms.clone(),
            ),
            stock_of(),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        contract::enters_a_supplier_invoice_once(
            &purchases,
            &stock_of(),
            &entries,
            company,
            &entry_seed,
            &seed,
        )
        .await;
    }
}
