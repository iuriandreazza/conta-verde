use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{
    DateCursor, RepositoryError, StockFloor, StockMovementQuery, StockMovementRepository,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntry, CalendarDate, CompanyId, Decimal, ProductId, StockBalance, StockMovement,
    StockMovementId, StockOriginKind,
};
use uuid::Uuid;

use crate::memory_audit::AuditLog;

/// Keeps the stock ledger and its audit trail in process memory. Used by tests and for running the API without a
/// database. Like the Postgres adapter, it checks what the products have under one lock and records an audit entry
/// only for a write that happened. Clones share the same ledger.
#[derive(Default, Clone)]
pub struct InMemoryStockMovementRepository {
    state: Arc<Mutex<StockState>>,
}

#[derive(Default)]
struct StockState {
    movements: Vec<StockMovement>,
    audit: AuditLog,
}

impl StockState {
    fn balance_of(
        &self,
        company_id: CompanyId,
        product_id: ProductId,
        on: Option<CalendarDate>,
    ) -> StockBalance {
        StockBalance::of(self.movements.iter().filter(|stored| {
            stored.company_id() == company_id
                && stored.product_id() == product_id
                && on.is_none_or(|day| stored.occurred_on() <= day)
        }))
    }
}

impl StockMovementRepository for InMemoryStockMovementRepository {
    async fn record(
        &self,
        company_id: CompanyId,
        movements: &[StockMovement],
        seen: &[(ProductId, StockBalance)],
        floor: StockFloor,
        audits: &[AuditEntry],
    ) -> Result<(), RepositoryError> {
        if movements.len() != audits.len() {
            return Err(RepositoryError::Storage(
                "each movement needs its audit entry".to_owned(),
            ));
        }
        for (movement, audit) in movements.iter().zip(audits) {
            ensure_audit_describes(audit, movement)?;
            if movement.company_id() != company_id {
                return Err(RepositoryError::NotFound);
            }
        }
        let mut state = self.state.lock().expect("repository lock poisoned");
        let mut books: HashMap<ProductId, StockBalance> = HashMap::new();
        for (product_id, expected) in seen {
            let stored = state.balance_of(company_id, *product_id, None);
            if stored != *expected {
                return Err(RepositoryError::StockChanged);
            }
            books.insert(*product_id, stored);
        }
        for (index, movement) in movements.iter().enumerate() {
            let earlier = &movements[..index];
            if movement.reverses().is_none()
                && movement.origin().is_some_and(|origin| {
                    state.movements.iter().chain(earlier).any(|stored| {
                        stored.company_id() == company_id
                            && stored.reverses().is_none()
                            && stored.origin() == Some(origin)
                    })
                })
            {
                return Err(RepositoryError::DuplicateOrigin);
            }
            if let Some(undone) = movement.reverses() {
                let missing = !state
                    .movements
                    .iter()
                    .any(|stored| stored.company_id() == company_id && stored.id() == undone);
                if missing {
                    return Err(RepositoryError::NotFound);
                }
                if state
                    .movements
                    .iter()
                    .chain(earlier)
                    .any(|stored| stored.reverses() == Some(undone))
                {
                    return Err(RepositoryError::DuplicateReversal);
                }
            }
            let balance = books.entry(movement.product_id()).or_default();
            *balance = balance.after(movement);
        }
        if floor == StockFloor::Zero {
            for (product_id, before) in seen {
                let after = books.get(product_id).copied().unwrap_or(*before);
                if after.quantity() < Decimal::ZERO && after.quantity() < before.quantity() {
                    return Err(RepositoryError::StockInsufficient);
                }
            }
        }
        state.movements.extend(movements.iter().cloned());
        for audit in audits {
            state.audit.record(audit);
        }
        Ok(())
    }

    async fn balances(
        &self,
        company_id: CompanyId,
        products: &[ProductId],
        on: Option<CalendarDate>,
    ) -> Result<Vec<(ProductId, StockBalance)>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(products
            .iter()
            .map(|product| (*product, state.balance_of(company_id, *product, on)))
            .collect())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: StockMovementId,
    ) -> Result<Option<StockMovement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .movements
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn of_origin(
        &self,
        company_id: CompanyId,
        kind: StockOriginKind,
        id: Uuid,
    ) -> Result<Vec<StockMovement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .movements
            .iter()
            .filter(|stored| {
                stored.company_id() == company_id
                    && stored
                        .origin()
                        .is_some_and(|origin| origin.kind == kind && origin.id == id)
            })
            .cloned()
            .collect())
    }

    async fn reversal_of(
        &self,
        company_id: CompanyId,
        id: StockMovementId,
    ) -> Result<Option<StockMovement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .movements
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.reverses() == Some(id))
            .cloned())
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &StockMovementQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> Result<Vec<StockMovement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let key = |movement: &StockMovement| (movement.occurred_on(), movement.id().as_uuid());
        let mut rows: Vec<StockMovement> = state
            .movements
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .filter(|stored| after.is_none_or(|cursor| key(stored) < (cursor.date, cursor.id)))
            .cloned()
            .collect();
        rows.sort_by_key(key);
        rows.reverse();
        rows.truncate(limit);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{
        FinancialEntryService, PurchaseService, SaleService, StockService,
    };

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        InMemoryFinancialEntryRepository, InMemoryPaymentTermRepository, InMemoryProductRepository,
        InMemoryPurchaseOrderRepository, InMemoryQuoteRepository, InMemorySaleRepository,
        RegisterDocumentReferences, contract,
    };

    #[tokio::test]
    async fn runs_the_stock_ledger() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let stock = StockService::new(InMemoryStockMovementRepository::default(), products);
        contract::runs_the_stock_ledger(&stock, company, other_company, &seed).await;
    }

    #[tokio::test]
    async fn stock_follows_purchases_and_sales() {
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
        let stock_of = || StockService::new(ledger.clone(), products.clone());
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
        contract::stock_follows_purchases_and_sales(
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
