use conta_verde_application::{
    FiscalDocumentCursor, FiscalDocumentQuery, FiscalDocumentRepository, ReceivedInvoiceCursor,
    ReceivedInvoiceQuery, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AccessKey, Amount, AuditEntity, AuditEntry, AuditRecord, BlobId, CalendarDate, CompanyId, Dps,
    FiscalAuthorization, FiscalDocument, FiscalDocumentDraft, FiscalDocumentId, FiscalDocumentKind,
    FiscalDocumentStatus, FiscalEnvironment, FiscalEvent, FiscalEventId, FiscalEventStatus,
    FiscalEventSubject, FiscalFile, FiscalFileKind, FiscalOrigin, FiscalRefusal, Manifestation,
    NfseAuthorization, ReceivedInvoice, ReceivedInvoiceDraft, ReceivedInvoiceId, ReceivedSituation,
    SaleId, TaxRegime, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

const ONE_LIVE_PER_SALE: &str = "fiscal_documents_one_live_per_sale_key";
const ONE_DOCUMENT_PER_DPS: &str = "fiscal_documents_dps_key";
const ONE_FILE_PER_KIND: &str = "fiscal_files_one_per_kind_key";
const ONE_LETTER_PER_PLACE: &str = "fiscal_events_sequence_key";
const ONE_OPEN_CANCELLATION: &str = "fiscal_events_one_open_cancellation_key";

const EVENT_COLUMNS: &str = "id, company_id, kind, document_id, sequence, body, series, first_number, last_number, \
     environment, status, protocol, refusal_code, refusal_message, blob_id, xml_size, requested_by_user_id, \
     requested_by_name";

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
     refusal_message, access_key, number, series, protocol, note, gateway_reference, nfse_number, nfse_key, \
     nfse_verification_code, dps_series, dps_number";

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
    let storage =
        |error: conta_verde_domain::DomainError| RepositoryError::Storage(error.to_string());
    let nfse = match row
        .try_get::<Option<i64>, _>("nfse_number")
        .map_err(storage_error)?
    {
        Some(number) => {
            let key: Option<String> = row.try_get("nfse_key").map_err(storage_error)?;
            let code: Option<String> = row
                .try_get("nfse_verification_code")
                .map_err(storage_error)?;
            let protocol: Option<String> = row.try_get("protocol").map_err(storage_error)?;
            Some(
                NfseAuthorization::new(
                    u64::try_from(number)
                        .map_err(|_| RepositoryError::Storage("stored number".to_owned()))?,
                    key.as_deref(),
                    code.as_deref(),
                    protocol.as_deref(),
                )
                .map_err(storage)?,
            )
        }
        None => None,
    };
    let dps = match (
        row.try_get::<Option<i32>, _>("dps_series")
            .map_err(storage_error)?,
        row.try_get::<Option<i64>, _>("dps_number")
            .map_err(storage_error)?,
    ) {
        (Some(series), Some(number)) => Some(
            Dps::new(
                u16::try_from(series)
                    .map_err(|_| RepositoryError::Storage("stored series".to_owned()))?,
                u64::try_from(number)
                    .map_err(|_| RepositoryError::Storage("stored number".to_owned()))?,
            )
            .map_err(storage)?,
        ),
        _ => None,
    };
    let note: Option<String> = row.try_get("note").map_err(storage_error)?;
    Ok(FiscalDocument::new_of_kind(
        kind,
        FiscalDocumentId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        draft,
    )
    .map_err(storage)?
    .restored(status, refusal, authorization, note)
    .with_nfse(nfse, dps)
    .with_gateway_reference(row.try_get("gateway_reference").map_err(storage_error)?))
}

const RECEIVED_COLUMNS: &str = "id, company_id, access_key, issuer_name, issuer_document, total::text AS total, \
     issued_on::text AS issued_on, situation, manifestation, manifestation_reason, gateway_version";

fn received_from_row(row: &PgRow) -> Result<ReceivedInvoice, RepositoryError> {
    let text = |name: &str| -> Result<String, RepositoryError> {
        row.try_get(name).map_err(storage_error)
    };
    let storage =
        |error: conta_verde_domain::DomainError| RepositoryError::Storage(error.to_string());
    let version: i64 = row.try_get("gateway_version").map_err(storage_error)?;
    let draft = ReceivedInvoiceDraft {
        access_key: AccessKey::parse(&text("access_key")?).map_err(storage)?,
        issuer_name: text("issuer_name")?,
        issuer_document: text("issuer_document")?,
        total: stored(Amount::parse(&text("total")?), "total")?,
        issued_on: stored(CalendarDate::parse(&text("issued_on")?), "issue day")?,
        situation: stored(ReceivedSituation::parse(&text("situation")?), "situation")?,
        manifestation: stored(
            Manifestation::parse(&text("manifestation")?),
            "manifestation",
        )?,
        version: u64::try_from(version)
            .map_err(|_| RepositoryError::Storage("stored version".to_owned()))?,
    };
    Ok(ReceivedInvoice::new(
        ReceivedInvoiceId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        draft,
    )
    .map_err(storage)?
    .restored(row.try_get("manifestation_reason").map_err(storage_error)?))
}

fn event_from_row(row: &PgRow) -> Result<FiscalEvent, RepositoryError> {
    let text = |name: &str| -> Result<String, RepositoryError> {
        row.try_get(name).map_err(storage_error)
    };
    let storage =
        |error: conta_verde_domain::DomainError| RepositoryError::Storage(error.to_string());
    let subject = match text("kind")?.as_str() {
        "correction" => {
            let sequence: i16 = row.try_get("sequence").map_err(storage_error)?;
            FiscalEventSubject::correction(
                FiscalDocumentId::from_uuid(row.try_get("document_id").map_err(storage_error)?),
                u8::try_from(sequence)
                    .map_err(|_| RepositoryError::Storage("sequence".to_owned()))?,
                &text("body")?,
            )
            .map_err(storage)?
        }
        "cancellation" => FiscalEventSubject::cancellation(
            FiscalDocumentId::from_uuid(row.try_get("document_id").map_err(storage_error)?),
            &text("body")?,
        )
        .map_err(storage)?,
        "invalidation" => {
            let series: i16 = row.try_get("series").map_err(storage_error)?;
            let first: i64 = row.try_get("first_number").map_err(storage_error)?;
            let last: i64 = row.try_get("last_number").map_err(storage_error)?;
            FiscalEventSubject::invalidation(
                u16::try_from(series).map_err(|_| RepositoryError::Storage("series".to_owned()))?,
                u32::try_from(first).map_err(|_| RepositoryError::Storage("first".to_owned()))?,
                u32::try_from(last).map_err(|_| RepositoryError::Storage("last".to_owned()))?,
                stored(
                    FiscalEnvironment::parse(&text("environment")?),
                    "environment",
                )?,
                &text("body")?,
            )
            .map_err(storage)?
        }
        _ => {
            return Err(RepositoryError::Storage(
                "stored event kind is not known".to_owned(),
            ));
        }
    };
    let refusal = match (
        row.try_get::<Option<String>, _>("refusal_code")
            .map_err(storage_error)?,
        row.try_get::<Option<String>, _>("refusal_message")
            .map_err(storage_error)?,
    ) {
        (Some(code), Some(message)) => Some(FiscalRefusal::new(&code, &message)),
        _ => None,
    };
    let xml = match (
        row.try_get::<Option<uuid::Uuid>, _>("blob_id")
            .map_err(storage_error)?,
        row.try_get::<Option<i64>, _>("xml_size")
            .map_err(storage_error)?,
    ) {
        (Some(blob), Some(size)) => Some((
            BlobId::from_uuid(blob),
            u64::try_from(size).map_err(|_| RepositoryError::Storage("xml size".to_owned()))?,
        )),
        _ => None,
    };
    Ok(FiscalEvent::new(
        FiscalEventId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        subject,
        UserId::from_uuid(row.try_get("requested_by_user_id").map_err(storage_error)?),
        &text("requested_by_name")?,
    )
    .map_err(storage)?
    .restored(
        stored(FiscalEventStatus::parse(&text("status")?), "event status")?,
        row.try_get("protocol").map_err(storage_error)?,
        refusal,
        xml,
    ))
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
    let nfse = document.nfse_authorization();
    let dps = document.dps();
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
        .bind(
            authorization
                .and_then(|auth| auth.protocol())
                .or_else(|| nfse.and_then(|nfse| nfse.protocol()))
                .map(str::to_owned),
        )
        .bind(document.note())
        .bind(document.gateway_reference())
        .bind(
            nfse.map(|nfse| i64::try_from(nfse.number()))
                .transpose()
                .map_err(|_| RepositoryError::Storage("number".to_owned()))?,
        )
        .bind(nfse.and_then(|nfse| nfse.key().map(str::to_owned)))
        .bind(nfse.and_then(|nfse| nfse.verification_code().map(str::to_owned)))
        .bind(dps.map(|dps| i32::from(dps.series())))
        .bind(
            dps.map(|dps| i64::try_from(dps.number()))
                .transpose()
                .map_err(|_| RepositoryError::Storage("number".to_owned()))?,
        ))
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
                 access_key, number, series, protocol, note, gateway_reference, nfse_number, nfse_key, \
                 nfse_verification_code, dps_series, dps_number) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, CAST($12 AS numeric), $13, \
                 CAST($14 AS date), $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, \
                 $28, $29, $30)",
            ),
            document,
        )?
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(ONE_LIVE_PER_SALE) => {
                RepositoryError::FiscalDocumentAlreadyLive
            }
            sqlx::Error::Database(db) if db.constraint() == Some(ONE_DOCUMENT_PER_DPS) => {
                RepositoryError::DuplicateFiscalDps
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
                 access_key = $20, number = $21, series = $22, protocol = $23, note = $24, \
                 gateway_reference = $25, nfse_number = $26, nfse_key = $27, nfse_verification_code = $28, \
                 dps_series = $29, dps_number = $30 \
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
            sqlx::Error::Database(db) if db.constraint() == Some(ONE_DOCUMENT_PER_DPS) => {
                RepositoryError::DuplicateFiscalDps
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

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &FiscalDocumentQuery,
        after: Option<&FiscalDocumentCursor>,
        limit: usize,
    ) -> Result<Vec<FiscalDocument>, RepositoryError> {
        // Every filter is optional: a NULL parameter does not narrow.
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM fiscal_documents WHERE company_id = $1 \
             AND ($2::text IS NULL OR status = $2) AND ($3::uuid IS NULL OR sale_id = $3) \
             AND ($4::bigint IS NULL OR (issued_at, id) < ($4, $5)) \
             ORDER BY issued_at DESC, id DESC LIMIT $6"
        ))
        .bind(company_id.as_uuid())
        .bind(query.status.map(|status| status.key()))
        .bind(query.sale.map(|sale| sale.as_uuid()))
        .bind(after.map(|cursor| cursor.issued_at_unix))
        .bind(after.map(|cursor| cursor.id.as_uuid()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(document_from_row).collect()
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

    async fn pending(
        &self,
        since_unix: i64,
        limit: usize,
    ) -> Result<Vec<FiscalDocument>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM fiscal_documents WHERE status IN ('processing', 'contingency') \
             AND issued_at >= $1 ORDER BY issued_at, id LIMIT $2"
        ))
        .bind(since_unix)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(document_from_row).collect()
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

    async fn issued_between(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
        limit: usize,
    ) -> Result<Vec<FiscalDocument>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM fiscal_documents WHERE company_id = $1 AND environment = 'production' \
             AND status IN ('authorized', 'cancelled') \
             AND issued_on BETWEEN CAST($2 AS date) AND CAST($3 AS date) \
             ORDER BY issued_at, id LIMIT $4"
        ))
        .bind(company_id.as_uuid())
        .bind(from.to_string())
        .bind(to.to_string())
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(document_from_row).collect()
    }

    async fn record_event(
        &self,
        event: &FiscalEvent,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, event)?;
        let (kind, document, sequence, series, first, last, environment) = match event.subject() {
            FiscalEventSubject::Correction {
                document, sequence, ..
            } => (
                "correction",
                Some(document.as_uuid()),
                Some(i16::from(*sequence)),
                None,
                None,
                None,
                None,
            ),
            FiscalEventSubject::Cancellation { document, .. } => (
                "cancellation",
                Some(document.as_uuid()),
                None,
                None,
                None,
                None,
                None,
            ),
            FiscalEventSubject::Invalidation {
                series,
                first,
                last,
                environment,
                ..
            } => (
                "invalidation",
                None,
                None,
                Some(
                    i16::try_from(*series)
                        .map_err(|_| RepositoryError::Storage("series".to_owned()))?,
                ),
                Some(i64::from(*first)),
                Some(i64::from(*last)),
                Some(environment.key()),
            ),
        };
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        if let Some(document) = document {
            // The document is read with its row held, so a cancellation cannot land between this look and the insert.
            let status: Option<String> = sqlx::query_scalar(
                "SELECT status FROM fiscal_documents WHERE id = $1 AND company_id = $2 FOR SHARE",
            )
            .bind(document)
            .bind(event.company_id().as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage_error)?;
            match status.as_deref() {
                None => return Err(RepositoryError::NotFound),
                Some("authorized") => {}
                Some(_) => return Err(RepositoryError::FiscalDocumentChanged),
            }
        }
        sqlx::query(
            "INSERT INTO fiscal_events (id, company_id, kind, document_id, sequence, body, series, \
             first_number, last_number, environment, status, requested_by_user_id, requested_by_name) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'sent', $11, $12)",
        )
        .bind(event.id().as_uuid())
        .bind(event.company_id().as_uuid())
        .bind(kind)
        .bind(document)
        .bind(sequence)
        .bind(event.subject().body())
        .bind(series)
        .bind(first)
        .bind(last)
        .bind(environment)
        .bind(event.requested_by().as_uuid())
        .bind(event.requested_by_name())
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db)
                if matches!(
                    db.constraint(),
                    Some(ONE_LETTER_PER_PLACE | ONE_OPEN_CANCELLATION)
                ) =>
            {
                RepositoryError::FiscalDocumentChanged
            }
            sqlx::Error::Database(db) if db.constraint() == Some("fiscal_events_document_fk") => {
                RepositoryError::NotFound
            }
            _ => storage_error(error),
        })?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn settle_event(
        &self,
        event: &FiscalEvent,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, event)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let current: Option<String> = sqlx::query_scalar(
            "SELECT status FROM fiscal_events WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(event.id().as_uuid())
        .bind(event.company_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        match current.as_deref() {
            None => return Err(RepositoryError::NotFound),
            Some("sent") => {}
            Some(_) => return Err(RepositoryError::FiscalDocumentChanged),
        }
        let xml = event.xml();
        sqlx::query(
            "UPDATE fiscal_events SET status = $2, protocol = $3, refusal_code = $4, refusal_message = $5, \
             blob_id = $6, xml_size = $7 WHERE id = $1",
        )
        .bind(event.id().as_uuid())
        .bind(event.status().key())
        .bind(event.protocol())
        .bind(event.refusal().map(|refusal| refusal.code().to_owned()))
        .bind(event.refusal().map(|refusal| refusal.message().to_owned()))
        .bind(xml.map(|(blob, _)| blob.as_uuid()))
        .bind(
            xml.map(|(_, size)| i64::try_from(size))
                .transpose()
                .map_err(|_| RepositoryError::Storage("xml size".to_owned()))?,
        )
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn event_history(
        &self,
        company_id: CompanyId,
        id: FiscalEventId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::FiscalEvent,
            id.as_uuid(),
        )
        .await
    }

    async fn find_event(
        &self,
        company_id: CompanyId,
        id: FiscalEventId,
    ) -> Result<Option<FiscalEvent>, RepositoryError> {
        let row = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM fiscal_events WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(event_from_row).transpose()
    }

    async fn corrections_of(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<FiscalEvent>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM fiscal_events WHERE company_id = $1 AND document_id = $2 \
             AND kind = 'correction' ORDER BY sequence"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(event_from_row).collect()
    }

    async fn open_cancellation(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Option<FiscalEvent>, RepositoryError> {
        let row = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM fiscal_events WHERE company_id = $1 AND document_id = $2 \
             AND kind = 'cancellation' AND status IN ('sent', 'accepted')"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(event_from_row).transpose()
    }

    async fn pending_events(&self, limit: usize) -> Result<Vec<FiscalEvent>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM fiscal_events WHERE status = 'sent' \
             AND kind IN ('correction', 'cancellation') ORDER BY recorded LIMIT $1"
        ))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(event_from_row).collect()
    }

    async fn invalidations(
        &self,
        company_id: CompanyId,
        limit: usize,
    ) -> Result<Vec<FiscalEvent>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM fiscal_events WHERE company_id = $1 AND kind = 'invalidation' \
             ORDER BY recorded DESC LIMIT $2"
        ))
        .bind(company_id.as_uuid())
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(event_from_row).collect()
    }

    async fn number_is_used(
        &self,
        company_id: CompanyId,
        environment: FiscalEnvironment,
        series: u16,
        first: u32,
        last: u32,
    ) -> Result<bool, RepositoryError> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM fiscal_documents WHERE company_id = $1 AND environment = $2 \
             AND kind = 'nfe' AND series = $3 AND number BETWEEN $4 AND $5)",
        )
        .bind(company_id.as_uuid())
        .bind(environment.key())
        .bind(i16::try_from(series).map_err(|_| RepositoryError::Storage("series".to_owned()))?)
        .bind(i64::from(first))
        .bind(i64::from(last))
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)
    }

    async fn mirror_received(
        &self,
        company_id: CompanyId,
        invoices: &[ReceivedInvoiceDraft],
    ) -> Result<usize, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let mut added = 0;
        for draft in invoices {
            let version = i64::try_from(draft.version)
                .map_err(|_| RepositoryError::Storage("version".to_owned()))?;
            // A new one is added; one the company has is brought up to what the gateway says. The reason the company gave
            // stays only with the word it gave.
            let inserted: bool = sqlx::query_scalar(
                "INSERT INTO received_invoices (id, company_id, access_key, issuer_name, issuer_document, total, \
                 issued_on, situation, manifestation, gateway_version) \
                 VALUES ($1, $2, $3, $4, $5, CAST($6 AS numeric), CAST($7 AS date), $8, $9, $10) \
                 ON CONFLICT (company_id, access_key) DO UPDATE SET issuer_name = EXCLUDED.issuer_name, \
                 issuer_document = EXCLUDED.issuer_document, total = EXCLUDED.total, issued_on = EXCLUDED.issued_on, \
                 situation = EXCLUDED.situation, \
                 manifestation_reason = CASE \
                     WHEN EXCLUDED.manifestation = received_invoices.manifestation \
                          OR (CASE EXCLUDED.manifestation WHEN 'none' THEN 0 WHEN 'awareness' THEN 1 ELSE 2 END) \
                           < (CASE received_invoices.manifestation WHEN 'none' THEN 0 WHEN 'awareness' THEN 1 ELSE 2 END) \
                     THEN received_invoices.manifestation_reason END, \
                 manifestation = CASE \
                     WHEN (CASE EXCLUDED.manifestation WHEN 'none' THEN 0 WHEN 'awareness' THEN 1 ELSE 2 END) \
                        < (CASE received_invoices.manifestation WHEN 'none' THEN 0 WHEN 'awareness' THEN 1 ELSE 2 END) \
                     THEN received_invoices.manifestation ELSE EXCLUDED.manifestation END, \
                 gateway_version = GREATEST(received_invoices.gateway_version, EXCLUDED.gateway_version) \
                 RETURNING (xmax = 0)",
            )
            .bind(ReceivedInvoiceId::generate().as_uuid())
            .bind(company_id.as_uuid())
            .bind(draft.access_key.as_str())
            .bind(draft.issuer_name.trim())
            .bind(&draft.issuer_document)
            .bind(draft.total.as_decimal().to_string())
            .bind(draft.issued_on.to_string())
            .bind(draft.situation.key())
            .bind(draft.manifestation.key())
            .bind(version)
            .fetch_one(&mut *tx)
            .await
            .map_err(storage_error)?;
            if inserted {
                added += 1;
            }
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(added)
    }

    async fn max_received_version(&self, company_id: CompanyId) -> Result<u64, RepositoryError> {
        let version: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(gateway_version) FROM received_invoices WHERE company_id = $1",
        )
        .bind(company_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        u64::try_from(version.unwrap_or(0))
            .map_err(|_| RepositoryError::Storage("stored version".to_owned()))
    }

    async fn find_received(
        &self,
        company_id: CompanyId,
        id: ReceivedInvoiceId,
    ) -> Result<Option<ReceivedInvoice>, RepositoryError> {
        let row = sqlx::query(&format!(
            "SELECT {RECEIVED_COLUMNS} FROM received_invoices WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(received_from_row).transpose()
    }

    async fn find_received_by_key(
        &self,
        company_id: CompanyId,
        key: &AccessKey,
    ) -> Result<Option<ReceivedInvoice>, RepositoryError> {
        let row = sqlx::query(&format!(
            "SELECT {RECEIVED_COLUMNS} FROM received_invoices WHERE company_id = $1 AND access_key = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(received_from_row).transpose()
    }

    async fn list_received(
        &self,
        company_id: CompanyId,
        query: &ReceivedInvoiceQuery,
        after: Option<&ReceivedInvoiceCursor>,
        limit: usize,
    ) -> Result<Vec<ReceivedInvoice>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {RECEIVED_COLUMNS} FROM received_invoices WHERE company_id = $1 \
             AND ($2::text IS NULL OR manifestation = $2) \
             AND ($3::text IS NULL OR (issued_on, id) < (CAST($3 AS date), $4)) \
             ORDER BY issued_on DESC, id DESC LIMIT $5"
        ))
        .bind(company_id.as_uuid())
        .bind(query.manifestation.map(|word| word.key()))
        .bind(after.map(|cursor| cursor.issued_on.to_string()))
        .bind(after.map(|cursor| cursor.id.as_uuid()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(received_from_row).collect()
    }

    async fn record_manifestation(
        &self,
        invoice: &ReceivedInvoice,
        expected: Manifestation,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, invoice)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The row is locked while it is read, so the word asked about is the one the declaration replaces.
        let current: Option<String> = sqlx::query_scalar(
            "SELECT manifestation FROM received_invoices WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(invoice.id().as_uuid())
        .bind(invoice.company_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        match current {
            None => return Err(RepositoryError::NotFound),
            Some(word) if word != expected.key() => {
                return Err(RepositoryError::FiscalDocumentChanged);
            }
            Some(_) => {}
        }
        sqlx::query(
            "UPDATE received_invoices SET manifestation = $2, manifestation_reason = $3 WHERE id = $1",
        )
        .bind(invoice.id().as_uuid())
        .bind(invoice.manifestation().key())
        .bind(invoice.justification())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn received_history(
        &self,
        company_id: CompanyId,
        id: ReceivedInvoiceId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::ReceivedInvoice,
            id.as_uuid(),
        )
        .await
    }

    async fn next_dps_number(
        &self,
        company_id: CompanyId,
        environment: FiscalEnvironment,
        series: u16,
    ) -> Result<u64, RepositoryError> {
        let last: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(dps_number) FROM fiscal_documents \
             WHERE company_id = $1 AND environment = $2 AND dps_series = $3",
        )
        .bind(company_id.as_uuid())
        .bind(environment.key())
        .bind(i32::from(series))
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        u64::try_from(last.unwrap_or(0) + 1)
            .map_err(|_| RepositoryError::Storage("stored number".to_owned()))
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

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_received_invoices() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        contract::received_invoices_are_kept(
            &PostgresFiscalDocumentRepository::new(pool.clone()),
            company,
            other_company,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_nfse_documents() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let sales = PostgresSaleRepository::new(pool.clone());
        let sale = contract::save_a_sale(&sales, company, &entry_seed, &seed).await;
        let others = [
            contract::save_a_sale(&sales, company, &entry_seed, &seed).await,
            contract::save_a_sale(&sales, company, &entry_seed, &seed).await,
        ];
        contract::fiscal_nfse_documents_are_kept(
            &PostgresFiscalDocumentRepository::new(pool.clone()),
            company,
            other_company,
            sale,
            others,
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
            sqlx::query("UPDATE fiscal_documents SET gateway_reference = 'other' WHERE id = $1")
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
        sqlx::query("UPDATE fiscal_documents SET status = 'cancelled' WHERE id = $1")
            .bind(document.as_uuid())
            .execute(&pool)
            .await
            .unwrap();
        refused(
            sqlx::query("UPDATE fiscal_documents SET status = 'authorized' WHERE id = $1")
                .bind(document.as_uuid())
                .execute(&pool)
                .await,
            "never moves again",
        );
        refused(
            sqlx::query("UPDATE fiscal_documents SET status = 'processing' WHERE id = $1")
                .bind(document.as_uuid())
                .execute(&pool)
                .await,
            "never moves again",
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

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_fiscal_events() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let sale = a_sale(&pool, company).await;
        contract::fiscal_events_are_kept(
            &PostgresFiscalDocumentRepository::new(pool.clone()),
            company,
            other_company,
            sale,
        )
        .await;
    }

    /// What the migration refuses of an event, whatever the application does.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_keeps_the_record_of_an_event() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let sale = a_sale(&pool, company).await;
        let refused = |result: Result<_, sqlx::Error>, text: &str| match result {
            Err(sqlx::Error::Database(error)) => assert!(
                error.message().contains(text)
                    || error.constraint().is_some_and(|name| name.contains(text)),
                "{text} was expected, got {}",
                error.message()
            ),
            other => panic!("{text} was expected, got {other:?}"),
        };
        let _ = sale;
        refused(
            sqlx::query("INSERT INTO fiscal_events (id, company_id, kind, body, series, first_number, last_number, environment, status, requested_by_user_id, requested_by_name) VALUES ($1, $2, 'invalidation', 'Numeração pulada por falha', 1, 5, 4, 'homologation', 'sent', gen_random_uuid(), 'Maria')")
                .bind(uuid::Uuid::new_v4())
                .bind(company.as_uuid())
                .execute(&pool)
                .await,
            "kind_follows_columns",
        );
        sqlx::query("INSERT INTO fiscal_events (id, company_id, kind, body, series, first_number, last_number, environment, status, requested_by_user_id, requested_by_name) VALUES ($1, $2, 'invalidation', 'Numeração pulada por falha', 1, 5, 6, 'homologation', 'sent', gen_random_uuid(), 'Maria')")
            .bind(uuid::Uuid::new_v4())
            .bind(company.as_uuid())
            .execute(&pool)
            .await
            .unwrap();
        refused(
            sqlx::query("UPDATE fiscal_events SET last_number = 9 WHERE company_id = $1")
                .bind(company.as_uuid())
                .execute(&pool)
                .await,
            "never changes",
        );
        refused(
            sqlx::query("DELETE FROM fiscal_events WHERE company_id = $1")
                .bind(company.as_uuid())
                .execute(&pool)
                .await,
            "never deleted",
        );
        sqlx::query("UPDATE fiscal_events SET status = 'accepted' WHERE company_id = $1")
            .bind(company.as_uuid())
            .execute(&pool)
            .await
            .unwrap();
        refused(
            sqlx::query("UPDATE fiscal_events SET status = 'refused', refusal_code = '1', refusal_message = 'x' WHERE company_id = $1")
                .bind(company.as_uuid())
                .execute(&pool)
                .await,
            "never changes",
        );
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn issues_an_nfe_from_a_sale() {
        use crate::{
            FakeFiscalGateway, LocalBlobStore, PostgresFiscalSettingsRepository,
            PostgresPeriodLockRepository, RegisterReferences,
        };
        use conta_verde_application::FiscalDocumentService;

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let parties = PostgresPartyRepository::new(pool.clone());
        let fiscal = contract::seed_fiscal_customer(&parties, company).await;
        let gateway = FakeFiscalGateway::authorizing_after(1);
        let settings = PostgresFiscalSettingsRepository::new(pool.clone());
        let sales = PostgresSaleRepository::new(pool.clone());
        let root =
            std::env::temp_dir().join(format!("conta-verde-fiscal-{}", uuid::Uuid::new_v4()));
        let service = FiscalDocumentService::new(
            PostgresFiscalDocumentRepository::new(pool.clone()),
            sales.clone(),
            parties,
            PostgresProductRepository::new(pool.clone()),
            settings.clone(),
            gateway.clone(),
            LocalBlobStore::new(&root),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
                PostgresPeriodLockRepository::new(pool.clone()),
            ),
        );
        contract::issues_an_nfe_from_a_sale(
            &service,
            &gateway,
            &settings,
            &sales,
            company,
            other_company,
            &entry_seed,
            &seed,
            &fiscal,
        )
        .await;
        std::fs::remove_dir_all(root).expect("the test's own directory is removed");
    }

    /// What the migration of the NFS-e refuses, as the domain would: each kind keeps what the authority gave in its own
    /// columns, a DPS belongs to an NFS-e, and the identity of an NFS-e is a number and letters and digits.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_keeps_an_nfse_to_its_own_shape() {
        use conta_verde_domain::{FiscalDocument, FiscalDocumentKind};

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let sales = PostgresSaleRepository::new(pool.clone());
        let nfse_sale = contract::save_a_sale(&sales, company, &entry_seed, &seed).await;
        let nfe_sale = contract::save_a_sale(&sales, company, &entry_seed, &seed).await;
        let repository = PostgresFiscalDocumentRepository::new(pool.clone());
        let nfse = FiscalDocument::new_of_kind(
            FiscalDocumentKind::Nfse,
            conta_verde_domain::FiscalDocumentId::generate(),
            company,
            contract::fiscal_document_draft(nfse_sale, "250.00"),
        )
        .unwrap();
        let nfe = contract::a_fiscal_document(company, nfe_sale, "10");
        for document in [&nfse, &nfe] {
            repository
                .create(
                    document,
                    &contract::fiscal_document_audit(
                        document,
                        conta_verde_domain::AuditAction::Created,
                        conta_verde_domain::fiscal_document_creation_changes(document),
                    ),
                )
                .await
                .unwrap();
        }
        let refused = |result: Result<_, sqlx::Error>, constraint: &str| match result {
            Err(sqlx::Error::Database(error)) => assert_eq!(
                error.constraint(),
                Some(constraint),
                "{constraint} was expected, got {}",
                error.message()
            ),
            other => panic!("{constraint} was expected, got {other:?}"),
        };
        let update = |sql: &'static str, id: conta_verde_domain::FiscalDocumentId| {
            let pool = pool.clone();
            async move { sqlx::query(sql).bind(id.as_uuid()).execute(&pool).await }
        };
        // A DPS is an NFS-e's, in the series an API may use.
        refused(
            update(
                "UPDATE fiscal_documents SET dps_series = 1, dps_number = 1 WHERE id = $1",
                nfe.id(),
            )
            .await,
            "fiscal_documents_authorization_follows_status",
        );
        refused(
            update(
                "UPDATE fiscal_documents SET dps_series = 50000, dps_number = 1 WHERE id = $1",
                nfse.id(),
            )
            .await,
            "fiscal_documents_dps_valid",
        );
        refused(
            update(
                "UPDATE fiscal_documents SET dps_series = 1 WHERE id = $1",
                nfse.id(),
            )
            .await,
            "fiscal_documents_dps_valid",
        );
        // An NFS-e has an NFS-e's identity and not an NF-e's; one that is authorized has its number and its DPS.
        refused(
            update(
                "UPDATE fiscal_documents SET status = 'authorized', nfse_number = 1 WHERE id = $1",
                nfse.id(),
            )
            .await,
            "fiscal_documents_authorization_follows_status",
        );
        refused(
            update("UPDATE fiscal_documents SET dps_series = 1, dps_number = 1, status = 'authorized', nfse_number = 1, number = 1 WHERE id = $1", nfse.id()).await,
            "fiscal_documents_authorization_follows_status",
        );
        refused(
            update(
                "UPDATE fiscal_documents SET nfse_number = 1 WHERE id = $1",
                nfse.id(),
            )
            .await,
            "fiscal_documents_authorization_follows_status",
        );
        refused(
            update("UPDATE fiscal_documents SET dps_series = 1, dps_number = 1, status = 'authorized', nfse_number = 0 WHERE id = $1", nfse.id()).await,
            "fiscal_documents_authorization_follows_status",
        );
        refused(
            update("UPDATE fiscal_documents SET dps_series = 1, dps_number = 1, status = 'authorized', nfse_number = 1, nfse_key = 'a/b' WHERE id = $1", nfse.id()).await,
            "fiscal_documents_nfse_identity_valid",
        );
        refused(
            update("UPDATE fiscal_documents SET dps_series = 1, dps_number = 1, status = 'authorized', nfse_number = 1, nfse_verification_code = '' WHERE id = $1", nfse.id()).await,
            "fiscal_documents_nfse_identity_valid",
        );
        // The kind is one of the two: a third fails both checks, and the database names the first by name.
        refused(
            update(
                "UPDATE fiscal_documents SET kind = 'other' WHERE id = $1",
                nfse.id(),
            )
            .await,
            "fiscal_documents_authorization_follows_status",
        );
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn issues_an_nfse_from_a_sale() {
        use crate::{
            FakeFiscalGateway, LocalBlobStore, PostgresFiscalSettingsRepository,
            PostgresPeriodLockRepository, RegisterReferences,
        };
        use conta_verde_application::FiscalDocumentService;

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let parties = PostgresPartyRepository::new(pool.clone());
        let fiscal = contract::seed_fiscal_customer(&parties, company).await;
        let gateway = FakeFiscalGateway::authorizing_after(1);
        let settings = PostgresFiscalSettingsRepository::new(pool.clone());
        let sales = PostgresSaleRepository::new(pool.clone());
        let root =
            std::env::temp_dir().join(format!("conta-verde-fiscal-{}", uuid::Uuid::new_v4()));
        let service = FiscalDocumentService::new(
            PostgresFiscalDocumentRepository::new(pool.clone()),
            sales.clone(),
            parties,
            PostgresProductRepository::new(pool.clone()),
            settings.clone(),
            gateway.clone(),
            LocalBlobStore::new(&root),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
                PostgresPeriodLockRepository::new(pool.clone()),
            ),
        );
        contract::issues_an_nfse_from_a_sale(
            &service,
            &gateway,
            &settings,
            &sales,
            company,
            other_company,
            &entry_seed,
            &seed,
            &fiscal,
        )
        .await;
        std::fs::remove_dir_all(root).expect("the test's own directory is removed");
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn syncs_and_manifests_received_invoices() {
        use crate::{
            FakeFiscalGateway, LocalBlobStore, PostgresFiscalSettingsRepository,
            PostgresPeriodLockRepository, RegisterReferences,
        };
        use conta_verde_application::FiscalDocumentService;

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let gateway = FakeFiscalGateway::default();
        let settings = PostgresFiscalSettingsRepository::new(pool.clone());
        let root =
            std::env::temp_dir().join(format!("conta-verde-fiscal-{}", uuid::Uuid::new_v4()));
        let service = FiscalDocumentService::new(
            PostgresFiscalDocumentRepository::new(pool.clone()),
            PostgresSaleRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            settings.clone(),
            gateway.clone(),
            LocalBlobStore::new(&root),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
                PostgresPeriodLockRepository::new(pool.clone()),
            ),
        );
        contract::syncs_and_manifests_received_invoices(
            &service,
            &gateway,
            &settings,
            company,
            other_company,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn issues_an_nfce_from_a_sale() {
        use crate::{
            FakeFiscalGateway, LocalBlobStore, PostgresFiscalSettingsRepository,
            PostgresPeriodLockRepository, RegisterReferences,
        };
        use conta_verde_application::FiscalDocumentService;

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let parties = PostgresPartyRepository::new(pool.clone());
        let fiscal = contract::seed_fiscal_customer(&parties, company).await;
        let gateway = FakeFiscalGateway::authorizing_after(1);
        let settings = PostgresFiscalSettingsRepository::new(pool.clone());
        let sales = PostgresSaleRepository::new(pool.clone());
        let root =
            std::env::temp_dir().join(format!("conta-verde-fiscal-{}", uuid::Uuid::new_v4()));
        let service = FiscalDocumentService::new(
            PostgresFiscalDocumentRepository::new(pool.clone()),
            sales.clone(),
            parties,
            PostgresProductRepository::new(pool.clone()),
            settings.clone(),
            gateway.clone(),
            LocalBlobStore::new(&root),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
                PostgresPeriodLockRepository::new(pool.clone()),
            ),
        );
        contract::issues_an_nfce_from_a_sale(
            &service,
            &gateway,
            &settings,
            &sales,
            company,
            other_company,
            &entry_seed,
            &seed,
            &fiscal,
        )
        .await;
        std::fs::remove_dir_all(root).expect("the test's own directory is removed");
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_regular_regimes_issue_nothing_until_their_calculation_is_signed() {
        use crate::{
            FakeFiscalGateway, LocalBlobStore, PostgresFiscalSettingsRepository,
            PostgresPeriodLockRepository, RegisterReferences,
        };
        use conta_verde_application::FiscalDocumentService;

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let parties = PostgresPartyRepository::new(pool.clone());
        let fiscal = contract::seed_fiscal_customer(&parties, company).await;
        let gateway = FakeFiscalGateway::authorizing_after(1);
        let settings = PostgresFiscalSettingsRepository::new(pool.clone());
        let sales = PostgresSaleRepository::new(pool.clone());
        let root =
            std::env::temp_dir().join(format!("conta-verde-fiscal-{}", uuid::Uuid::new_v4()));
        let service = FiscalDocumentService::new(
            PostgresFiscalDocumentRepository::new(pool.clone()),
            sales.clone(),
            parties,
            PostgresProductRepository::new(pool.clone()),
            settings.clone(),
            gateway.clone(),
            LocalBlobStore::new(&root),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
                PostgresPeriodLockRepository::new(pool.clone()),
            ),
        );
        contract::the_regular_regimes_issue_nothing_until_their_calculation_is_signed(
            &service,
            &gateway,
            &settings,
            &sales,
            company,
            &entry_seed,
            &seed,
            &fiscal,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn follows_a_gateway_that_names_and_queues() {
        use crate::{
            FakeFiscalGateway, LocalBlobStore, PostgresFiscalSettingsRepository,
            PostgresPeriodLockRepository, RegisterReferences,
        };
        use conta_verde_application::FiscalDocumentService;

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let parties = PostgresPartyRepository::new(pool.clone());
        let fiscal = contract::seed_fiscal_customer(&parties, company).await;
        let gateway = FakeFiscalGateway::default();
        let settings = PostgresFiscalSettingsRepository::new(pool.clone());
        let sales = PostgresSaleRepository::new(pool.clone());
        let root =
            std::env::temp_dir().join(format!("conta-verde-fiscal-{}", uuid::Uuid::new_v4()));
        let service = FiscalDocumentService::new(
            PostgresFiscalDocumentRepository::new(pool.clone()),
            sales.clone(),
            parties,
            PostgresProductRepository::new(pool.clone()),
            settings.clone(),
            gateway.clone(),
            LocalBlobStore::new(&root),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
                PostgresPeriodLockRepository::new(pool.clone()),
            ),
        );
        contract::follows_a_gateway_that_names_and_queues(
            &service,
            &gateway,
            &settings,
            &sales,
            company,
            &entry_seed,
            &seed,
            &fiscal,
        )
        .await;
        std::fs::remove_dir_all(root).expect("the test's own directory is removed");
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
