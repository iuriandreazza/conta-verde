use conta_verde_application::{
    NameCursor, ProductQuery, ProductRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, Product, ProductDraft, ProductId, ProductKind,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use crate::postgres::{push_name_keyset, storage_error};
use crate::postgres_audit;

const SKU_UNIQUE_INDEX: &str = "products_company_sku_key";

// Amounts are read as text and bound as text with a cast, so no sqlx feature is needed; the domain
// parses what comes back and keeps it without trailing zeros.
const SELECT_PRODUCTS: &str = "SELECT id, company_id, kind, name, sku, unit, ncm, \
     sale_price::text AS sale_price, cost_price::text AS cost_price, \
     minimum_stock::text AS minimum_stock, active \
     FROM products";

pub struct PostgresProductRepository {
    pool: PgPool,
}

impl PostgresProductRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn write_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(db) if db.constraint() == Some(SKU_UNIQUE_INDEX) => {
            RepositoryError::DuplicateSku
        }
        _ => storage_error(error),
    }
}

impl ProductRepository for PostgresProductRepository {
    async fn save(&self, product: &Product, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, product)?;
        // One transaction: if the product is refused (a duplicate SKU) or the audit row cannot be
        // written, the rollback leaves neither behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO products (id, company_id, kind, name, sku, unit, ncm, sale_price, \
             cost_price, minimum_stock, active) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS NUMERIC), CAST($9 AS NUMERIC), \
             CAST($10 AS NUMERIC), $11)",
        )
        .bind(product.id().as_uuid())
        .bind(product.company_id().as_uuid())
        .bind(product.kind().as_str())
        .bind(product.name())
        .bind(product.sku())
        .bind(product.unit())
        .bind(product.ncm())
        .bind(product.sale_price().map(|amount| amount.to_string()))
        .bind(product.cost_price().map(|amount| amount.to_string()))
        .bind(product.minimum_stock().map(|amount| amount.to_string()))
        .bind(product.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<Option<Product>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_PRODUCTS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(product_from_row).transpose()
    }

    async fn update(&self, product: &Product, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, product)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE products SET name = $3, sku = $4, unit = $5, ncm = $6, \
             sale_price = CAST($7 AS NUMERIC), cost_price = CAST($8 AS NUMERIC), \
             minimum_stock = CAST($9 AS NUMERIC), active = $10 \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(product.id().as_uuid())
        .bind(product.company_id().as_uuid())
        .bind(product.name())
        .bind(product.sku())
        .bind(product.unit())
        .bind(product.ncm())
        .bind(product.sale_price().map(|amount| amount.to_string()))
        .bind(product.cost_price().map(|amount| amount.to_string()))
        .bind(product.minimum_stock().map(|amount| amount.to_string()))
        .bind(product.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        // Matching no row means the product is not in that company: report it, do not pretend.
        // The transaction is dropped, so no audit row is left for a write that did not happen.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Product, id.as_uuid()).await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &ProductQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<Product>, RepositoryError> {
        // Every value is bound, never spliced into the text: the pieces pushed as text are fixed.
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_PRODUCTS);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(kind) = query.kind() {
            sql.push(" AND kind = ").push_bind(kind.as_str());
        }
        if let Some(active) = query.active() {
            sql.push(" AND active = ").push_bind(active);
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves. SKUs are
            // stored in upper case, so the text is compared in upper case.
            sql.push(" AND (strpos(lower(name), lower(")
                .push_bind(text)
                .push(")) > 0 OR strpos(coalesce(sku, ''), upper(")
                .push_bind(text)
                .push(")) > 0)");
        }
        push_name_keyset(&mut sql, query.sort(), after, limit);

        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(product_from_row).collect()
    }
}

fn product_from_row(row: &PgRow) -> Result<Product, RepositoryError> {
    let id = ProductId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let kind = ProductKind::parse(&kind)
        .ok_or_else(|| RepositoryError::Storage(format!("unknown product kind `{kind}`")))?;
    let draft = ProductDraft {
        kind,
        name: row.try_get("name").map_err(storage_error)?,
        sku: row.try_get("sku").map_err(storage_error)?,
        unit: row.try_get("unit").map_err(storage_error)?,
        ncm: row.try_get("ncm").map_err(storage_error)?,
        sale_price: row.try_get("sale_price").map_err(storage_error)?,
        cost_price: row.try_get("cost_price").map_err(storage_error)?,
        minimum_stock: row.try_get("minimum_stock").map_err(storage_error)?,
        active: row.try_get("active").map_err(storage_error)?,
    };
    Product::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored product {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::ProductService;

    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_products_with_their_trail() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresProductRepository::new(pool);
        contract::stores_products_with_their_trail(&repository, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn edits_products_and_records_only_changes() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = ProductService::new(PostgresProductRepository::new(pool));
        contract::edits_products_and_records_only_changes(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn filters_and_sorts_products() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = ProductService::new(PostgresProductRepository::new(pool));
        contract::filters_and_sorts_products(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn breaks_ties_between_equal_product_names() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = ProductService::new(PostgresProductRepository::new(pool));
        contract::breaks_ties_between_equal_product_names(&service, company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let insert = |statement: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(&format!(
                    "INSERT INTO products (id, company_id, kind, name, unit, active, {statement}"
                ))
                .bind(uuid::Uuid::new_v4())
                .bind(company.as_uuid())
                .execute(&pool)
                .await
            }
        };
        // A service with an NCM, a malformed NCM, a negative price and an unknown kind.
        assert!(
            insert("ncm) VALUES ($1, $2, 'service', 'x', 'UN', true, '19059090')")
                .await
                .is_err()
        );
        assert!(
            insert("ncm) VALUES ($1, $2, 'product', 'x', 'UN', true, '1905')")
                .await
                .is_err()
        );
        assert!(
            insert("sale_price) VALUES ($1, $2, 'product', 'x', 'UN', true, -1)")
                .await
                .is_err()
        );
        assert!(
            insert("ncm) VALUES ($1, $2, 'gadget', 'x', 'UN', true, NULL)")
                .await
                .is_err()
        );
        assert!(
            insert("ncm) VALUES ($1, $2, 'product', 'x', 'UN', true, '19059090')")
                .await
                .is_ok()
        );
    }
}
