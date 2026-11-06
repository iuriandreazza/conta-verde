use conta_verde_application::{
    CollectionSettingsRepository, LinkTarget, PaymentEventRepository, PaymentLinkRepository,
    Recorded, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, BankAccountId, CategoryId, ChargeId, CollectionSettings,
    CompanyId, PaymentEvent, PaymentEventId, PaymentEventKind, PaymentEventOutcome, event_note,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::postgres::storage_error;
use crate::postgres_audit;

pub struct PostgresCollectionSettingsRepository {
    pool: PgPool,
}

impl PostgresCollectionSettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl CollectionSettingsRepository for PostgresCollectionSettingsRepository {
    async fn find(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<CollectionSettings>, RepositoryError> {
        let row = sqlx::query(
            "SELECT bank_account_id, fee_category_id FROM collection_settings WHERE company_id = $1",
        )
        .bind(company_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.map(|row| {
            let category: Option<Uuid> = row.try_get("fee_category_id").map_err(storage_error)?;
            Ok(CollectionSettings::new(
                company_id,
                BankAccountId::from_uuid(row.try_get("bank_account_id").map_err(storage_error)?),
                category.map(CategoryId::from_uuid),
            ))
        })
        .transpose()
    }

    async fn save(
        &self,
        settings: &CollectionSettings,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, settings)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO collection_settings (company_id, bank_account_id, fee_category_id) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (company_id) DO UPDATE \
             SET bank_account_id = EXCLUDED.bank_account_id, \
                 fee_category_id = EXCLUDED.fee_category_id",
        )
        .bind(settings.company_id().as_uuid())
        .bind(settings.bank_account_id().as_uuid())
        .bind(settings.fee_category_id().map(|id| id.as_uuid()))
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
            AuditEntity::CollectionSettings,
            company_id.as_uuid(),
        )
        .await
    }
}

pub struct PostgresPaymentEventRepository {
    pool: PgPool,
}

impl PostgresPaymentEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const SELECT_EVENTS: &str = "SELECT id, company_id, charge_id, provider, event_id, kind, outcome, note, raw FROM payment_events";

fn event_from_row(row: &PgRow) -> Result<PaymentEvent, RepositoryError> {
    let id = PaymentEventId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let invalid =
        |what: &str| RepositoryError::Storage(format!("stored payment event {id:?}: {what}"));
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let outcome: String = row.try_get("outcome").map_err(storage_error)?;
    let provider: String = row.try_get("provider").map_err(storage_error)?;
    let event_id: String = row.try_get("event_id").map_err(storage_error)?;
    let note: Option<String> = row.try_get("note").map_err(storage_error)?;
    let event = PaymentEvent::new(
        id,
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        ChargeId::from_uuid(row.try_get("charge_id").map_err(storage_error)?),
        &provider,
        &event_id,
        PaymentEventKind::parse(&kind).ok_or_else(|| invalid("kind"))?,
        row.try_get("raw").map_err(storage_error)?,
    )
    .map_err(|error| invalid(&format!("it is invalid: {error}")))?;
    Ok(event.with_outcome(
        PaymentEventOutcome::parse(&outcome).ok_or_else(|| invalid("outcome"))?,
        note.as_deref(),
    ))
}

impl PaymentEventRepository for PostgresPaymentEventRepository {
    async fn record(&self, event: &PaymentEvent) -> Result<Recorded, RepositoryError> {
        let inserted = sqlx::query(
            "INSERT INTO payment_events (id, company_id, charge_id, provider, event_id, kind, raw) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (company_id, provider, event_id) DO NOTHING",
        )
        .bind(event.id().as_uuid())
        .bind(event.company_id().as_uuid())
        .bind(event.charge_id().as_uuid())
        .bind(event.provider())
        .bind(event.event_id())
        .bind(event.kind().as_str())
        .bind(event.raw())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(Recorded::New(event.clone()));
        }
        let row = sqlx::query(&format!(
            "{SELECT_EVENTS} WHERE company_id = $1 AND provider = $2 AND event_id = $3"
        ))
        .bind(event.company_id().as_uuid())
        .bind(event.provider())
        .bind(event.event_id())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(Recorded::Existing(event_from_row(&row)?))
    }

    async fn finish(
        &self,
        company_id: CompanyId,
        id: PaymentEventId,
        outcome: PaymentEventOutcome,
        note: Option<&str>,
    ) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE payment_events SET outcome = $3, note = $4 WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .bind(outcome.as_str())
        .bind(event_note(note))
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }
}

pub struct PostgresPaymentLinkRepository {
    pool: PgPool,
}

impl PostgresPaymentLinkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl PaymentLinkRepository for PostgresPaymentLinkRepository {
    async fn save(
        &self,
        company_id: CompanyId,
        charge_id: ChargeId,
        digest: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO payment_links (digest, company_id, charge_id) VALUES ($1, $2, $3)",
        )
        .bind(digest)
        .bind(company_id.as_uuid())
        .bind(charge_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn find(&self, digest: &str) -> Result<Option<LinkTarget>, RepositoryError> {
        let row = sqlx::query("SELECT company_id, charge_id FROM payment_links WHERE digest = $1")
            .bind(digest)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        row.map(|row| {
            Ok(LinkTarget {
                company_id: CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
                charge_id: ChargeId::from_uuid(row.try_get("charge_id").map_err(storage_error)?),
            })
        })
        .transpose()
    }
}
