use std::sync::Mutex;

use conta_verde_application::{
    NameCursor, ProductQuery, ProductRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{AuditEntity, AuditEntry, AuditRecord, CompanyId, Product, ProductId};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

/// Keeps products and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened: a refused or missed write leaves nothing in the trail.
#[derive(Default)]
pub struct InMemoryProductRepository {
    state: Mutex<ProductState>,
}

#[derive(Default)]
struct ProductState {
    products: Vec<Product>,
    audit: AuditLog,
}

impl ProductState {
    /// Whether another product of the company already uses the SKU.
    fn sku_taken(&self, product: &Product) -> bool {
        product.sku().is_some()
            && self.products.iter().any(|other| {
                other.company_id() == product.company_id()
                    && other.id() != product.id()
                    && other.sku() == product.sku()
            })
    }
}

impl ProductRepository for InMemoryProductRepository {
    async fn save(&self, product: &Product, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, product)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state.sku_taken(product) {
            return Err(RepositoryError::DuplicateSku);
        }
        state.products.push(product.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<Option<Product>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .products
            .iter()
            .find(|product| product.company_id() == company_id && product.id() == id)
            .cloned())
    }

    async fn update(&self, product: &Product, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, product)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .products
            .iter()
            .position(|stored| {
                stored.company_id() == product.company_id() && stored.id() == product.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if state.sku_taken(product) {
            return Err(RepositoryError::DuplicateSku);
        }
        state.products[position] = product.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Product, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &ProductQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<Product>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .products
            .iter()
            .filter(|product| product.company_id() == company_id && query.matches(product))
            .cloned()
            .collect();
        Ok(keyset_page(
            matching,
            query.sort(),
            after,
            limit,
            |product| name_key(product.name(), product.id().as_uuid()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::ProductService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn stores_products_with_their_trail() {
        let repository = InMemoryProductRepository::default();
        contract::stores_products_with_their_trail(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn edits_products_and_records_only_changes() {
        let service = ProductService::new(InMemoryProductRepository::default());
        contract::edits_products_and_records_only_changes(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn filters_and_sorts_products() {
        let service = ProductService::new(InMemoryProductRepository::default());
        contract::filters_and_sorts_products(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn breaks_ties_between_equal_product_names() {
        let service = ProductService::new(InMemoryProductRepository::default());
        contract::breaks_ties_between_equal_product_names(&service, CompanyId::generate()).await;
    }
}
