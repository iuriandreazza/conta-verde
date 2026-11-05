use conta_verde_application::{
    NameCursor, PaymentMethodQuery, PaymentMethodRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, PaymentMethod, PaymentMethodDraft,
    PaymentMethodId, PaymentMethodKind,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use crate::postgres::{push_name_keyset, storage_error};
use crate::postgres_audit;

const NAME_UNIQUE_INDEX: &str = "payment_methods_company_name_key";

const SELECT_PAYMENT_METHODS: &str =
    "SELECT id, company_id, kind, name, active FROM payment_methods";

pub struct PostgresPaymentMethodRepository {
    pool: PgPool,
}

impl PostgresPaymentMethodRepository {
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

impl PaymentMethodRepository for PostgresPaymentMethodRepository {
    async fn save(
        &self,
        method: &PaymentMethod,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, method)?;
        // One transaction: if the method is refused (a duplicate name) or the audit row cannot be
        // written, the rollback leaves neither behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO payment_methods (id, company_id, kind, name, active) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(method.id().as_uuid())
        .bind(method.company_id().as_uuid())
        .bind(method.kind().as_str())
        .bind(method.name())
        .bind(method.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> Result<Option<PaymentMethod>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_PAYMENT_METHODS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(method_from_row).transpose()
    }

    async fn update(
        &self,
        method: &PaymentMethod,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, method)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE payment_methods SET kind = $3, name = $4, active = $5 WHERE id = $1 AND company_id = $2",
        )
        .bind(method.id().as_uuid())
        .bind(method.company_id().as_uuid())
        .bind(method.kind().as_str())
        .bind(method.name())
        .bind(method.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        // Matching no row means the method is not in that company: report it, do not pretend. The
        // transaction is dropped, so no audit row is left for a write that did not happen.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::PaymentMethod,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PaymentMethodQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<PaymentMethod>, RepositoryError> {
        // Every value is bound, never spliced into the text: the pieces pushed as text are fixed.
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_PAYMENT_METHODS);
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
        rows.iter().map(method_from_row).collect()
    }
}

fn method_from_row(row: &PgRow) -> Result<PaymentMethod, RepositoryError> {
    let id = PaymentMethodId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let kind = PaymentMethodKind::parse(&kind)
        .ok_or_else(|| RepositoryError::Storage(format!("unknown payment method kind `{kind}`")))?;
    let draft = PaymentMethodDraft {
        kind,
        name: row.try_get("name").map_err(storage_error)?,
        active: row.try_get("active").map_err(storage_error)?,
    };
    PaymentMethod::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored payment method {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::PaymentMethodService;

    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_payment_methods_with_their_trail() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresPaymentMethodRepository::new(pool);
        contract::stores_payment_methods_with_their_trail(&repository, company, other_company)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn edits_payment_methods_and_records_only_changes() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = PaymentMethodService::new(PostgresPaymentMethodRepository::new(pool));
        contract::edits_payment_methods_and_records_only_changes(&service, company, other_company)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn filters_and_sorts_payment_methods() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = PaymentMethodService::new(PostgresPaymentMethodRepository::new(pool));
        contract::filters_and_sorts_payment_methods(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_an_unknown_kind_or_a_bad_name() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        for (kind, name, constraint) in [
            (
                "boleto".to_owned(),
                "x".to_owned(),
                "payment_methods_kind_valid",
            ),
            (
                "pix".to_owned(),
                String::new(),
                "payment_methods_name_is_short",
            ),
            (
                "pix".to_owned(),
                " Pix ".to_owned(),
                "payment_methods_name_is_short",
            ),
            (
                "pix".to_owned(),
                "x".repeat(201),
                "payment_methods_name_is_short",
            ),
        ] {
            let result = sqlx::query(
                "INSERT INTO payment_methods (id, company_id, kind, name, active) VALUES ($1, $2, $3, $4, true)",
            )
            .bind(uuid::Uuid::new_v4())
            .bind(company.as_uuid())
            .bind(&kind)
            .bind(&name)
            .execute(&pool)
            .await;
            match result {
                Err(sqlx::Error::Database(db)) => assert_eq!(db.constraint(), Some(constraint)),
                other => panic!("expected {constraint} to refuse it, got {other:?}"),
            }
        }
    }
}
