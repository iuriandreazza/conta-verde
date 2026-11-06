use std::sync::{Arc, Mutex};

use conta_verde_application::{
    ContractQuery, ContractRepository, NameCursor, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CalendarDate, CompanyId, Contract, ContractId,
    ContractStatus,
};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

/// Keeps contracts and their audit trail in process memory. Used by tests and for running the API without a
/// database. Like the Postgres adapter, it records an audit entry only for a write that happened. Clones share
/// the same contracts.
#[derive(Default, Clone)]
pub struct InMemoryContractRepository {
    state: Arc<Mutex<ContractState>>,
}

#[derive(Default)]
struct ContractState {
    contracts: Vec<Contract>,
    audit: AuditLog,
}

impl ContractRepository for InMemoryContractRepository {
    async fn save(&self, contract: &Contract, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, contract)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.contracts.push(contract.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> Result<Option<Contract>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .contracts
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(&self, contract: &Contract, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, contract)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .contracts
            .iter_mut()
            .find(|stored| {
                stored.company_id() == contract.company_id() && stored.id() == contract.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() == ContractStatus::Terminated {
            return Err(RepositoryError::ContractChanged);
        }
        // What the job billed through in the meantime is not taken back.
        let through = stored.billed_through().max(contract.billed_through());
        *stored = contract.clone().restored(contract.status(), through);
        state.audit.record(audit);
        Ok(())
    }

    async fn mark_billed(
        &self,
        company_id: CompanyId,
        id: ContractId,
        through: CalendarDate,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        if let Some(stored) = state
            .contracts
            .iter_mut()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .filter(|stored| stored.billed_through().is_none_or(|done| done < through))
        {
            *stored = stored.clone().restored(stored.status(), Some(through));
        }
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Contract, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &ContractQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<Contract>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .contracts
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

    async fn list_to_bill(
        &self,
        until: CalendarDate,
        after: Option<ContractId>,
        limit: usize,
    ) -> Result<Vec<Contract>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut due: Vec<Contract> = state
            .contracts
            .iter()
            .filter(|stored| {
                let done = stored.billed_through();
                stored.status() != ContractStatus::Paused
                    && after.is_none_or(|last| stored.id().as_uuid() > last.as_uuid())
                    && stored.starts_on() <= until
                    && done.is_none_or(|day| day < until)
                    && stored
                        .ends_on()
                        .is_none_or(|end| done.is_none_or(|day| day < end))
            })
            .cloned()
            .collect();
        due.sort_by_key(|stored| stored.id().as_uuid());
        due.truncate(limit);
        Ok(due)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{ContractService, FinancialEntryService, SaleService};

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        InMemoryFinancialEntryRepository, InMemoryPaymentTermRepository, InMemoryProductRepository,
        InMemoryQuoteRepository, InMemorySaleRepository, RegisterDocumentReferences, contract,
    };

    #[tokio::test]
    async fn bills_contracts_every_cycle() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let entry_seed = registers.seed(company).await;
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let (entries, sale_store, quotes) = (
            InMemoryFinancialEntryRepository::default(),
            InMemorySaleRepository::default(),
            InMemoryQuoteRepository::default(),
        );
        let sales = || {
            SaleService::new(
                sale_store.clone(),
                quotes.clone(),
                FinancialEntryService::new(entries.clone(), registers.references()),
                registers.references(),
                RegisterDocumentReferences::new(
                    registers.parties.clone(),
                    products.clone(),
                    terms.clone(),
                ),
            )
        };
        let service = ContractService::new(InMemoryContractRepository::default(), sales());
        contract::bills_contracts_every_cycle(
            &service,
            &sales(),
            company,
            other_company,
            &entry_seed,
            &seed,
        )
        .await;
    }
}
