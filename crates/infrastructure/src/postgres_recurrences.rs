use conta_verde_application::{
    NameCursor, RecurrenceQuery, RecurrenceRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    Actor, AuditEntity, AuditEntry, AuditRecord, CalendarDate, CategoryId, CompanyId, CostCenterId,
    EntryKind, Frequency, PartyId, Recurrence, RecurrenceDraft, RecurrenceId, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{push_text_keyset, storage_error};
use crate::postgres_audit;

const SELECT_RECURRENCES: &str = "SELECT id, company_id, kind, description, party_id, category_id, \
     cost_center_id, amount::text AS amount, frequency, to_char(starts_on, 'YYYY-MM-DD') AS starts_on, \
     to_char(ends_on, 'YYYY-MM-DD') AS ends_on, active, \
     to_char(generated_through, 'YYYY-MM-DD') AS generated_through, saved_by_user_id, saved_by_name \
     FROM recurrences";

pub struct PostgresRecurrenceRepository {
    pool: PgPool,
}

impl PostgresRecurrenceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn saved_by(recurrence: &Recurrence) -> (Uuid, &str) {
    // The column holds a person's id and name; no key reaches it ([ADR 0087]).
    let by = recurrence.saved_by();
    (by.id(), by.name())
}

impl RecurrenceRepository for PostgresRecurrenceRepository {
    async fn save(
        &self,
        recurrence: &Recurrence,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, recurrence)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let (user_id, user_name) = saved_by(recurrence);
        sqlx::query(
            "INSERT INTO recurrences \
             (id, company_id, kind, description, party_id, category_id, cost_center_id, amount, \
              frequency, starts_on, ends_on, active, generated_through, saved_by_user_id, saved_by_name) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS NUMERIC), $9, CAST($10 AS DATE), \
                     CAST($11 AS DATE), $12, CAST($13 AS DATE), $14, $15)",
        )
        .bind(recurrence.id().as_uuid())
        .bind(recurrence.company_id().as_uuid())
        .bind(recurrence.kind().as_str())
        .bind(recurrence.description())
        .bind(recurrence.party_id().map(|id| id.as_uuid()))
        .bind(recurrence.category_id().as_uuid())
        .bind(recurrence.cost_center_id().map(|id| id.as_uuid()))
        .bind(recurrence.amount().to_string())
        .bind(recurrence.frequency().as_str())
        .bind(recurrence.starts_on().to_string())
        .bind(recurrence.ends_on().map(|day| day.to_string()))
        .bind(recurrence.is_active())
        .bind(recurrence.generated_through().map(|day| day.to_string()))
        .bind(user_id)
        .bind(user_name)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> Result<Option<Recurrence>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_RECURRENCES} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(recurrence_from_row).transpose()
    }

    async fn update(
        &self,
        recurrence: &Recurrence,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, recurrence)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let (user_id, user_name) = saved_by(recurrence);
        let result = sqlx::query(
            "UPDATE recurrences SET kind = $3, description = $4, party_id = $5, category_id = $6, \
             cost_center_id = $7, amount = CAST($8 AS NUMERIC), frequency = $9, \
             starts_on = CAST($10 AS DATE), ends_on = CAST($11 AS DATE), active = $12, \
             generated_through = GREATEST(generated_through, CAST($13 AS DATE)), \
             saved_by_user_id = $14, saved_by_name = $15 \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(recurrence.id().as_uuid())
        .bind(recurrence.company_id().as_uuid())
        .bind(recurrence.kind().as_str())
        .bind(recurrence.description())
        .bind(recurrence.party_id().map(|id| id.as_uuid()))
        .bind(recurrence.category_id().as_uuid())
        .bind(recurrence.cost_center_id().map(|id| id.as_uuid()))
        .bind(recurrence.amount().to_string())
        .bind(recurrence.frequency().as_str())
        .bind(recurrence.starts_on().to_string())
        .bind(recurrence.ends_on().map(|day| day.to_string()))
        .bind(recurrence.is_active())
        .bind(recurrence.generated_through().map(|day| day.to_string()))
        .bind(user_id)
        .bind(user_name)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn mark_generated(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
        through: CalendarDate,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE recurrences SET generated_through = CAST($3 AS DATE) \
             WHERE id = $1 AND company_id = $2 \
               AND (generated_through IS NULL OR generated_through < CAST($3 AS DATE))",
        )
        .bind(id.as_uuid())
        .bind(company_id.as_uuid())
        .bind(through.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::Recurrence,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &RecurrenceQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<Recurrence>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_RECURRENCES);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(kind) = query.kind() {
            sql.push(" AND kind = ").push_bind(kind.as_str());
        }
        if let Some(active) = query.active() {
            sql.push(" AND active = ").push_bind(active);
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
            sql.push(" AND strpos(lower(description), lower(")
                .push_bind(text)
                .push(")) > 0");
        }
        push_text_keyset(&mut sql, "description", query.sort(), after, limit);
        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(recurrence_from_row).collect()
    }

    async fn list_to_generate(
        &self,
        until: CalendarDate,
        after: Option<RecurrenceId>,
        limit: usize,
    ) -> Result<Vec<Recurrence>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_RECURRENCES} WHERE active AND starts_on <= CAST($1 AS DATE) \
             AND (generated_through IS NULL OR generated_through < CAST($1 AS DATE)) \
             AND (ends_on IS NULL OR generated_through IS NULL OR generated_through < ends_on) \
             AND ($2::uuid IS NULL OR id > $2) \
             ORDER BY id LIMIT $3"
        ))
        .bind(until.to_string())
        .bind(after.map(|id| id.as_uuid()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(recurrence_from_row).collect()
    }
}

fn recurrence_from_row(row: &PgRow) -> Result<Recurrence, RepositoryError> {
    let id = RecurrenceId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let kind = EntryKind::parse(&kind)
        .ok_or_else(|| RepositoryError::Storage(format!("stored recurrence with kind {kind:?}")))?;
    let frequency: String = row.try_get("frequency").map_err(storage_error)?;
    let frequency = Frequency::parse(&frequency).ok_or_else(|| {
        RepositoryError::Storage(format!("stored recurrence with frequency {frequency:?}"))
    })?;
    let party: Option<Uuid> = row.try_get("party_id").map_err(storage_error)?;
    let cost_center: Option<Uuid> = row.try_get("cost_center_id").map_err(storage_error)?;
    let draft = RecurrenceDraft {
        kind,
        description: row.try_get("description").map_err(storage_error)?,
        party_id: party.map(PartyId::from_uuid),
        category_id: CategoryId::from_uuid(row.try_get("category_id").map_err(storage_error)?),
        cost_center_id: cost_center.map(CostCenterId::from_uuid),
        amount: row.try_get("amount").map_err(storage_error)?,
        frequency,
        starts_on: row.try_get("starts_on").map_err(storage_error)?,
        ends_on: row.try_get("ends_on").map_err(storage_error)?,
        active: row.try_get("active").map_err(storage_error)?,
    };
    let saved_by = Actor::User {
        id: UserId::from_uuid(row.try_get("saved_by_user_id").map_err(storage_error)?),
        name: row.try_get("saved_by_name").map_err(storage_error)?,
    };
    let through: Option<String> = row.try_get("generated_through").map_err(storage_error)?;
    let recurrence = Recurrence::new(id, company_id, draft, saved_by).map_err(|error| {
        RepositoryError::Storage(format!("stored recurrence {id:?} is invalid: {error}"))
    })?;
    Ok(recurrence.with_generated_through(through.as_deref().and_then(CalendarDate::parse)))
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{FinancialEntryService, RecurrenceService};

    use super::*;
    use crate::contract;
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresPartyRepository, PostgresPaymentMethodRepository,
        RegisterReferences,
    };

    fn references(
        pool: &PgPool,
    ) -> RegisterReferences<
        PostgresCategoryRepository,
        PostgresCostCenterRepository,
        PostgresPartyRepository,
        PostgresBankAccountRepository,
        PostgresPaymentMethodRepository,
        crate::PostgresPeriodLockRepository,
    > {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
            crate::PostgresPeriodLockRepository::new(pool.clone()),
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn generates_recurring_entries_ahead() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await;
        let service = RecurrenceService::new(
            PostgresRecurrenceRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            ),
            references(&pool),
        );
        contract::generates_recurring_entries_ahead(
            &service,
            &PostgresRecurrenceRepository::new(pool.clone()),
            &PostgresFinancialEntryRepository::new(pool.clone()),
            &PostgresCategoryRepository::new(pool.clone()),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    async fn seed_for(pool: &PgPool, company: CompanyId) -> contract::EntrySeed {
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
    async fn the_database_refuses_what_the_domain_would_and_a_reference_to_another_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed_for(&pool, company).await;
        let other_seed = seed_for(&pool, other_company).await;
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let insert = |company: CompanyId,
                      category: Uuid,
                      amount: &'static str,
                      frequency: &'static str,
                      starts_on: &'static str,
                      ends_on: Option<&'static str>| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO recurrences (id, company_id, kind, description, category_id, amount, \
                     frequency, starts_on, ends_on, saved_by_user_id, saved_by_name) \
                     VALUES ($1, $2, 'payable', 'Aluguel', $3, CAST($4 AS NUMERIC), $5, \
                             CAST($6 AS DATE), CAST($7 AS DATE), $8, 'Ana')",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(category)
                .bind(amount)
                .bind(frequency)
                .bind(starts_on)
                .bind(ends_on)
                .bind(Uuid::new_v4())
                .execute(&pool)
                .await
            }
        };
        let category = seed.category.as_uuid();
        for (amount, frequency, starts_on, ends_on, constraint) in [
            (
                "0",
                "monthly",
                "2026-11-05",
                None,
                "recurrences_amount_valid",
            ),
            (
                "1.005",
                "monthly",
                "2026-11-05",
                None,
                "recurrences_amount_valid",
            ),
            (
                "10",
                "daily",
                "2026-11-05",
                None,
                "recurrences_frequency_valid",
            ),
            (
                "10",
                "monthly",
                "1899-12-31",
                None,
                "recurrences_starts_on_valid",
            ),
            (
                "10",
                "monthly",
                "2026-11-05",
                Some("2026-11-04"),
                "recurrences_ends_on_valid",
            ),
        ] {
            assert_eq!(
                refused_by(insert(company, category, amount, frequency, starts_on, ends_on).await)
                    .as_deref(),
                Some(constraint),
                "{amount} {frequency} {starts_on} {ends_on:?}"
            );
        }
        assert_eq!(
            refused_by(
                insert(
                    company,
                    other_seed.category.as_uuid(),
                    "10",
                    "monthly",
                    "2026-11-05",
                    None
                )
                .await
            )
            .as_deref(),
            Some("recurrences_category_fk")
        );
        assert!(
            insert(
                company,
                category,
                "10",
                "weekly",
                "2026-11-05",
                Some("2026-11-05")
            )
            .await
            .is_ok()
        );

        // An entry knows both where it came from and on which day, or neither.
        let lonely = sqlx::query(
            "INSERT INTO financial_entries (id, company_id, kind, description, category_id, competence_date, recurrence_id) \
             VALUES ($1, $2, 'payable', 'X', $3, DATE '2026-11-05', $4)",
        )
        .bind(Uuid::new_v4())
        .bind(company.as_uuid())
        .bind(category)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await;
        assert_eq!(
            refused_by(lonely).as_deref(),
            Some("financial_entries_origin_whole")
        );
    }
}
