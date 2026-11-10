use conta_verde_application::{RepositoryError, SaleClaim, SaleClaimRepository};
use conta_verde_domain::{CompanyId, SaleId};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;

pub struct PostgresSaleClaimRepository {
    pool: PgPool,
}

impl PostgresSaleClaimRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn claim_from_row(row: &PgRow) -> Result<SaleClaim, RepositoryError> {
    Ok(SaleClaim {
        sale_id: SaleId::from_uuid(row.try_get("sale_id").map_err(storage_error)?),
        request: row.try_get("request").map_err(storage_error)?,
    })
}

impl SaleClaimRepository for PostgresSaleClaimRepository {
    async fn claim(
        &self,
        company_id: CompanyId,
        reference: &str,
        sale_id: SaleId,
        request: &str,
    ) -> Result<SaleClaim, RepositoryError> {
        // One statement: the insert that loses to another returns nothing, and the claim that holds is read next. A
        // claim is never deleted, so what is read is the one that won.
        sqlx::query(
            "INSERT INTO sale_claims (company_id, reference, sale_id, request) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (company_id, reference) DO NOTHING",
        )
        .bind(company_id.as_uuid())
        .bind(reference)
        .bind(sale_id.as_uuid())
        .bind(request)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        self.find(company_id, reference).await?.ok_or_else(|| {
            RepositoryError::Storage("a claim that was just made cannot be read".to_owned())
        })
    }

    async fn reference_of(
        &self,
        company_id: CompanyId,
        sale_id: SaleId,
    ) -> Result<Option<String>, RepositoryError> {
        sqlx::query_scalar(
            "SELECT reference FROM sale_claims WHERE company_id = $1 AND sale_id = $2",
        )
        .bind(company_id.as_uuid())
        .bind(sale_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        reference: &str,
    ) -> Result<Option<SaleClaim>, RepositoryError> {
        let row = sqlx::query(
            "SELECT sale_id, request FROM sale_claims WHERE company_id = $1 AND reference = $2",
        )
        .bind(company_id.as_uuid())
        .bind(reference)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(claim_from_row).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn a_reference_is_claimed_once_per_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        crate::contract_integrations::claims_a_sale_reference_once_per_company(
            &PostgresSaleClaimRepository::new(pool),
            company,
            other_company,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_a_reference_that_is_blank_or_has_control_characters() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let repository = PostgresSaleClaimRepository::new(pool);
        for reference in ["", " padded ", "line\nbreak", &"x".repeat(101)] {
            assert!(
                repository
                    .claim(company, reference, SaleId::generate(), "{}")
                    .await
                    .is_err(),
                "{reference:?}"
            );
        }
    }
}
