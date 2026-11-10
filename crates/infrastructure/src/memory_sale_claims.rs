use std::sync::{Arc, Mutex};

use conta_verde_application::{RepositoryError, SaleClaim, SaleClaimRepository};
use conta_verde_domain::{CompanyId, SaleId};

/// Keeps the claims on external references in process memory. Used by tests and for running the API without a database.
#[derive(Default, Clone)]
pub struct InMemorySaleClaimRepository {
    claims: Arc<Mutex<Vec<(CompanyId, String, SaleClaim)>>>,
}

impl SaleClaimRepository for InMemorySaleClaimRepository {
    async fn claim(
        &self,
        company_id: CompanyId,
        reference: &str,
        sale_id: SaleId,
        request: &str,
    ) -> Result<SaleClaim, RepositoryError> {
        let mut claims = self.claims.lock().expect("repository lock poisoned");
        if let Some((_, _, held)) = claims
            .iter()
            .find(|(company, stored, _)| *company == company_id && stored == reference)
        {
            return Ok(held.clone());
        }
        let claim = SaleClaim {
            sale_id,
            request: request.to_owned(),
        };
        claims.push((company_id, reference.to_owned(), claim.clone()));
        Ok(claim)
    }

    async fn reference_of(
        &self,
        company_id: CompanyId,
        sale_id: SaleId,
    ) -> Result<Option<String>, RepositoryError> {
        let claims = self.claims.lock().expect("repository lock poisoned");
        Ok(claims
            .iter()
            .find(|(company, _, claim)| *company == company_id && claim.sale_id == sale_id)
            .map(|(_, reference, _)| reference.clone()))
    }

    async fn find(
        &self,
        company_id: CompanyId,
        reference: &str,
    ) -> Result<Option<SaleClaim>, RepositoryError> {
        let claims = self.claims.lock().expect("repository lock poisoned");
        Ok(claims
            .iter()
            .find(|(company, stored, _)| *company == company_id && stored == reference)
            .map(|(_, _, claim)| claim.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_integrations;

    #[tokio::test]
    async fn a_reference_is_claimed_once_per_company() {
        contract_integrations::claims_a_sale_reference_once_per_company(
            &InMemorySaleClaimRepository::default(),
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }
}
