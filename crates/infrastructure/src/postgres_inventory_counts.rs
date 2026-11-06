use std::collections::HashMap;

use conta_verde_application::{
    InventoryCountQuery, InventoryCountRepository, NumberCursor, NumberSort, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, CountLineDraft, CountStatus, InventoryCount,
    InventoryCountDraft, InventoryCountId, ProductId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{next_document_number, snapshot, storage_error};
use crate::postgres_audit;

const SELECT_COUNTS: &str = "SELECT id, company_id, number, to_char(counted_on, 'YYYY-MM-DD') AS counted_on, \
     notes, status FROM inventory_counts";

pub struct PostgresInventoryCountRepository {
    pool: PgPool,
}

impl PostgresInventoryCountRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Writes the lines of a count, in order, on the connection of the transaction that writes the count.
async fn insert_lines(
    connection: &mut PgConnection,
    count: &InventoryCount,
) -> Result<(), RepositoryError> {
    for (index, line) in count.lines().iter().enumerate() {
        sqlx::query(
            "INSERT INTO inventory_count_items (company_id, count_id, position, product_id, counted) \
             VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC))",
        )
        .bind(count.company_id().as_uuid())
        .bind(count.id().as_uuid())
        .bind(
            i16::try_from(index + 1)
                .map_err(|_| RepositoryError::Storage("a count has too many lines".to_owned()))?,
        )
        .bind(line.product_id().as_uuid())
        .bind(line.counted().to_string())
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
    }
    Ok(())
}

/// The status of the count, locked until the transaction ends.
async fn lock_status(
    connection: &mut PgConnection,
    company_id: CompanyId,
    id: InventoryCountId,
) -> Result<String, RepositoryError> {
    sqlx::query_scalar(
        "SELECT status FROM inventory_counts WHERE id = $1 AND company_id = $2 FOR UPDATE",
    )
    .bind(id.as_uuid())
    .bind(company_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?
    .ok_or(RepositoryError::NotFound)
}

impl InventoryCountRepository for PostgresInventoryCountRepository {
    async fn save(
        &self,
        count: &InventoryCount,
        audit: &AuditEntry,
    ) -> Result<i64, RepositoryError> {
        ensure_audit_describes(audit, count)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let number = next_document_number(&mut tx, count.company_id(), "inventory_count").await?;
        sqlx::query(
            "INSERT INTO inventory_counts (id, company_id, number, counted_on, notes, status) \
             VALUES ($1, $2, $3, CAST($4 AS DATE), $5, $6)",
        )
        .bind(count.id().as_uuid())
        .bind(count.company_id().as_uuid())
        .bind(number)
        .bind(count.counted_on().to_string())
        .bind(count.notes())
        .bind(count.status().as_str())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        insert_lines(&mut tx, count).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(number)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> Result<Option<InventoryCount>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_COUNTS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut counts = with_lines(&mut tx, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(counts.pop())
    }

    async fn update(
        &self,
        count: &InventoryCount,
        expected: CountStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, count)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let stored = lock_status(&mut tx, count.company_id(), count.id()).await?;
        if stored != expected.as_str() {
            return Err(RepositoryError::CountChanged);
        }
        sqlx::query(
            "UPDATE inventory_counts SET counted_on = CAST($3 AS DATE), notes = $4, status = $5 \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(count.id().as_uuid())
        .bind(count.company_id().as_uuid())
        .bind(count.counted_on().to_string())
        .bind(count.notes())
        .bind(count.status().as_str())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        // The lines are replaced as a whole, in the same transaction as the count and its audit entry.
        sqlx::query("DELETE FROM inventory_count_items WHERE count_id = $1 AND company_id = $2")
            .bind(count.id().as_uuid())
            .bind(count.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        insert_lines(&mut tx, count).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn set_status(
        &self,
        count: &InventoryCount,
        expected: CountStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, count)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let stored = lock_status(&mut tx, count.company_id(), count.id()).await?;
        if stored != expected.as_str() {
            return Err(RepositoryError::CountChanged);
        }
        sqlx::query("UPDATE inventory_counts SET status = $3 WHERE id = $1 AND company_id = $2")
            .bind(count.id().as_uuid())
            .bind(count.company_id().as_uuid())
            .bind(count.status().as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::InventoryCount,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &InventoryCountQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> Result<Vec<InventoryCount>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_COUNTS);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(status) = query.status() {
            sql.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
            sql.push(" AND (number::text = ")
                .push_bind(text.to_owned())
                .push(" OR strpos(lower(coalesce(notes, '')), lower(")
                .push_bind(text.to_owned())
                .push(")) > 0)");
        }
        let (comparison, direction) = match query.sort() {
            NumberSort::NumberAsc => (">", "ASC"),
            NumberSort::NumberDesc => ("<", "DESC"),
        };
        if let Some(cursor) = after {
            sql.push(format!(" AND (number, id) {comparison} ("))
                .push_bind(cursor.number)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        sql.push(format!(
            " ORDER BY number {direction}, id {direction} LIMIT "
        ))
        .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
        let mut tx = snapshot(&self.pool).await?;
        let rows = sql
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_error)?;
        let counts = with_lines(&mut tx, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(counts)
    }
}

/// The counts of these rows, each with its lines in order: one query for all of them.
async fn with_lines(
    connection: &mut PgConnection,
    rows: Vec<PgRow>,
) -> Result<Vec<InventoryCount>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let line_rows = sqlx::query(
        "SELECT count_id, product_id, counted::text AS counted FROM inventory_count_items \
         WHERE count_id = ANY($1) ORDER BY count_id, position",
    )
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut by_count: HashMap<Uuid, Vec<CountLineDraft>> = HashMap::new();
    for row in &line_rows {
        let count_id: Uuid = row.try_get("count_id").map_err(storage_error)?;
        by_count.entry(count_id).or_default().push(CountLineDraft {
            product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage_error)?),
            counted: row.try_get("counted").map_err(storage_error)?,
        });
    }
    rows.iter()
        .map(|row| count_from_row(row, &mut by_count))
        .collect()
}

fn count_from_row(
    row: &PgRow,
    lines: &mut HashMap<Uuid, Vec<CountLineDraft>>,
) -> Result<InventoryCount, RepositoryError> {
    let raw_id: Uuid = row.try_get("id").map_err(storage_error)?;
    let id = InventoryCountId::from_uuid(raw_id);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let status: String = row.try_get("status").map_err(storage_error)?;
    let status = CountStatus::parse(&status)
        .ok_or_else(|| RepositoryError::Storage(format!("stored count with status {status:?}")))?;
    let draft = InventoryCountDraft {
        counted_on: row.try_get("counted_on").map_err(storage_error)?,
        notes: row.try_get("notes").map_err(storage_error)?,
        lines: lines.remove(&raw_id).unwrap_or_default(),
    };
    let number: i64 = row.try_get("number").map_err(storage_error)?;
    InventoryCount::new(id, company_id, draft)
        .map(|count| count.restored(number, status))
        .map_err(|error| {
            RepositoryError::Storage(format!("stored count {id:?} is invalid: {error}"))
        })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{InventoryCountService, StockService};

    use super::*;
    use crate::{
        PostgresPartyRepository, PostgresPaymentTermRepository, PostgresProductRepository,
        PostgresStockMovementRepository, PostgresStockSettingsRepository, contract,
    };

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_inventory_count() {
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
        let stock_of = || {
            StockService::new(
                PostgresStockMovementRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresStockSettingsRepository::new(pool.clone()),
            )
        };
        let counts = InventoryCountService::new(
            PostgresInventoryCountRepository::new(pool.clone()),
            stock_of(),
        );
        contract::runs_the_inventory_count(&counts, &stock_of(), company, other_company, &seed)
            .await;
    }
}
