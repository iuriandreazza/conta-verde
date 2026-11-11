use std::collections::HashMap;

use conta_verde_application::{
    NameCursor, PaymentTermQuery, PaymentTermRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, InstallmentDraft, PaymentTerm,
    PaymentTermDraft, PaymentTermId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{push_name_keyset, snapshot, storage_error};
use crate::postgres_audit;

const NAME_UNIQUE_INDEX: &str = "payment_terms_company_name_key";

const SELECT_PAYMENT_TERMS: &str = "SELECT id, company_id, name, active FROM payment_terms";

pub struct PostgresPaymentTermRepository {
    pool: PgPool,
}

impl PostgresPaymentTermRepository {
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

/// Writes the installments of a term, in order, on the connection of the transaction that writes
/// the term: they commit with it or not at all.
async fn insert_installments(
    connection: &mut PgConnection,
    term: &PaymentTerm,
) -> Result<(), RepositoryError> {
    for (index, installment) in term.installments().iter().enumerate() {
        sqlx::query(
            "INSERT INTO payment_term_installments (company_id, term_id, position, days, percent) \
             VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC))",
        )
        .bind(term.company_id().as_uuid())
        .bind(term.id().as_uuid())
        .bind(i16::try_from(index + 1).map_err(|_| {
            RepositoryError::Storage("a payment term has too many installments".to_owned())
        })?)
        .bind(i32::try_from(installment.days()).map_err(|_| {
            RepositoryError::Storage("an installment is due too far ahead".to_owned())
        })?)
        .bind(installment.percent().to_string())
        .execute(&mut *connection)
        .await
        .map_err(write_error)?;
    }
    Ok(())
}

impl PaymentTermRepository for PostgresPaymentTermRepository {
    async fn save(&self, term: &PaymentTerm, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, term)?;
        // One transaction: if the term is refused (a duplicate name) or the audit row cannot be
        // written, the rollback leaves neither the term nor its installments behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO payment_terms (id, company_id, name, active) VALUES ($1, $2, $3, $4)",
        )
        .bind(term.id().as_uuid())
        .bind(term.company_id().as_uuid())
        .bind(term.name())
        .bind(term.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        insert_installments(&mut tx, term).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<Option<PaymentTerm>, RepositoryError> {
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
        let mut terms = with_installments(&mut tx, company_id, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(terms.pop())
    }

    async fn update(&self, term: &PaymentTerm, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, term)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE payment_terms SET name = $3, active = $4 WHERE id = $1 AND company_id = $2",
        )
        .bind(term.id().as_uuid())
        .bind(term.company_id().as_uuid())
        .bind(term.name())
        .bind(term.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        // Matching no row means the term is not in that company: report it, do not pretend. The
        // transaction is dropped, so the installments it was about to replace are untouched.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        // The installments are replaced as a whole, inside the same transaction as the term and
        // its audit entry: a refusal anywhere leaves the old ones in place.
        sqlx::query("DELETE FROM payment_term_installments WHERE term_id = $1 AND company_id = $2")
            .bind(term.id().as_uuid())
            .bind(term.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        insert_installments(&mut tx, term).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::PaymentTerm,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PaymentTermQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<PaymentTerm>, RepositoryError> {
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
        let terms = with_installments(&mut tx, company_id, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(terms)
    }
}

/// The terms of these rows, each with its installments in order: one query for all of them.
async fn with_installments(
    connection: &mut PgConnection,
    company_id: CompanyId,
    rows: Vec<PgRow>,
) -> Result<Vec<PaymentTerm>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let installment_rows = sqlx::query(
        "SELECT term_id, days, percent::text AS percent FROM payment_term_installments \
         WHERE company_id = $1 AND term_id = ANY($2) ORDER BY term_id, position",
    )
    .bind(company_id.as_uuid())
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut by_term: HashMap<Uuid, Vec<InstallmentDraft>> = HashMap::new();
    for row in &installment_rows {
        let term_id: Uuid = row.try_get("term_id").map_err(storage_error)?;
        let days: i32 = row.try_get("days").map_err(storage_error)?;
        by_term.entry(term_id).or_default().push(InstallmentDraft {
            days: u32::try_from(days).map_err(|_| {
                RepositoryError::Storage(format!("stored installment with {days} days"))
            })?,
            percent: row.try_get("percent").map_err(storage_error)?,
        });
    }
    rows.iter()
        .map(|row| term_from_row(row, &mut by_term))
        .collect()
}

fn term_from_row(
    row: &PgRow,
    installments: &mut HashMap<Uuid, Vec<InstallmentDraft>>,
) -> Result<PaymentTerm, RepositoryError> {
    let raw_id: Uuid = row.try_get("id").map_err(storage_error)?;
    let id = PaymentTermId::from_uuid(raw_id);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let draft = PaymentTermDraft {
        name: row.try_get("name").map_err(storage_error)?,
        installments: installments.remove(&raw_id).unwrap_or_default(),
        active: row.try_get("active").map_err(storage_error)?,
    };
    PaymentTerm::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored payment term {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::PaymentTermService;

    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_payment_terms_with_their_trail() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresPaymentTermRepository::new(pool);
        contract::stores_payment_terms_with_their_trail(&repository, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn edits_payment_terms_and_records_only_changes() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = PaymentTermService::new(PostgresPaymentTermRepository::new(pool));
        contract::edits_payment_terms_and_records_only_changes(&service, company, other_company)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn filters_and_sorts_payment_terms() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = PaymentTermService::new(PostgresPaymentTermRepository::new(pool));
        contract::filters_and_sorts_payment_terms(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_one_row_can_show() {
        let pool = contract::owner_pool().await;
        let company = contract::create_company(&pool).await;
        let term_id = Uuid::new_v4();
        sqlx::query("INSERT INTO payment_terms (id, company_id, name, active) VALUES ($1, $2, 'Base', true)")
            .bind(term_id)
            .bind(company.as_uuid())
            .execute(&pool)
            .await
            .unwrap();
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let installment = |position: i16, days: i32, percent: &'static str, company: Uuid| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO payment_term_installments (company_id, term_id, position, days, percent) \
                     VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC))",
                )
                .bind(company)
                .bind(term_id)
                .bind(position)
                .bind(days)
                .bind(percent)
                .execute(&pool)
                .await
            }
        };
        let mine = company.as_uuid();
        let cases = [
            (
                installment(0, 0, "50", mine).await,
                "payment_term_installments_position_valid",
            ),
            (
                installment(25, 0, "50", mine).await,
                "payment_term_installments_position_valid",
            ),
            (
                installment(1, -1, "50", mine).await,
                "payment_term_installments_days_valid",
            ),
            (
                installment(1, 3651, "50", mine).await,
                "payment_term_installments_days_valid",
            ),
            (
                installment(1, 0, "0", mine).await,
                "payment_term_installments_percent_valid",
            ),
            (
                installment(1, 0, "100.01", mine).await,
                "payment_term_installments_percent_valid",
            ),
            // More than two decimals is refused, not rounded to a valid share.
            (
                installment(1, 0, "33.333", mine).await,
                "payment_term_installments_percent_valid",
            ),
            (
                installment(1, 0, "100.004", mine).await,
                "payment_term_installments_percent_valid",
            ),
            // The installment must be in its term's company.
            (
                installment(1, 0, "50", Uuid::new_v4()).await,
                "payment_term_installments_term_fk",
            ),
        ];
        for (result, constraint) in cases {
            assert_eq!(refused_by(result).as_deref(), Some(constraint));
        }
        // A valid one is stored; the same day twice, or the same position twice, is not.
        installment(1, 0, "50", mine).await.unwrap();
        assert_eq!(
            refused_by(installment(2, 0, "50", mine).await).as_deref(),
            Some("payment_term_installments_one_per_day_key")
        );
        assert_eq!(
            refused_by(installment(1, 30, "50", mine).await).as_deref(),
            Some("payment_term_installments_pkey")
        );
        // What spans the rows is not the database's: 50 and 25 are stored, though they do not add
        // up to 100. The domain refuses such a term before it gets here.
        installment(2, 30, "25", mine).await.unwrap();
    }
}
