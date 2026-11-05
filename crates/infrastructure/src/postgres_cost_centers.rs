use conta_verde_application::{
    CostCenterQuery, CostCenterRepository, NameCursor, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, CostCenter, CostCenterDraft, CostCenterId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use crate::postgres::{push_name_keyset, storage_error};
use crate::postgres_audit;

const NAME_UNIQUE_INDEX: &str = "cost_centers_company_name_key";

const SELECT_COST_CENTERS: &str = "SELECT id, company_id, name, active FROM cost_centers";

pub struct PostgresCostCenterRepository {
    pool: PgPool,
}

impl PostgresCostCenterRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn write_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(db) if db.constraint() == Some(NAME_UNIQUE_INDEX) => {
            RepositoryError::DuplicateName
        }
        _ => storage_error(error),
    }
}

impl CostCenterRepository for PostgresCostCenterRepository {
    async fn save(
        &self,
        cost_center: &CostCenter,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, cost_center)?;
        // One transaction: if the cost center is refused (a duplicate name) or the audit row
        // cannot be written, the rollback leaves neither behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO cost_centers (id, company_id, name, active) VALUES ($1, $2, $3, $4)",
        )
        .bind(cost_center.id().as_uuid())
        .bind(cost_center.company_id().as_uuid())
        .bind(cost_center.name())
        .bind(cost_center.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> Result<Option<CostCenter>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_COST_CENTERS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(cost_center_from_row).transpose()
    }

    async fn update(
        &self,
        cost_center: &CostCenter,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, cost_center)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE cost_centers SET name = $3, active = $4 WHERE id = $1 AND company_id = $2",
        )
        .bind(cost_center.id().as_uuid())
        .bind(cost_center.company_id().as_uuid())
        .bind(cost_center.name())
        .bind(cost_center.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        // Matching no row means the cost center is not in that company: report it, do not
        // pretend. The transaction is dropped, so no audit row is left for a write that did not
        // happen.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::CostCenter,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &CostCenterQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<CostCenter>, RepositoryError> {
        // Every value is bound, never spliced into the text: the pieces pushed as text are fixed.
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_COST_CENTERS);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(active) = query.active() {
            sql.push(" AND active = ").push_bind(active);
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
            sql.push(" AND strpos(lower(name), lower(")
                .push_bind(text)
                .push(")) > 0");
        }
        push_name_keyset(&mut sql, query.sort(), after, limit);

        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(cost_center_from_row).collect()
    }
}

fn cost_center_from_row(row: &PgRow) -> Result<CostCenter, RepositoryError> {
    let id = CostCenterId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let draft = CostCenterDraft {
        name: row.try_get("name").map_err(storage_error)?,
        active: row.try_get("active").map_err(storage_error)?,
    };
    CostCenter::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored cost center {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::CostCenterService;

    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_cost_centers_with_their_trail() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresCostCenterRepository::new(pool);
        contract::stores_cost_centers_with_their_trail(&repository, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn edits_cost_centers_and_records_only_changes() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = CostCenterService::new(PostgresCostCenterRepository::new(pool));
        contract::edits_cost_centers_and_records_only_changes(&service, company, other_company)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn filters_and_sorts_cost_centers() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = CostCenterService::new(PostgresCostCenterRepository::new(pool));
        contract::filters_and_sorts_cost_centers(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_a_name_that_is_empty_or_too_long() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        for name in [String::new(), "x".repeat(201)] {
            let result = sqlx::query(
                "INSERT INTO cost_centers (id, company_id, name, active) VALUES ($1, $2, $3, true)",
            )
            .bind(uuid::Uuid::new_v4())
            .bind(company.as_uuid())
            .bind(&name)
            .execute(&pool)
            .await;
            match result {
                Err(sqlx::Error::Database(db)) => {
                    assert_eq!(db.constraint(), Some("cost_centers_name_is_short"));
                }
                other => panic!("expected the name CHECK to refuse it, got {other:?}"),
            }
        }
    }
}
