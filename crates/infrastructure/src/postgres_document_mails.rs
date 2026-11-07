use conta_verde_application::{DocumentMailRepository, RepositoryError, Reservation};
use conta_verde_domain::{
    CompanyId, DocumentMail, DocumentMailId, DocumentMailKind, Email, UserId,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::postgres::storage_error;

/// Keeps the log of e-mailed documents in Postgres ([ADR 0064]). The limits are checked and the row written in one
/// transaction that holds a lock on the company, so two sends racing for the last place cannot both have it.
#[derive(Clone)]
pub struct PostgresDocumentMailRepository {
    pool: PgPool,
}

impl PostgresDocumentMailRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl DocumentMailRepository for PostgresDocumentMailRepository {
    async fn reserve(
        &self,
        mail: &DocumentMail,
        cooldown_millis: i64,
        day_millis: i64,
        cap: u32,
    ) -> Result<Reservation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // One company's sends are counted one at a time; another company's are not held up.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(mail.company_id.as_uuid().to_string())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        let too_soon: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM document_mails WHERE company_id = $1 AND kind = $2 \
               AND document_id = $3 AND to_address = $4 AND sent_at > $5 - $6)",
        )
        .bind(mail.company_id.as_uuid())
        .bind(mail.kind.as_str())
        .bind(mail.document_id)
        .bind(mail.to.as_str())
        .bind(mail.sent_at_millis)
        .bind(cooldown_millis)
        .fetch_one(&mut *tx)
        .await
        .map_err(storage_error)?;
        if too_soon {
            return Ok(Reservation::TooSoon);
        }
        let today: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM document_mails WHERE company_id = $1 AND sent_at > $2 - $3",
        )
        .bind(mail.company_id.as_uuid())
        .bind(mail.sent_at_millis)
        .bind(day_millis)
        .fetch_one(&mut *tx)
        .await
        .map_err(storage_error)?;
        if today >= i64::from(cap) {
            return Ok(Reservation::CapReached);
        }
        sqlx::query(
            "INSERT INTO document_mails (id, company_id, kind, document_id, to_address, sent_by, sent_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(mail.id.as_uuid())
        .bind(mail.company_id.as_uuid())
        .bind(mail.kind.as_str())
        .bind(mail.document_id)
        .bind(mail.to.as_str())
        .bind(mail.sent_by.as_uuid())
        .bind(mail.sent_at_millis)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(Reservation::Reserved)
    }

    async fn release(
        &self,
        company_id: CompanyId,
        id: DocumentMailId,
    ) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM document_mails WHERE company_id = $1 AND id = $2")
            .bind(company_id.as_uuid())
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn of_document(
        &self,
        company_id: CompanyId,
        kind: DocumentMailKind,
        document_id: Uuid,
        limit: usize,
    ) -> Result<Vec<DocumentMail>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, to_address, sent_by, sent_at FROM document_mails \
             WHERE company_id = $1 AND kind = $2 AND document_id = $3 \
             ORDER BY sent_at DESC, id DESC LIMIT $4",
        )
        .bind(company_id.as_uuid())
        .bind(kind.as_str())
        .bind(document_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter()
            .map(|row| {
                let to: String = row.get("to_address");
                Ok(DocumentMail {
                    id: DocumentMailId::from_uuid(row.get("id")),
                    company_id,
                    kind,
                    document_id,
                    to: Email::parse(&to).map_err(|error| {
                        RepositoryError::Storage(format!("stored mail to {to:?}: {error}"))
                    })?,
                    sent_by: UserId::from_uuid(row.get("sent_by")),
                    sent_at_millis: row.get("sent_at"),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn limits_and_logs_the_documents_that_are_mailed() {
        let pool = contract::test_pool().await;
        let (company, other_company) = (
            contract::create_company(&pool).await,
            contract::create_company(&pool).await,
        );
        let user = contract::create_user(&pool).await;
        contract::limits_and_logs_the_documents_that_are_mailed(
            &PostgresDocumentMailRepository::new(pool),
            company,
            other_company,
            user,
        )
        .await;
    }
}
