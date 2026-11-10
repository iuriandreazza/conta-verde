use conta_verde_application::{FiscalDocumentRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AccessKey, Amount, AuditEntity, AuditEntry, AuditRecord, BlobId, CalendarDate, CompanyId,
    FiscalAuthorization, FiscalDocument, FiscalDocumentDraft, FiscalDocumentId, FiscalDocumentKind,
    FiscalDocumentStatus, FiscalEnvironment, FiscalFile, FiscalFileKind, FiscalOrigin,
    FiscalRefusal, SaleId, TaxRegime, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

const ONE_LIVE_PER_SALE: &str = "fiscal_documents_one_live_per_sale_key";
const ONE_FILE_PER_KIND: &str = "fiscal_files_one_per_kind_key";

#[derive(Clone)]
pub struct PostgresFiscalDocumentRepository {
    pool: PgPool,
}

impl PostgresFiscalDocumentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUMNS: &str = "id, company_id, kind, sale_id, issuer_cnpj, issuer_name, regime, environment, \
     recipient_name, recipient_document, nature_code, total::text AS total, issued_at, \
     issued_on::text AS issued_on, submitted_by_user_id, submitted_by_name, status, refusal_code, \
     refusal_message, access_key, number, series, protocol, note";

fn stored<T>(value: Option<T>, what: &str) -> Result<T, RepositoryError> {
    value.ok_or_else(|| RepositoryError::Storage(format!("stored {what} is not known")))
}

fn document_from_row(row: &PgRow) -> Result<FiscalDocument, RepositoryError> {
    let text = |name: &str| -> Result<String, RepositoryError> {
        row.try_get(name).map_err(storage_error)
    };
    let kind = stored(
        FiscalDocumentKind::parse(&text("kind")?),
        "fiscal document kind",
    )?;
    debug_assert_eq!(kind, FiscalDocumentKind::Nfe);
    let draft = FiscalDocumentDraft {
        origin: FiscalOrigin::Sale(SaleId::from_uuid(
            row.try_get("sale_id").map_err(storage_error)?,
        )),
        issuer_cnpj: text("issuer_cnpj")?,
        issuer_name: text("issuer_name")?,
        regime: stored(TaxRegime::parse(&text("regime")?), "regime")?,
        environment: stored(
            FiscalEnvironment::parse(&text("environment")?),
            "environment",
        )?,
        recipient_name: text("recipient_name")?,
        recipient_document: row.try_get("recipient_document").map_err(storage_error)?,
        nature_code: text("nature_code")?,
        total: stored(Amount::parse(&text("total")?), "total")?,
        issued_at_unix: row.try_get("issued_at").map_err(storage_error)?,
        issued_on: stored(CalendarDate::parse(&text("issued_on")?), "issue day")?,
        submitted_by: UserId::from_uuid(
            row.try_get("submitted_by_user_id").map_err(storage_error)?,
        ),
        submitted_by_name: text("submitted_by_name")?,
    };
    let status = stored(
        FiscalDocumentStatus::parse(&text("status")?),
        "fiscal document status",
    )?;
    let refusal = match (
        row.try_get::<Option<String>, _>("refusal_code")
            .map_err(storage_error)?,
        row.try_get::<Option<String>, _>("refusal_message")
            .map_err(storage_error)?,
    ) {
        (Some(code), Some(message)) => Some(FiscalRefusal::new(&code, &message)),
        _ => None,
    };
    let authorization = match row
        .try_get::<Option<String>, _>("access_key")
        .map_err(storage_error)?
    {
        Some(key) => {
            let number: i64 = row.try_get("number").map_err(storage_error)?;
            let series: i16 = row.try_get("series").map_err(storage_error)?;
            let protocol: Option<String> = row.try_get("protocol").map_err(storage_error)?;
            Some(
                FiscalAuthorization::new(
                    AccessKey::parse(&key)
                        .map_err(|error| RepositoryError::Storage(error.to_string()))?,
                    u32::try_from(number)
                        .map_err(|_| RepositoryError::Storage("stored number".to_owned()))?,
                    u16::try_from(series)
                        .map_err(|_| RepositoryError::Storage("stored series".to_owned()))?,
                    protocol.as_deref(),
                )
                .map_err(|error| RepositoryError::Storage(error.to_string()))?,
            )
        }
        None => None,
    };
    let note: Option<String> = row.try_get("note").map_err(storage_error)?;
    Ok(FiscalDocument::new(
        FiscalDocumentId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        draft,
    )
    .map_err(|error| RepositoryError::Storage(error.to_string()))?
    .restored(status, refusal, authorization, note))
}

fn file_from_row(row: &PgRow) -> Result<FiscalFile, RepositoryError> {
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let size: i64 = row.try_get("size").map_err(storage_error)?;
    FiscalFile::new(
        BlobId::from_uuid(row.try_get("blob_id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        FiscalDocumentId::from_uuid(row.try_get("document_id").map_err(storage_error)?),
        stored(FiscalFileKind::parse(&kind), "fiscal file kind")?,
        u64::try_from(size).map_err(|_| RepositoryError::Storage("stored size".to_owned()))?,
    )
    .map_err(|error| RepositoryError::Storage(error.to_string()))
}

fn bind_values<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    document: &'q FiscalDocument,
) -> Result<sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>, RepositoryError> {
    let FiscalOrigin::Sale(sale) = document.origin();
    let authorization = document.authorization();
    let number = authorization.map(|auth| i64::from(auth.number()));
    let series = authorization
        .map(|auth| i16::try_from(auth.series()))
        .transpose()
        .map_err(|_| RepositoryError::Storage("series".to_owned()))?;
    Ok(query
        .bind(document.id().as_uuid())
        .bind(document.company_id().as_uuid())
        .bind(document.kind().key())
        .bind(sale.as_uuid())
        .bind(document.issuer_cnpj())
        .bind(document.issuer_name())
        .bind(document.regime().key())
        .bind(document.environment().key())
        .bind(document.recipient_name())
        .bind(document.recipient_document())
        .bind(document.nature_code())
        .bind(document.total().as_decimal().to_string())
        .bind(document.issued_at_unix())
        .bind(document.issued_on().to_string())
        .bind(document.submitted_by().as_uuid())
        .bind(document.submitted_by_name())
        .bind(document.status().key())
        .bind(document.refusal().map(|refusal| refusal.code().to_owned()))
        .bind(
            document
                .refusal()
                .map(|refusal| refusal.message().to_owned()),
        )
        .bind(authorization.map(|auth| auth.access_key().as_str().to_owned()))
        .bind(number)
        .bind(series)
        .bind(authorization.and_then(|auth| auth.protocol().map(str::to_owned)))
        .bind(document.note()))
}

impl FiscalDocumentRepository for PostgresFiscalDocumentRepository {
    async fn create(
        &self,
        document: &FiscalDocument,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, document)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        bind_values(
            sqlx::query(
                "INSERT INTO fiscal_documents (id, company_id, kind, sale_id, issuer_cnpj, issuer_name, \
                 regime, environment, recipient_name, recipient_document, nature_code, total, issued_at, \
                 issued_on, submitted_by_user_id, submitted_by_name, status, refusal_code, refusal_message, \
                 access_key, number, series, protocol, note) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, CAST($12 AS numeric), $13, \
                 CAST($14 AS date), $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)",
            ),
            document,
        )?
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(ONE_LIVE_PER_SALE) => {
                RepositoryError::FiscalDocumentAlreadyLive
            }
            _ => storage_error(error),
        })?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn replace(
        &self,
        document: &FiscalDocument,
        expected: FiscalDocumentStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, document)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The row is locked while it is read, so the status asked about is the one the write replaces.
        let current: Option<String> = sqlx::query_scalar(
            "SELECT status FROM fiscal_documents WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(document.id().as_uuid())
        .bind(document.company_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        match current {
            None => return Err(RepositoryError::NotFound),
            Some(status) if status != expected.key() => {
                return Err(RepositoryError::FiscalDocumentChanged);
            }
            Some(_) => {}
        }
        bind_values(
            sqlx::query(
                "UPDATE fiscal_documents SET company_id = $2, kind = $3, sale_id = $4, issuer_cnpj = $5, \
                 issuer_name = $6, regime = $7, environment = $8, recipient_name = $9, \
                 recipient_document = $10, nature_code = $11, total = CAST($12 AS numeric), \
                 issued_at = $13, issued_on = CAST($14 AS date), submitted_by_user_id = $15, \
                 submitted_by_name = $16, status = $17, refusal_code = $18, refusal_message = $19, \
                 access_key = $20, number = $21, series = $22, protocol = $23, note = $24 \
                 WHERE id = $1",
            ),
            document,
        )?
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(ONE_LIVE_PER_SALE) => {
                RepositoryError::FiscalDocumentAlreadyLive
            }
            _ => storage_error(error),
        })?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Option<FiscalDocument>, RepositoryError> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM fiscal_documents WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(document_from_row).transpose()
    }

    async fn find_live(
        &self,
        company_id: CompanyId,
        origin: FiscalOrigin,
        kind: FiscalDocumentKind,
    ) -> Result<Option<FiscalDocument>, RepositoryError> {
        let FiscalOrigin::Sale(sale) = origin;
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM fiscal_documents WHERE company_id = $1 AND sale_id = $2 AND kind = $3 \
             AND status IN ('draft', 'processing', 'authorized', 'contingency')"
        ))
        .bind(company_id.as_uuid())
        .bind(sale.as_uuid())
        .bind(kind.key())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(document_from_row).transpose()
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::FiscalDocument,
            id.as_uuid(),
        )
        .await
    }

    async fn save_file(&self, file: &FiscalFile) -> Result<(), RepositoryError> {
        let size = i64::try_from(file.size())
            .map_err(|_| RepositoryError::Storage("file size".to_owned()))?;
        let known: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM fiscal_documents WHERE company_id = $1 AND id = $2")
                .bind(file.company_id().as_uuid())
                .bind(file.document_id().as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage_error)?;
        if known.is_none() {
            return Err(RepositoryError::NotFound);
        }
        sqlx::query(
            "INSERT INTO fiscal_files (blob_id, company_id, document_id, kind, size) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(file.blob_id().as_uuid())
        .bind(file.company_id().as_uuid())
        .bind(file.document_id().as_uuid())
        .bind(file.kind().key())
        .bind(size)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(ONE_FILE_PER_KIND) => {
                RepositoryError::DuplicateFiscalFile
            }
            _ => storage_error(error),
        })
    }

    async fn files_of(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<FiscalFile>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT blob_id, company_id, document_id, kind, size FROM fiscal_files \
             WHERE company_id = $1 AND document_id = $2 ORDER BY kind",
        )
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(file_from_row).collect()
    }

    async fn find_file(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
        kind: FiscalFileKind,
    ) -> Result<Option<FiscalFile>, RepositoryError> {
        let row = sqlx::query(
            "SELECT blob_id, company_id, document_id, kind, size FROM fiscal_files \
             WHERE company_id = $1 AND document_id = $2 AND kind = $3",
        )
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .bind(kind.key())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(file_from_row).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresPartyRepository, PostgresPaymentMethodRepository, PostgresPaymentTermRepository,
        PostgresProductRepository, PostgresSaleRepository, contract,
    };

    async fn seeds(
        pool: &PgPool,
        company: CompanyId,
    ) -> (contract::EntrySeed, contract::SalesSeed) {
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
        (entry_seed, seed)
    }

    async fn a_sale(pool: &PgPool, company: CompanyId) -> SaleId {
        let (entry_seed, seed) = seeds(pool, company).await;
        contract::save_a_sale(
            &PostgresSaleRepository::new(pool.clone()),
            company,
            &entry_seed,
            &seed,
        )
        .await
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_fiscal_documents() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let sale = a_sale(&pool, company).await;
        let other_sale = a_sale(&pool, other_company).await;
        contract::fiscal_documents_are_kept(
            &PostgresFiscalDocumentRepository::new(pool.clone()),
            company,
            other_company,
            sale,
            other_sale,
        )
        .await;
    }

    /// What the migration refuses, as the domain would: the database is the second barrier, and the trigger keeps
    /// the record whatever the application does.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_keeps_the_record_of_an_authorized_document() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let sale = a_sale(&pool, company).await;
        let repository = PostgresFiscalDocumentRepository::new(pool.clone());
        let document =
            contract::fiscal_document_ready_to_authorize(&repository, company, sale).await;
        let refused = |result: Result<_, sqlx::Error>, text: &str| match result {
            Err(sqlx::Error::Database(error)) => {
                assert!(
                    error.message().contains(text)
                        || error.constraint().is_some_and(|name| name.contains(text)),
                    "{text} was expected, got {}",
                    error.message()
                );
            }
            other => panic!("{text} was expected, got {other:?}"),
        };
        refused(
            sqlx::query("UPDATE fiscal_documents SET total = 1 WHERE id = $1")
                .bind(document.as_uuid())
                .execute(&pool)
                .await,
            "never changes",
        );
        refused(
            sqlx::query("DELETE FROM fiscal_documents WHERE id = $1")
                .bind(document.as_uuid())
                .execute(&pool)
                .await,
            "never deleted",
        );
        refused(
            sqlx::query("UPDATE fiscal_documents SET status = 'processing' WHERE id = $1")
                .bind(document.as_uuid())
                .execute(&pool)
                .await,
            "authorization_follows_status",
        );
        refused(
            sqlx::query("UPDATE fiscal_documents SET access_key = '123' WHERE id = $1")
                .bind(document.as_uuid())
                .execute(&pool)
                .await,
            "never changes",
        );
        sqlx::query(
            "INSERT INTO fiscal_files (blob_id, company_id, document_id, kind, size) \
             VALUES (gen_random_uuid(), $1, $2, 'xml', 10)",
        )
        .bind(company.as_uuid())
        .bind(document.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
        refused(
            sqlx::query("DELETE FROM fiscal_files WHERE document_id = $1")
                .bind(document.as_uuid())
                .execute(&pool)
                .await,
            "never changed or deleted",
        );
        refused(
            sqlx::query("INSERT INTO fiscal_files (blob_id, company_id, document_id, kind, size) VALUES (gen_random_uuid(), $1, $2, 'zip', 10)")
                .bind(company.as_uuid())
                .bind(document.as_uuid())
                .execute(&pool)
                .await,
            "kind_valid",
        );
    }

    /// An access key names one document of a company.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn an_access_key_names_one_document() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let sales = PostgresSaleRepository::new(pool.clone());
        let first = contract::save_a_sale(&sales, company, &entry_seed, &seed).await;
        let second = contract::save_a_sale(&sales, company, &entry_seed, &seed).await;
        let repository = PostgresFiscalDocumentRepository::new(pool.clone());
        contract::fiscal_document_ready_to_authorize(&repository, company, first).await;
        let error = contract::authorize_with_the_same_key(&repository, company, second).await;
        assert!(matches!(error, RepositoryError::Storage(_)), "{error:?}");
    }
}
