use conta_verde_application::{CategoryRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CashFlowGroup, Category, CategoryId, CompanyId, DreGroup,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

const SIBLING_NAME_UNIQUE_INDEX: &str = "categories_siblings_name_key";
const ONE_TRANSFER_UNIQUE_INDEX: &str = "categories_one_transfer_key";
const PARENT_FOREIGN_KEY: &str = "categories_parent_fk";

const SELECT_CATEGORIES: &str = "SELECT id, company_id, parent_id, name, dre_group, \
     cash_flow_group, level, active FROM categories";

pub struct PostgresCategoryRepository {
    pool: PgPool,
}

impl PostgresCategoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn write_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(db) => match db.constraint() {
            Some(SIBLING_NAME_UNIQUE_INDEX) => RepositoryError::DuplicateName,
            Some(ONE_TRANSFER_UNIQUE_INDEX) => RepositoryError::DuplicateTransferCategory,
            // The parent is not a category of that company, one level up, with those groups.
            Some(PARENT_FOREIGN_KEY) => RepositoryError::NotFound,
            _ => storage_error(error),
        },
        _ => storage_error(error),
    }
}

async fn insert(
    connection: &mut PgConnection,
    category: &Category,
    audit: &AuditEntry,
) -> Result<(), RepositoryError> {
    ensure_audit_describes(audit, category)?;
    sqlx::query(
        "INSERT INTO categories (id, company_id, parent_id, name, dre_group, cash_flow_group, \
         level, active) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(category.id().as_uuid())
    .bind(category.company_id().as_uuid())
    .bind(category.parent_id().map(|parent| parent.as_uuid()))
    .bind(category.name())
    .bind(category.dre_group().as_str())
    .bind(category.cash_flow_group().as_str())
    .bind(i16::from(category.level()))
    .bind(category.is_active())
    .execute(&mut *connection)
    .await
    .map_err(write_error)?;
    postgres_audit::insert(connection, audit).await
}

impl CategoryRepository for PostgresCategoryRepository {
    async fn save(&self, category: &Category, audit: &AuditEntry) -> Result<(), RepositoryError> {
        // One transaction: if the category is refused or the audit row cannot be written, the
        // rollback leaves neither behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        insert(&mut tx, category, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn save_all(&self, categories: &[(Category, AuditEntry)]) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        for (category, audit) in categories {
            insert(&mut tx, category, audit).await?;
        }
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> Result<Option<Category>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_CATEGORIES} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(category_from_row).transpose()
    }

    async fn update(&self, category: &Category, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, category)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // Only the name and the active flag: the place in the tree and the groups are fixed.
        let result = sqlx::query(
            "UPDATE categories SET name = $3, active = $4 WHERE id = $1 AND company_id = $2",
        )
        .bind(category.id().as_uuid())
        .bind(category.company_id().as_uuid())
        .bind(category.name())
        .bind(category.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        // Matching no row means the category is not in that company: report it, do not pretend.
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
        id: CategoryId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Category, id.as_uuid()).await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        active: Option<bool>,
    ) -> Result<Vec<Category>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_CATEGORIES} WHERE company_id = $1 AND ($2::boolean IS NULL OR active = $2) \
             ORDER BY lower(name), id"
        ))
        .bind(company_id.as_uuid())
        .bind(active)
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(category_from_row).collect()
    }
}

fn category_from_row(row: &PgRow) -> Result<Category, RepositoryError> {
    let id = CategoryId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let parent_id = row
        .try_get::<Option<uuid::Uuid>, _>("parent_id")
        .map_err(storage_error)?
        .map(CategoryId::from_uuid);
    let dre_group: String = row.try_get("dre_group").map_err(storage_error)?;
    let cash_flow_group: String = row.try_get("cash_flow_group").map_err(storage_error)?;
    let level: i16 = row.try_get("level").map_err(storage_error)?;
    let invalid =
        |what: String| RepositoryError::Storage(format!("stored category {id:?}: {what}"));
    let dre_group = DreGroup::parse(&dre_group)
        .ok_or_else(|| invalid(format!("unknown DRE group `{dre_group}`")))?;
    let cash_flow_group = CashFlowGroup::parse(&cash_flow_group)
        .ok_or_else(|| invalid(format!("unknown cash-flow group `{cash_flow_group}`")))?;
    let level = u8::try_from(level).map_err(|_| invalid(format!("level {level}")))?;
    let name: String = row.try_get("name").map_err(storage_error)?;
    Category::restore(
        id,
        company_id,
        parent_id,
        &name,
        dre_group,
        cash_flow_group,
        level,
        row.try_get("active").map_err(storage_error)?,
    )
    .map_err(|error| invalid(error.to_string()))
}

#[cfg(test)]
mod tests {
    use conta_verde_application::CategoryService;

    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_categories_with_their_trail() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresCategoryRepository::new(pool);
        contract::stores_categories_with_their_trail(&repository, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn saves_a_batch_all_or_nothing() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let repository = PostgresCategoryRepository::new(pool);
        contract::saves_a_batch_all_or_nothing(&repository, company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn applies_the_default_template_once() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = CategoryService::new(PostgresCategoryRepository::new(pool));
        contract::applies_the_default_template_once(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_the_tree_consistent() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = CategoryService::new(PostgresCategoryRepository::new(pool));
        contract::keeps_the_tree_consistent(&service, company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let insert = |columns_and_values: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(&format!(
                    "INSERT INTO categories (id, company_id, name, {columns_and_values}"
                ))
                .bind(uuid::Uuid::new_v4())
                .bind(company.as_uuid())
                .execute(&pool)
                .await
            }
        };
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let case = |sql: &'static str| async move { refused_by(insert(sql).await) };
        assert_eq!(
            case("dre_group, cash_flow_group, level, active) VALUES ($1, $2, 'x', 'profit', 'operating', 1, true)").await.as_deref(),
            Some("categories_dre_group_valid")
        );
        assert_eq!(
            case("dre_group, cash_flow_group, level, active) VALUES ($1, $2, 'x', 'costs', 'cash', 1, true)").await.as_deref(),
            Some("categories_cash_flow_group_valid")
        );
        assert_eq!(
            case("dre_group, cash_flow_group, level, active) VALUES ($1, $2, 'x', 'transfers', 'operating', 1, true)").await.as_deref(),
            Some("categories_transfer_groups_go_together")
        );
        assert_eq!(
            case("dre_group, cash_flow_group, level, active) VALUES ($1, $2, 'x', 'costs', 'operating', 2, true)").await.as_deref(),
            Some("categories_level_matches_parent")
        );
        assert_eq!(
            case("dre_group, cash_flow_group, level, active) VALUES ($1, $2, 'x', 'transfers', 'transfer', 1, false)").await.as_deref(),
            Some("categories_transfer_stays_active")
        );
        // A level-2 row under no parent at all is the foreign key's to refuse.
        assert_eq!(
            case("dre_group, cash_flow_group, level, active, parent_id) VALUES ($1, $2, 'x', 'costs', 'operating', 2, true, gen_random_uuid())").await.as_deref(),
            Some("categories_parent_fk")
        );
        assert_eq!(
            case("dre_group, cash_flow_group, level, active) VALUES ($1, $2, '', 'costs', 'operating', 1, true)").await.as_deref(),
            Some("categories_name_is_short")
        );
    }
}
