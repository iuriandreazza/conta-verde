use conta_verde_application::{PackageScheduleRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CalendarDate, CompanyId, PackageSchedule,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::postgres::storage_error;
use crate::postgres_audit;

#[derive(Clone)]
pub struct PostgresPackageScheduleRepository {
    pool: PgPool,
}

impl PostgresPackageScheduleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl PackageScheduleRepository for PostgresPackageScheduleRepository {
    async fn find(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<PackageSchedule>, RepositoryError> {
        let row = sqlx::query("SELECT enabled, day FROM package_schedules WHERE company_id = $1")
            .bind(company_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let enabled: bool = row.try_get("enabled").map_err(storage_error)?;
        let day: i16 = row.try_get("day").map_err(storage_error)?;
        let day = u8::try_from(day)
            .map_err(|_| RepositoryError::Storage(format!("stored package day {day}")))?;
        PackageSchedule::new(company_id, enabled, day)
            .map(Some)
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    async fn save(
        &self,
        schedule: &PackageSchedule,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, schedule)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO package_schedules (company_id, enabled, day) VALUES ($1, $2, $3) \
             ON CONFLICT (company_id) DO UPDATE SET enabled = EXCLUDED.enabled, day = EXCLUDED.day",
        )
        .bind(schedule.company_id().as_uuid())
        .bind(schedule.is_enabled())
        .bind(i16::from(schedule.day()))
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
            AuditEntity::PackageSchedule,
            company_id.as_uuid(),
        )
        .await
    }

    async fn due(&self, day_of_month: u8) -> Result<Vec<CompanyId>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT company_id FROM package_schedules \
             WHERE enabled AND day <= $1 ORDER BY company_id",
        )
        .bind(i16::from(day_of_month))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter()
            .map(|row| {
                let id: Uuid = row.try_get("company_id").map_err(storage_error)?;
                Ok(CompanyId::from_uuid(id))
            })
            .collect()
    }

    async fn claim(
        &self,
        company_id: CompanyId,
        month: CalendarDate,
        at_millis: i64,
    ) -> Result<bool, RepositoryError> {
        // One statement: the row is the claim, and whoever inserts it first has the month.
        let result = sqlx::query(
            "INSERT INTO package_sends (company_id, month, claimed_at) \
             VALUES ($1, CAST($2 AS DATE), $3) ON CONFLICT DO NOTHING",
        )
        .bind(company_id.as_uuid())
        .bind(month.to_string())
        .bind(at_millis)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(result.rows_affected() == 1)
    }

    async fn release(
        &self,
        company_id: CompanyId,
        month: CalendarDate,
    ) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM package_sends WHERE company_id = $1 AND month = CAST($2 AS DATE)")
            .bind(company_id.as_uuid())
            .bind(month.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::PackageScheduleService;

    use super::*;
    use crate::{InMemoryEmailSender, PostgresIdentityRepository, contract};

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_the_package_schedule() {
        let pool = contract::test_pool().await;
        contract::stores_the_package_schedule(
            &PostgresPackageScheduleRepository::new(pool.clone()),
            contract::create_company(&pool).await,
            contract::create_company(&pool).await,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn announces_the_monthly_package() {
        let pool = contract::test_pool().await;
        let identities = PostgresIdentityRepository::new(pool.clone());
        let mailbox = InMemoryEmailSender::default();
        let service = PackageScheduleService::new(
            PostgresPackageScheduleRepository::new(pool),
            identities.clone(),
            mailbox.clone(),
            "https://contas.example.com/",
        );
        contract::announces_the_monthly_package(&service, &identities, &mailbox).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn gives_the_month_back_when_nothing_could_be_queued() {
        let pool = contract::test_pool().await;
        let identities = PostgresIdentityRepository::new(pool.clone());
        let store = PostgresPackageScheduleRepository::new(pool);
        let service = PackageScheduleService::new(
            store.clone(),
            identities.clone(),
            InMemoryEmailSender::broken(),
            "https://contas.example.com",
        );
        contract::gives_the_month_back_when_nothing_could_be_queued(&service, &store, &identities)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_a_day_the_domain_would() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        for day in [0_i16, 29] {
            let refused = sqlx::query(
                "INSERT INTO package_schedules (company_id, enabled, day) VALUES ($1, true, $2)",
            )
            .bind(company.as_uuid())
            .bind(day)
            .execute(&pool)
            .await;
            assert!(refused.is_err(), "day {day}");
        }
        let not_first = sqlx::query(
            "INSERT INTO package_sends (company_id, month, claimed_at) VALUES ($1, DATE '2026-10-15', 1)",
        )
        .bind(company.as_uuid())
        .execute(&pool)
        .await;
        assert!(not_first.is_err(), "a month is claimed by its first day");
    }
}
