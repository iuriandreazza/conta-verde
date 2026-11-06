use std::sync::{Arc, Mutex};

use conta_verde_application::{
    NumberCursor, NumberSort, RepositoryError, SaleQuery, SaleRepository, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, ContractCycle, Decimal, QuoteId, Sale, SaleId,
    SaleReturn, SaleStatus,
};

use crate::memory_audit::AuditLog;

/// Keeps sales, their returns and their audit trail in process memory. Used by tests and for running the
/// API without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened. Clones share the same sales.
#[derive(Default, Clone)]
pub struct InMemorySaleRepository {
    state: Arc<Mutex<SaleState>>,
}

#[derive(Default)]
struct SaleState {
    sales: Vec<Sale>,
    returns: Vec<SaleReturn>,
    audit: AuditLog,
}

impl SaleRepository for InMemorySaleRepository {
    async fn save(&self, sale: &Sale, audit: &AuditEntry) -> Result<i64, RepositoryError> {
        ensure_audit_describes(audit, sale)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if sale.quote_id().is_some_and(|quote| {
            state
                .sales
                .iter()
                .any(|stored| stored.quote_id() == Some(quote))
        }) {
            return Err(RepositoryError::DuplicateQuoteSale);
        }
        if sale.cycle().is_some_and(|cycle| {
            state
                .sales
                .iter()
                .any(|stored| stored.cycle() == Some(cycle))
        }) {
            return Err(RepositoryError::DuplicateCycle);
        }
        let number = state
            .sales
            .iter()
            .filter(|stored| stored.company_id() == sale.company_id())
            .map(Sale::number)
            .max()
            .unwrap_or(0)
            + 1;
        state.sales.push(sale.clone().with_number(number));
        state.audit.record(audit);
        Ok(number)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: SaleId,
    ) -> Result<Option<Sale>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .sales
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn find_by_cycle(
        &self,
        company_id: CompanyId,
        cycle: ContractCycle,
    ) -> Result<Option<Sale>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .sales
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.cycle() == Some(cycle))
            .cloned())
    }

    async fn find_by_quote(
        &self,
        company_id: CompanyId,
        quote_id: QuoteId,
    ) -> Result<Option<Sale>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .sales
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.quote_id() == Some(quote_id))
            .cloned())
    }

    async fn update(
        &self,
        sale: &Sale,
        expected: SaleStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, sale)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .sales
            .iter_mut()
            .find(|stored| stored.company_id() == sale.company_id() && stored.id() == sale.id())
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != expected {
            return Err(RepositoryError::SaleChanged);
        }
        // The number is the store's, not the draft's.
        *stored = sale.clone().with_number(stored.number());
        state.audit.record(audit);
        Ok(())
    }

    async fn set_status(
        &self,
        sale: &Sale,
        expected: SaleStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, sale)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let returned = state
            .returns
            .iter()
            .any(|made| made.company_id() == sale.company_id() && made.sale_id() == sale.id());
        let stored = state
            .sales
            .iter_mut()
            .find(|stored| stored.company_id() == sale.company_id() && stored.id() == sale.id())
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != expected {
            return Err(RepositoryError::SaleChanged);
        }
        if sale.status() == SaleStatus::Cancelled && returned {
            return Err(RepositoryError::SaleHasReturns);
        }
        // Only the status moves: what an edit wrote meanwhile stays.
        *stored = stored.with_status(sale.status());
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: SaleId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Sale, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &SaleQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> Result<Vec<Sale>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let descending = query.sort() == NumberSort::NumberDesc;
        let key = |sale: &Sale| (sale.number(), sale.id().as_uuid());
        let mut rows: Vec<Sale> = state
            .sales
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

    async fn save_return(
        &self,
        sale_return: &SaleReturn,
        known_returns: usize,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, sale_return)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let sale = state
            .sales
            .iter()
            .find(|stored| {
                stored.company_id() == sale_return.company_id()
                    && stored.id() == sale_return.sale_id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if sale.status() != SaleStatus::Completed {
            return Err(RepositoryError::SaleChanged);
        }
        let stored_returns = state
            .returns
            .iter()
            .filter(|made| made.sale_id() == sale_return.sale_id())
            .count();
        if stored_returns != known_returns {
            return Err(RepositoryError::SaleChanged);
        }
        for (line, quantity) in sale_return.items() {
            let sold = usize::from(*line)
                .checked_sub(1)
                .and_then(|index| sale.lines().get(index))
                .map(|line| line.quantity().as_decimal())
                .ok_or(RepositoryError::ReturnTooMuch)?;
            let returned: Decimal = state
                .returns
                .iter()
                .filter(|made| made.sale_id() == sale_return.sale_id())
                .flat_map(|made| made.items().iter())
                .filter(|(number, _)| number == line)
                .map(|(_, quantity)| quantity.as_decimal())
                .sum();
            if returned + quantity.as_decimal() > sold {
                return Err(RepositoryError::ReturnTooMuch);
            }
        }
        state.returns.push(sale_return.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn returns_of_sales(
        &self,
        company_id: CompanyId,
        sale_ids: &[SaleId],
    ) -> Result<Vec<SaleReturn>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .returns
            .iter()
            .filter(|made| made.company_id() == company_id && sale_ids.contains(&made.sale_id()))
            .cloned()
            .collect())
    }

    async fn returns_of(
        &self,
        company_id: CompanyId,
        sale_id: SaleId,
    ) -> Result<Vec<SaleReturn>, RepositoryError> {
        self.returns_of_sales(company_id, &[sale_id]).await
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{FinancialEntryService, QuoteService, SaleService};

    use conta_verde_application::StockService;

    use super::*;
    use crate::InMemoryStockMovementRepository;
    use crate::memory_entries::tests::Registers;
    use crate::{
        Argon2SecretService, InMemoryEmailSender, InMemoryFinancialEntryRepository,
        InMemoryPaymentTermRepository, InMemoryProductRepository, InMemoryQuoteRepository,
        RegisterDocumentReferences, contract,
    };
    use crate::{InMemoryStockMovementRepository, InMemoryStockSettingsRepository};

    #[tokio::test]
    async fn voids_entries() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entries = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::voids_entries(&entries, company, other_company, &seed).await;
    }

    #[tokio::test]
    async fn runs_the_sale_lifecycle() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let entry_seed = registers.seed(company).await;
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let document_references = || {
            RegisterDocumentReferences::new(
                registers.parties.clone(),
                products.clone(),
                terms.clone(),
            )
        };
        let entry_store = InMemoryFinancialEntryRepository::default();
        let quote_store = InMemoryQuoteRepository::default();
        let store = InMemorySaleRepository::default();
        let sales = SaleService::new(
            store.clone(),
            quote_store.clone(),
            FinancialEntryService::new(entry_store.clone(), registers.references()),
            registers.references(),
            document_references(),
            StockService::new(
                InMemoryStockMovementRepository::default(),
                products.clone(),
                InMemoryStockSettingsRepository::default(),
            ),
        );
        let quotes = QuoteService::new(
            quote_store,
            document_references(),
            InMemoryEmailSender::default(),
            Argon2SecretService::new().unwrap(),
            "https://erp.example",
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        contract::runs_the_sale_lifecycle(
            &sales,
            &store,
            &quotes,
            &entries,
            company,
            other_company,
            &entry_seed,
            &seed,
        )
        .await;
    }
}
