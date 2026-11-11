use std::collections::HashMap;

use conta_verde_application::{
    NameCursor, PriceListQuery, PriceListRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, PriceList, PriceListDraft, PriceListId,
    PriceListItemDraft, ProductId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{push_name_keyset, snapshot, storage_error};
use crate::postgres_audit;

const NAME_UNIQUE_INDEX: &str = "price_lists_company_name_key";

const SELECT_PAYMENT_TERMS: &str = "SELECT id, company_id, name, active FROM price_lists";

pub struct PostgresPriceListRepository {
    pool: PgPool,
}

impl PostgresPriceListRepository {
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

/// Writes the prices of a list, in order, on the connection of the transaction that writes the list: they
/// commit with it or not at all.
async fn insert_items(
    connection: &mut PgConnection,
    list: &PriceList,
) -> Result<(), RepositoryError> {
    for (index, item) in list.items().iter().enumerate() {
        sqlx::query(
            "INSERT INTO price_list_items (company_id, list_id, position, product_id, price) \
             VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC))",
        )
        .bind(list.company_id().as_uuid())
        .bind(list.id().as_uuid())
        .bind(
            i16::try_from(index + 1).map_err(|_| {
                RepositoryError::Storage("a price list has too many prices".to_owned())
            })?,
        )
        .bind(item.product_id().as_uuid())
        .bind(item.price().to_string())
        .execute(&mut *connection)
        .await
        .map_err(write_error)?;
    }
    Ok(())
}

impl PriceListRepository for PostgresPriceListRepository {
    async fn save(&self, list: &PriceList, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, list)?;
        // One transaction: if the list is refused (a duplicate name) or the audit row cannot be
        // written, the rollback leaves neither the list nor its prices behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO price_lists (id, company_id, name, active) VALUES ($1, $2, $3, $4)",
        )
        .bind(list.id().as_uuid())
        .bind(list.company_id().as_uuid())
        .bind(list.name())
        .bind(list.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        insert_items(&mut tx, list).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PriceListId,
    ) -> Result<Option<PriceList>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_PAYMENT_TERMS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut lists = with_items(&mut tx, company_id, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(lists.pop())
    }

    async fn update(&self, list: &PriceList, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, list)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE price_lists SET name = $3, active = $4 WHERE id = $1 AND company_id = $2",
        )
        .bind(list.id().as_uuid())
        .bind(list.company_id().as_uuid())
        .bind(list.name())
        .bind(list.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        // Matching no row means the list is not in that company: report it, do not pretend. The
        // transaction is dropped, so the prices it was about to replace are untouched.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        // The prices are replaced as a whole, inside the same transaction as the list and
        // its audit entry: a refusal anywhere leaves the old ones in place.
        sqlx::query("DELETE FROM price_list_items WHERE list_id = $1 AND company_id = $2")
            .bind(list.id().as_uuid())
            .bind(list.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        insert_items(&mut tx, list).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PriceListId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::PriceList, id.as_uuid()).await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PriceListQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<PriceList>, RepositoryError> {
        // Every value is bound, never spliced into the text: the pieces pushed as text are fixed.
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_PAYMENT_TERMS);
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

        let mut tx = snapshot(&self.pool).await?;
        let rows = sql
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_error)?;
        let lists = with_items(&mut tx, company_id, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(lists)
    }
}

/// The lists of these rows, each with its prices in order: one query for all of them.
async fn with_items(
    connection: &mut PgConnection,
    company_id: CompanyId,
    rows: Vec<PgRow>,
) -> Result<Vec<PriceList>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let item_rows = sqlx::query(
        "SELECT list_id, product_id, price::text AS price FROM price_list_items \
         WHERE company_id = $1 AND list_id = ANY($2) ORDER BY list_id, position",
    )
    .bind(company_id.as_uuid())
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut by_list: HashMap<Uuid, Vec<PriceListItemDraft>> = HashMap::new();
    for row in &item_rows {
        let list_id: Uuid = row.try_get("list_id").map_err(storage_error)?;
        let product_id: Uuid = row.try_get("product_id").map_err(storage_error)?;
        by_list
            .entry(list_id)
            .or_default()
            .push(PriceListItemDraft {
                product_id: ProductId::from_uuid(product_id),
                price: row.try_get("price").map_err(storage_error)?,
            });
    }
    rows.iter()
        .map(|row| list_from_row(row, &mut by_list))
        .collect()
}

fn list_from_row(
    row: &PgRow,
    items: &mut HashMap<Uuid, Vec<PriceListItemDraft>>,
) -> Result<PriceList, RepositoryError> {
    let raw_id: Uuid = row.try_get("id").map_err(storage_error)?;
    let id = PriceListId::from_uuid(raw_id);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let draft = PriceListDraft {
        name: row.try_get("name").map_err(storage_error)?,
        items: items.remove(&raw_id).unwrap_or_default(),
        active: row.try_get("active").map_err(storage_error)?,
    };
    PriceList::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored price list {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::PriceListService;

    use super::*;
    use crate::{
        PostgresPartyRepository, PostgresPaymentTermRepository, PostgresProductRepository,
        RegisterDocumentReferences, contract,
    };

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_price_list_lifecycle() {
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
        let service = PriceListService::new(
            PostgresPriceListRepository::new(pool.clone()),
            RegisterDocumentReferences::new(
                PostgresPartyRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresPaymentTermRepository::new(pool),
            ),
        );
        contract::runs_the_price_list_lifecycle(&service, company, other_company, &seed).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_one_row_can_show() {
        let pool = contract::owner_pool().await;
        let company = contract::create_company(&pool).await;
        let seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            company,
        )
        .await;
        let list_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO price_lists (id, company_id, name, active) VALUES ($1, $2, 'Base', true)",
        )
        .bind(list_id)
        .bind(company.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let item = |position: i16, product: Uuid, price: &'static str, company: Uuid| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO price_list_items (company_id, list_id, position, product_id, price) \
                     VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC))",
                )
                .bind(company)
                .bind(list_id)
                .bind(position)
                .bind(product)
                .bind(price)
                .execute(&pool)
                .await
            }
        };
        let (mine, coffee) = (company.as_uuid(), seed.product.as_uuid());
        let cases = [
            (
                item(0, coffee, "1", mine).await,
                "price_list_items_position_valid",
            ),
            (
                item(1001, coffee, "1", mine).await,
                "price_list_items_position_valid",
            ),
            (
                item(1, coffee, "-0.01", mine).await,
                "price_list_items_price_valid",
            ),
            // More than six decimals is refused, not rounded to a valid price.
            (
                item(1, coffee, "1.0000001", mine).await,
                "price_list_items_price_valid",
            ),
            (
                item(1, coffee, "10000000000000", mine).await,
                "price_list_items_price_valid",
            ),
            // The item must be in its list's company, and so must its product.
            (
                item(1, coffee, "1", Uuid::new_v4()).await,
                "price_list_items_list_fk",
            ),
            (
                item(1, Uuid::new_v4(), "1", mine).await,
                "price_list_items_product_fk",
            ),
        ];
        for (result, constraint) in cases {
            assert_eq!(refused_by(result).as_deref(), Some(constraint));
        }
        // A free price is valid; the same product twice, or the same position twice, is not.
        item(1, coffee, "0", mine).await.unwrap();
        assert_eq!(
            refused_by(item(2, coffee, "1", mine).await).as_deref(),
            Some("price_list_items_one_per_product_key")
        );
        assert_eq!(
            refused_by(item(1, seed.other_product.as_uuid(), "1", mine).await).as_deref(),
            Some("price_list_items_pkey")
        );
    }
}
