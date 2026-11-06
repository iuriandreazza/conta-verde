use std::sync::{Arc, Mutex};

use conta_verde_application::{
    NameCursor, PriceListQuery, PriceListRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{AuditEntity, AuditEntry, AuditRecord, CompanyId, PriceList, PriceListId};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

/// Keeps price lists and their audit trail in process memory. Used by tests and for running the
/// API without a database. Like the Postgres adapter, it records an audit entry only for a write
/// that happened: a refused or missed write leaves nothing in the trail, and the prices of a
/// list it could not change stay as they were.
#[derive(Default, Clone)]
pub struct InMemoryPriceListRepository {
    state: Arc<Mutex<PriceListState>>,
}

#[derive(Default)]
struct PriceListState {
    lists: Vec<PriceList>,
    audit: AuditLog,
}

impl PriceListState {
    /// Whether another price list of the company already has the name, ignoring case.
    fn name_taken(&self, list: &PriceList) -> bool {
        self.lists.iter().any(|other| {
            other.company_id() == list.company_id()
                && other.id() != list.id()
                && other.name().to_lowercase() == list.name().to_lowercase()
        })
    }
}

impl PriceListRepository for InMemoryPriceListRepository {
    async fn save(&self, list: &PriceList, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, list)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state.name_taken(list) {
            return Err(RepositoryError::DuplicateName);
        }
        state.lists.push(list.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PriceListId,
    ) -> Result<Option<PriceList>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .lists
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(&self, list: &PriceList, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, list)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .lists
            .iter()
            .position(|stored| stored.company_id() == list.company_id() && stored.id() == list.id())
            .ok_or(RepositoryError::NotFound)?;
        if state.name_taken(list) {
            return Err(RepositoryError::DuplicateName);
        }
        state.lists[position] = list.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PriceListId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::PriceList, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PriceListQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<PriceList>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .lists
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
    use conta_verde_application::PriceListService;

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        InMemoryPaymentTermRepository, InMemoryProductRepository, RegisterDocumentReferences,
        contract,
    };

    #[tokio::test]
    async fn runs_the_price_list_lifecycle() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let service = PriceListService::new(
            InMemoryPriceListRepository::default(),
            RegisterDocumentReferences::new(registers.parties.clone(), products, terms),
        );
        contract::runs_the_price_list_lifecycle(&service, company, other_company, &seed).await;
    }
}
