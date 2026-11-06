use conta_verde_application::{
    CommissionRuleQuery, CommissionRuleRepository, RepositoryError, RuleCursor,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CommissionRule, CommissionRuleDraft, CommissionRuleId,
    CompanyId, PartyId, ProductId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::storage_error;
use crate::postgres_audit;

const SELECT_RULES: &str = "SELECT id, company_id, seller_id, product_id, rate::text AS rate, \
     to_char(starts_on, 'YYYY-MM-DD') AS starts_on, to_char(ends_on, 'YYYY-MM-DD') AS ends_on, active \
     FROM commission_rules";

pub struct PostgresCommissionRuleRepository {
    pool: PgPool,
}

impl PostgresCommissionRuleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl CommissionRuleRepository for PostgresCommissionRuleRepository {
    async fn save(&self, rule: &CommissionRule, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, rule)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO commission_rules (id, company_id, seller_id, product_id, rate, starts_on, ends_on, active) \
             VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC), CAST($6 AS DATE), CAST($7 AS DATE), $8)",
        )
        .bind(rule.id().as_uuid())
        .bind(rule.company_id().as_uuid())
        .bind(rule.seller_id().map(|id| id.as_uuid()))
        .bind(rule.product_id().map(|id| id.as_uuid()))
        .bind(rule.rate().to_string())
        .bind(rule.starts_on().to_string())
        .bind(rule.ends_on().map(|day| day.to_string()))
        .bind(rule.is_active())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> Result<Option<CommissionRule>, RepositoryError> {
        let row = sqlx::query(&format!("{SELECT_RULES} WHERE company_id = $1 AND id = $2"))
            .bind(company_id.as_uuid())
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        row.as_ref().map(rule_from_row).transpose()
    }

    async fn update(
        &self,
        rule: &CommissionRule,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, rule)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE commission_rules SET seller_id = $3, product_id = $4, rate = CAST($5 AS NUMERIC), \
             starts_on = CAST($6 AS DATE), ends_on = CAST($7 AS DATE), active = $8 \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(rule.id().as_uuid())
        .bind(rule.company_id().as_uuid())
        .bind(rule.seller_id().map(|id| id.as_uuid()))
        .bind(rule.product_id().map(|id| id.as_uuid()))
        .bind(rule.rate().to_string())
        .bind(rule.starts_on().to_string())
        .bind(rule.ends_on().map(|day| day.to_string()))
        .bind(rule.is_active())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::CommissionRule,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &CommissionRuleQuery,
        after: Option<&RuleCursor>,
        limit: usize,
    ) -> Result<Vec<CommissionRule>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_RULES);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(seller) = query.seller() {
            sql.push(" AND seller_id = ").push_bind(seller.as_uuid());
        }
        if let Some(product) = query.product() {
            sql.push(" AND product_id = ").push_bind(product.as_uuid());
        }
        if let Some(active) = query.active() {
            sql.push(" AND active = ").push_bind(active);
        }
        if let Some(cursor) = after {
            sql.push(" AND (starts_on, id) < (CAST(")
                .push_bind(cursor.starts_on.to_string())
                .push(" AS DATE), ")
                .push_bind(cursor.id)
                .push(")");
        }
        sql.push(" ORDER BY starts_on DESC, id DESC LIMIT ")
            .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(rule_from_row).collect()
    }
}

fn rule_from_row(row: &PgRow) -> Result<CommissionRule, RepositoryError> {
    let id = CommissionRuleId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let seller: Option<Uuid> = row.try_get("seller_id").map_err(storage_error)?;
    let product: Option<Uuid> = row.try_get("product_id").map_err(storage_error)?;
    let draft = CommissionRuleDraft {
        seller_id: seller.map(PartyId::from_uuid),
        product_id: product.map(ProductId::from_uuid),
        rate: row.try_get("rate").map_err(storage_error)?,
        starts_on: row.try_get("starts_on").map_err(storage_error)?,
        ends_on: row.try_get("ends_on").map_err(storage_error)?,
        active: row.try_get("active").map_err(storage_error)?,
    };
    CommissionRule::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored commission rule {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{CommissionService, FinancialEntryService, SaleService};

    use super::*;
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresPartyRepository, PostgresPaymentMethodRepository,
        PostgresPaymentTermRepository, PostgresProductRepository, PostgresQuoteRepository,
        PostgresSaleRepository, RegisterDocumentReferences, RegisterReferences, contract,
    };

    fn references(
        pool: &PgPool,
    ) -> RegisterReferences<
        PostgresCategoryRepository,
        PostgresCostCenterRepository,
        PostgresPartyRepository,
        PostgresBankAccountRepository,
        PostgresPaymentMethodRepository,
    > {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
        )
    }

    fn document_references(
        pool: &PgPool,
    ) -> RegisterDocumentReferences<
        PostgresPartyRepository,
        PostgresProductRepository,
        PostgresPaymentTermRepository,
    > {
        RegisterDocumentReferences::new(
            PostgresPartyRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresPaymentTermRepository::new(pool.clone()),
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn computes_commission_statements() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let entry_seed = contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await;
        let seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            company,
        )
        .await;
        let sales = SaleService::new(
            PostgresSaleRepository::new(pool.clone()),
            PostgresQuoteRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            ),
            references(&pool),
            document_references(&pool),
        );
        let commissions = CommissionService::new(
            PostgresCommissionRuleRepository::new(pool.clone()),
            PostgresSaleRepository::new(pool.clone()),
            document_references(&pool),
        );
        contract::computes_commission_statements(
            &commissions,
            &sales,
            company,
            other_company,
            &entry_seed,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_and_a_reference_to_another_company() {
        fn refused_by<T>(result: Result<T, sqlx::Error>) -> Option<String> {
            match result {
                Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
                _ => None,
            }
        }
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            company,
        )
        .await;
        let other_seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            other_company,
        )
        .await;
        let insert = |seller: Option<Uuid>,
                      product: Option<Uuid>,
                      rate: &'static str,
                      starts_on: &'static str,
                      ends_on: Option<&'static str>| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO commission_rules (id, company_id, seller_id, product_id, rate, starts_on, ends_on) \
                     VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC), CAST($6 AS DATE), CAST($7 AS DATE))",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(seller)
                .bind(product)
                .bind(rate)
                .bind(starts_on)
                .bind(ends_on)
                .execute(&pool)
                .await
            }
        };
        for (rate, starts_on, ends_on, constraint) in [
            ("0", "2026-01-01", None, "commission_rules_rate_valid"),
            ("100.01", "2026-01-01", None, "commission_rules_rate_valid"),
            ("5.005", "2026-01-01", None, "commission_rules_rate_valid"),
            ("5", "1899-12-31", None, "commission_rules_starts_on_valid"),
            (
                "5",
                "2026-02-01",
                Some("2026-01-31"),
                "commission_rules_ends_on_valid",
            ),
        ] {
            assert_eq!(
                refused_by(insert(None, None, rate, starts_on, ends_on).await).as_deref(),
                Some(constraint),
                "{rate} {starts_on} {ends_on:?}"
            );
        }
        assert_eq!(
            refused_by(
                insert(
                    Some(other_seed.seller.as_uuid()),
                    None,
                    "5",
                    "2026-01-01",
                    None
                )
                .await
            )
            .as_deref(),
            Some("commission_rules_seller_fk")
        );
        assert_eq!(
            refused_by(
                insert(
                    None,
                    Some(other_seed.product.as_uuid()),
                    "5",
                    "2026-01-01",
                    None
                )
                .await
            )
            .as_deref(),
            Some("commission_rules_product_fk")
        );
        assert!(
            insert(
                Some(seed.seller.as_uuid()),
                Some(seed.product.as_uuid()),
                "100",
                "2026-01-01",
                None
            )
            .await
            .is_ok()
        );
    }
}
