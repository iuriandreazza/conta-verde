use conta_verde_application::{PeriodLockRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CalendarDate, CompanyId, PeriodLock,
};
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

#[derive(Clone)]
pub struct PostgresPeriodLockRepository {
    pool: PgPool,
}

impl PostgresPeriodLockRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl PeriodLockRepository for PostgresPeriodLockRepository {
    async fn find(&self, company_id: CompanyId) -> Result<Option<PeriodLock>, RepositoryError> {
        let row = sqlx::query(
            "SELECT to_char(locked_through, 'YYYY-MM-DD') AS locked_through \
             FROM period_locks WHERE company_id = $1",
        )
        .bind(company_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let through: Option<String> = row.try_get("locked_through").map_err(storage_error)?;
        let through = through
            .map(|raw| {
                CalendarDate::parse(&raw)
                    .ok_or_else(|| RepositoryError::Storage(format!("bad locked_through {raw}")))
            })
            .transpose()?;
        PeriodLock::through(company_id, through)
            .map(Some)
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    async fn save(&self, lock: &PeriodLock, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, lock)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO period_locks (company_id, locked_through) VALUES ($1, CAST($2 AS DATE)) \
             ON CONFLICT (company_id) DO UPDATE SET locked_through = EXCLUDED.locked_through",
        )
        .bind(lock.company_id().as_uuid())
        .bind(lock.locked_through().map(|day| day.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(&self, company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::PeriodLock,
            company_id.as_uuid(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{
        BalanceService, FinancialEntryService, PeriodLockService, PurchaseService, SaleService,
        StockService, TransferService,
    };
    use sqlx::PgPool;

    use super::*;
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresOpeningRepository, PostgresPartyRepository,
        PostgresPaymentMethodRepository, PostgresPaymentTermRepository, PostgresProductRepository,
        PostgresPurchaseOrderRepository, PostgresQuoteRepository, PostgresSaleRepository,
        PostgresStockMovementRepository, PostgresStockSettingsRepository,
        PostgresTransferRepository, RegisterDocumentReferences, RegisterReferences, contract,
    };

    fn references(
        pool: &PgPool,
    ) -> RegisterReferences<
        PostgresCategoryRepository,
        PostgresCostCenterRepository,
        PostgresPartyRepository,
        PostgresBankAccountRepository,
        PostgresPaymentMethodRepository,
        PostgresPeriodLockRepository,
    > {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
            PostgresPeriodLockRepository::new(pool.clone()),
        )
    }

    async fn seed(pool: &PgPool, company: CompanyId) -> contract::EntrySeed {
        contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_period_lock() {
        let pool = contract::test_pool().await;
        let locks = PeriodLockService::new(PostgresPeriodLockRepository::new(pool.clone()));
        contract::runs_the_period_lock(
            &locks,
            contract::create_company(&pool).await,
            contract::create_company(&pool).await,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_a_lock_that_is_not_the_end_of_a_month() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let refused = sqlx::query(
            "INSERT INTO period_locks (company_id, locked_through) VALUES ($1, DATE '2026-10-30')",
        )
        .bind(company.as_uuid())
        .execute(&pool)
        .await;
        assert!(refused.is_err());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn a_closed_month_refuses_what_the_books_hold() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let (seed, other_seed) = (seed(&pool, company).await, seed(&pool, other_company).await);
        contract::a_closed_month_refuses_what_the_books_hold(
            &PeriodLockService::new(PostgresPeriodLockRepository::new(pool.clone())),
            &FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            ),
            &TransferService::new(
                PostgresTransferRepository::new(pool.clone()),
                references(&pool),
            ),
            &BalanceService::new(
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresFinancialEntryRepository::new(pool.clone()),
                PostgresTransferRepository::new(pool.clone()),
                PostgresOpeningRepository::new(pool.clone()),
            )
            .with_period_lock(PostgresPeriodLockRepository::new(pool.clone())),
            company,
            other_company,
            &seed,
            &other_seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn a_closed_month_refuses_stock_and_documents() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let entry_seed = seed(&pool, company).await;
        let seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            company,
        )
        .await;
        let documents = || {
            RegisterDocumentReferences::new(
                PostgresPartyRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresPaymentTermRepository::new(pool.clone()),
            )
        };
        let stock_of = || {
            StockService::new(
                PostgresStockMovementRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresStockSettingsRepository::new(pool.clone()),
            )
            .with_period_lock(PostgresPeriodLockRepository::new(pool.clone()))
        };
        let entries = || {
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            )
        };
        let purchases = PurchaseService::new(
            PostgresPurchaseOrderRepository::new(pool.clone()),
            entries(),
            references(&pool),
            documents(),
            stock_of(),
        );
        let sales = SaleService::new(
            PostgresSaleRepository::new(pool.clone()),
            PostgresQuoteRepository::new(pool.clone()),
            entries(),
            references(&pool),
            documents(),
            stock_of(),
        );
        contract::a_closed_month_refuses_stock_and_documents(
            &PeriodLockService::new(PostgresPeriodLockRepository::new(pool.clone())),
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
