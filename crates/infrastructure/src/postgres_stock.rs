use std::collections::HashMap;
use std::str::FromStr;

use conta_verde_application::{
    DateCursor, MovementTotals, RepositoryError, StockFloor, StockMovementQuery,
    StockMovementRepository, StockSettingsRepository, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CalendarDate, CompanyId, Decimal, ProductId,
    StockBalance, StockMovement, StockMovementId, StockMovementKind, StockOrigin, StockOriginKind,
    StockSettings,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{snapshot, storage_error};
use crate::postgres_audit;

const ORIGIN_UNIQUE_INDEX: &str = "stock_movements_origin_key";
const REVERSES_UNIQUE_INDEX: &str = "stock_movements_reverses_key";

const SELECT_MOVEMENTS: &str = "SELECT id, company_id, product_id, kind, quantity::text AS quantity, \
     value::text AS value, to_char(occurred_on, 'YYYY-MM-DD') AS occurred_on, origin_kind, origin_id, \
     origin_line, reverses, note FROM stock_movements";

pub struct PostgresStockMovementRepository {
    pool: PgPool,
}

impl PostgresStockMovementRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn decimal(raw: &str) -> Result<Decimal, RepositoryError> {
    Decimal::from_str(raw)
        .map_err(|_| RepositoryError::Storage(format!("stored stock figure {raw} is invalid")))
}

/// What each product has, through `on` when given, read on `connection`; products with no movement are empty.
async fn read_balances(
    connection: &mut sqlx::PgConnection,
    company_id: CompanyId,
    products: &[ProductId],
    on: Option<CalendarDate>,
) -> Result<Vec<(ProductId, StockBalance)>, RepositoryError> {
    let rows = sqlx::query(
        "SELECT product_id, COALESCE(SUM(quantity), 0)::text AS quantity, \
         COALESCE(SUM(value), 0)::text AS value FROM stock_movements \
         WHERE company_id = $1 AND product_id = ANY($2) \
         AND ($3::text IS NULL OR occurred_on <= CAST($3 AS DATE)) GROUP BY product_id",
    )
    .bind(company_id.as_uuid())
    .bind(
        products
            .iter()
            .map(|id| id.as_uuid())
            .collect::<Vec<Uuid>>(),
    )
    .bind(on.map(|day| day.to_string()))
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut found: HashMap<Uuid, StockBalance> = HashMap::new();
    for row in &rows {
        let quantity: String = row.try_get("quantity").map_err(storage_error)?;
        let value: String = row.try_get("value").map_err(storage_error)?;
        found.insert(
            row.try_get("product_id").map_err(storage_error)?,
            StockBalance::new(decimal(&quantity)?, decimal(&value)?),
        );
    }
    Ok(products
        .iter()
        .map(|id| (*id, found.get(&id.as_uuid()).copied().unwrap_or_default()))
        .collect())
}

impl StockMovementRepository for PostgresStockMovementRepository {
    async fn record(
        &self,
        company_id: CompanyId,
        movements: &[StockMovement],
        seen: &[(ProductId, StockBalance)],
        floor: StockFloor,
        audits: &[AuditEntry],
    ) -> Result<(), RepositoryError> {
        if movements.len() != audits.len() {
            return Err(RepositoryError::Storage(
                "each movement needs its audit entry".to_owned(),
            ));
        }
        for (movement, audit) in movements.iter().zip(audits) {
            ensure_audit_describes(audit, movement)?;
        }
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The products are locked first, in id order so two writes cannot wait on each other, and the balances
        // are read in the next statement: in READ COMMITTED a sum read in the statement that waits for a lock
        // uses the snapshot from before the wait.
        let mut ids: Vec<Uuid> = seen.iter().map(|(id, _)| id.as_uuid()).collect();
        ids.sort();
        sqlx::query(
            "SELECT id FROM products WHERE company_id = $1 AND id = ANY($2) ORDER BY id \
             FOR NO KEY UPDATE",
        )
        .bind(company_id.as_uuid())
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        let products: Vec<ProductId> = seen.iter().map(|(id, _)| *id).collect();
        let stored = read_balances(&mut tx, company_id, &products, None).await?;
        let mut books: HashMap<ProductId, StockBalance> = HashMap::new();
        for ((product, expected), (_, now)) in seen.iter().zip(&stored) {
            if now != expected {
                return Err(RepositoryError::StockChanged);
            }
            books.insert(*product, *now);
        }
        for movement in movements {
            let balance = books.entry(movement.product_id()).or_default();
            *balance = balance.after(movement);
        }
        if floor == StockFloor::Zero {
            for (product, before) in seen {
                let after = books.get(product).copied().unwrap_or(*before);
                if after.quantity() < Decimal::ZERO && after.quantity() < before.quantity() {
                    return Err(RepositoryError::StockInsufficient);
                }
            }
        }
        for (movement, audit) in movements.iter().zip(audits) {
            sqlx::query(
                "INSERT INTO stock_movements (id, company_id, product_id, kind, quantity, value, \
                 occurred_on, origin_kind, origin_id, origin_line, reverses, note) \
                 VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC), CAST($6 AS NUMERIC), \
                 CAST($7 AS DATE), $8, $9, $10, $11, $12)",
            )
            .bind(movement.id().as_uuid())
            .bind(movement.company_id().as_uuid())
            .bind(movement.product_id().as_uuid())
            .bind(movement.kind().as_str())
            .bind(movement.quantity().to_string())
            .bind(movement.value().to_string())
            .bind(movement.occurred_on().to_string())
            .bind(movement.origin().map(|origin| origin.kind.as_str()))
            .bind(movement.origin().map(|origin| origin.id))
            .bind(
                movement
                    .origin()
                    .map(|origin| i16::try_from(origin.line).unwrap_or(i16::MAX)),
            )
            .bind(movement.reverses().map(|id| id.as_uuid()))
            .bind(movement.note())
            .execute(&mut *tx)
            .await
            .map_err(|error| match &error {
                sqlx::Error::Database(db) if db.constraint() == Some(ORIGIN_UNIQUE_INDEX) => {
                    RepositoryError::DuplicateOrigin
                }
                sqlx::Error::Database(db) if db.constraint() == Some(REVERSES_UNIQUE_INDEX) => {
                    RepositoryError::DuplicateReversal
                }
                _ => storage_error(error),
            })?;
            postgres_audit::insert(&mut tx, audit).await?;
        }
        tx.commit().await.map_err(storage_error)
    }

    async fn balances(
        &self,
        company_id: CompanyId,
        products: &[ProductId],
        on: Option<CalendarDate>,
    ) -> Result<Vec<(ProductId, StockBalance)>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let balances = read_balances(&mut tx, company_id, products, on).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(balances)
    }

    async fn totals_by_product(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<Vec<(ProductId, MovementTotals)>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let rows = sqlx::query(
            "SELECT product_id, \
               COALESCE(SUM(quantity) FILTER (WHERE kind = 'in'), 0)::text AS in_quantity, \
               COALESCE(SUM(value) FILTER (WHERE kind = 'in'), 0)::text AS in_value, \
               COALESCE(SUM(quantity) FILTER (WHERE kind = 'out'), 0)::text AS out_quantity, \
               COALESCE(SUM(value) FILTER (WHERE kind = 'out'), 0)::text AS out_value, \
               COALESCE(SUM(quantity) FILTER (WHERE kind = 'adjustment'), 0)::text AS adj_quantity, \
               COALESCE(SUM(value) FILTER (WHERE kind = 'adjustment'), 0)::text AS adj_value \
             FROM stock_movements \
             WHERE company_id = $1 AND occurred_on BETWEEN CAST($2 AS DATE) AND CAST($3 AS DATE) \
             GROUP BY product_id",
        )
        .bind(company_id.as_uuid())
        .bind(from.to_string())
        .bind(to.to_string())
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        let figure = |row: &PgRow, column: &str| -> Result<Decimal, RepositoryError> {
            let raw: String = row.try_get(column).map_err(storage_error)?;
            decimal(&raw)
        };
        rows.iter()
            .map(|row| {
                Ok((
                    ProductId::from_uuid(row.try_get("product_id").map_err(storage_error)?),
                    MovementTotals {
                        entered_quantity: figure(row, "in_quantity")?,
                        entered_value: figure(row, "in_value")?,
                        exited_quantity: figure(row, "out_quantity")?,
                        exited_value: figure(row, "out_value")?,
                        adjusted_quantity: figure(row, "adj_quantity")?,
                        adjusted_value: figure(row, "adj_value")?,
                    },
                ))
            })
            .collect()
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: StockMovementId,
    ) -> Result<Option<StockMovement>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_MOVEMENTS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(movement_from_row).transpose()
    }

    async fn of_origin(
        &self,
        company_id: CompanyId,
        kind: StockOriginKind,
        id: Uuid,
    ) -> Result<Vec<StockMovement>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_MOVEMENTS} WHERE company_id = $1 AND origin_kind = $2 AND origin_id = $3 \
             ORDER BY created_at, id"
        ))
        .bind(company_id.as_uuid())
        .bind(kind.as_str())
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(movement_from_row).collect()
    }

    async fn reversal_of(
        &self,
        company_id: CompanyId,
        id: StockMovementId,
    ) -> Result<Option<StockMovement>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_MOVEMENTS} WHERE company_id = $1 AND reverses = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(movement_from_row).transpose()
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &StockMovementQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> Result<Vec<StockMovement>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_MOVEMENTS);
        push_movement_filters(&mut sql, company_id, query);
        if let Some(cursor) = after {
            sql.push(" AND (occurred_on, id) < (CAST(")
                .push_bind(cursor.date.to_string())
                .push(" AS DATE), ")
                .push_bind(cursor.id)
                .push(")");
        }
        sql.push(" ORDER BY occurred_on DESC, id DESC LIMIT ")
            .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
        let mut tx = snapshot(&self.pool).await?;
        let rows = sql
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        rows.iter().map(movement_from_row).collect()
    }

    async fn count_by_kind(
        &self,
        company_id: CompanyId,
        query: &StockMovementQuery,
    ) -> Result<Vec<(StockMovementKind, u64)>, RepositoryError> {
        let mut sql =
            QueryBuilder::<Postgres>::new("SELECT kind, count(*) AS total FROM stock_movements");
        push_movement_filters(&mut sql, company_id, &query.clone().any_kind());
        sql.push(" GROUP BY kind");
        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter()
            .map(|row| {
                let kind: String = row.get("kind");
                let total: i64 = row.get("total");
                let kind = StockMovementKind::parse(&kind)
                    .ok_or_else(|| RepositoryError::Storage(format!("unknown kind {kind}")))?;
                Ok((kind, u64::try_from(total).unwrap_or(0)))
            })
            .collect()
    }
}

/// ` WHERE company_id = ... AND ...` for what `query` narrows to, shared by the list and the counts so that both
/// agree on what matches.
fn push_movement_filters(
    sql: &mut QueryBuilder<'_, Postgres>,
    company_id: CompanyId,
    query: &StockMovementQuery,
) {
    sql.push(" WHERE company_id = ")
        .push_bind(company_id.as_uuid());
    if let Some(product) = query.product() {
        sql.push(" AND product_id = ").push_bind(product.as_uuid());
    }
    if let Some(kind) = query.kind() {
        sql.push(" AND kind = ").push_bind(kind.as_str());
    }
    if let Some(from) = query.first_day() {
        sql.push(" AND occurred_on >= CAST(")
            .push_bind(from.to_string())
            .push(" AS DATE)");
    }
    if let Some(to) = query.last_day() {
        sql.push(" AND occurred_on <= CAST(")
            .push_bind(to.to_string())
            .push(" AS DATE)");
    }
}

fn movement_from_row(row: &PgRow) -> Result<StockMovement, RepositoryError> {
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let kind = StockMovementKind::parse(&kind)
        .ok_or_else(|| RepositoryError::Storage(format!("stored movement of kind {kind:?}")))?;
    let quantity: String = row.try_get("quantity").map_err(storage_error)?;
    let value: String = row.try_get("value").map_err(storage_error)?;
    let day: String = row.try_get("occurred_on").map_err(storage_error)?;
    let origin_kind: Option<String> = row.try_get("origin_kind").map_err(storage_error)?;
    let origin_id: Option<Uuid> = row.try_get("origin_id").map_err(storage_error)?;
    let origin_line: Option<i16> = row.try_get("origin_line").map_err(storage_error)?;
    let origin = match (origin_kind, origin_id, origin_line) {
        (Some(kind), Some(id), Some(line)) => Some(StockOrigin {
            kind: StockOriginKind::parse(&kind).ok_or_else(|| {
                RepositoryError::Storage(format!("stored movement of origin {kind:?}"))
            })?,
            id,
            line: u16::try_from(line).unwrap_or(0),
        }),
        _ => None,
    };
    let reverses: Option<Uuid> = row.try_get("reverses").map_err(storage_error)?;
    Ok(StockMovement::restored(
        StockMovementId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        ProductId::from_uuid(row.try_get("product_id").map_err(storage_error)?),
        kind,
        decimal(&quantity)?,
        decimal(&value)?,
        CalendarDate::parse(&day)
            .ok_or_else(|| RepositoryError::Storage(format!("stored day {day} is invalid")))?,
        origin,
        reverses.map(StockMovementId::from_uuid),
        row.try_get("note").map_err(storage_error)?,
    ))
}

pub struct PostgresStockSettingsRepository {
    pool: PgPool,
}

impl PostgresStockSettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl StockSettingsRepository for PostgresStockSettingsRepository {
    async fn find(&self, company_id: CompanyId) -> Result<Option<StockSettings>, RepositoryError> {
        let blocked: Option<bool> = sqlx::query_scalar(
            "SELECT block_sale_without_balance FROM stock_settings WHERE company_id = $1",
        )
        .bind(company_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(blocked.map(|blocked| StockSettings::new(company_id, blocked)))
    }

    async fn save(
        &self,
        settings: &StockSettings,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, settings)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO stock_settings (company_id, block_sale_without_balance) VALUES ($1, $2) \
             ON CONFLICT (company_id) DO UPDATE \
             SET block_sale_without_balance = EXCLUDED.block_sale_without_balance",
        )
        .bind(settings.company_id().as_uuid())
        .bind(settings.blocks_sales_without_balance())
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
            AuditEntity::StockSettings,
            company_id.as_uuid(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{
        FinancialEntryService, PurchaseService, SaleService, StockService,
    };

    use super::*;
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresPartyRepository, PostgresPaymentMethodRepository,
        PostgresPaymentTermRepository, PostgresProductRepository, PostgresPurchaseOrderRepository,
        PostgresQuoteRepository, PostgresSaleRepository, RegisterDocumentReferences,
        RegisterReferences, contract,
    };

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_stock_ledger() {
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
        let stock = StockService::new(
            PostgresStockMovementRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresStockSettingsRepository::new(pool.clone()),
        );
        contract::runs_the_stock_ledger(&stock, company, other_company, &seed).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stock_follows_purchases_and_sales() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let references = || {
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
            )
        };
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
        };
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
        let purchases = PurchaseService::new(
            PostgresPurchaseOrderRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(),
            ),
            references(),
            documents(),
            stock_of(),
        );
        let sales = SaleService::new(
            PostgresSaleRepository::new(pool.clone()),
            PostgresQuoteRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(),
            ),
            references(),
            documents(),
            stock_of(),
        );
        contract::stock_follows_purchases_and_sales(
            &purchases,
            &sales,
            &stock_of(),
            company,
            &entry_seed,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn blocks_a_sale_without_balance_when_the_company_says_so() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let references = || {
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
            )
        };
        let stock_of = || {
            StockService::new(
                PostgresStockMovementRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresStockSettingsRepository::new(pool.clone()),
            )
        };
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
                references(),
            ),
            references(),
            RegisterDocumentReferences::new(
                PostgresPartyRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresPaymentTermRepository::new(pool.clone()),
            ),
            stock_of(),
        );
        contract::blocks_a_sale_without_balance_when_the_company_says_so(
            &sales,
            &stock_of(),
            company,
            other_company,
            &entry_seed,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn reports_the_stock_valuation_and_its_movements() {
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
        let stock = StockService::new(
            PostgresStockMovementRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresStockSettingsRepository::new(pool.clone()),
        );
        contract::reports_the_stock_valuation_and_its_movements(
            &stock,
            company,
            other_company,
            &seed,
        )
        .await;
    }
}
