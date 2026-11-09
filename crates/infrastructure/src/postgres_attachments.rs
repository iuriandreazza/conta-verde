use conta_verde_application::{AttachmentRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    Attachment, AttachmentDraft, AttachmentId, AuditEntity, AuditEntry, AuditRecord, CompanyId,
    EntryId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

const SELECT_ATTACHMENTS: &str = "SELECT id, company_id, entry_id, file_name, content_type, \
     size_bytes, removed FROM entry_attachments";

pub struct PostgresAttachmentRepository {
    pool: PgPool,
}

impl PostgresAttachmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn attachment_from_row(row: &PgRow) -> Result<Attachment, RepositoryError> {
    let id = AttachmentId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let entry_id = EntryId::from_uuid(row.try_get("entry_id").map_err(storage_error)?);
    let size: i64 = row.try_get("size_bytes").map_err(storage_error)?;
    let draft = AttachmentDraft {
        file_name: row.try_get("file_name").map_err(storage_error)?,
        content_type: row.try_get("content_type").map_err(storage_error)?,
        size: u64::try_from(size).map_err(|_| {
            RepositoryError::Storage(format!("stored attachment {id:?} has a negative size"))
        })?,
    };
    let removed: bool = row.try_get("removed").map_err(storage_error)?;
    Attachment::new(id, company_id, entry_id, draft)
        .map(|attachment| attachment.with_removed(removed))
        .map_err(|error| {
            RepositoryError::Storage(format!("stored attachment {id:?} is invalid: {error}"))
        })
}

impl AttachmentRepository for PostgresAttachmentRepository {
    async fn save(
        &self,
        attachment: &Attachment,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, attachment)?;
        let size = i64::try_from(attachment.size())
            .map_err(|_| RepositoryError::Storage("attachment size out of range".to_owned()))?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO entry_attachments \
             (id, company_id, entry_id, file_name, content_type, size_bytes, removed) \
             VALUES ($1, $2, $3, $4, $5, $6, false)",
        )
        .bind(attachment.id().as_uuid())
        .bind(attachment.company_id().as_uuid())
        .bind(attachment.entry_id().as_uuid())
        .bind(attachment.file_name())
        .bind(attachment.content_type())
        .bind(size)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn remove(
        &self,
        removed: &Attachment,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, removed)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE entry_attachments SET removed = true \
             WHERE id = $1 AND company_id = $2 AND entry_id = $3 AND NOT removed",
        )
        .bind(removed.id().as_uuid())
        .bind(removed.company_id().as_uuid())
        .bind(removed.entry_id().as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: AttachmentId,
    ) -> Result<Option<Attachment>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_ATTACHMENTS} WHERE company_id = $1 AND entry_id = $2 AND id = $3"
        ))
        .bind(company_id.as_uuid())
        .bind(entry_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(attachment_from_row).transpose()
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: AttachmentId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::Attachment,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Attachment>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_ATTACHMENTS} WHERE company_id = $1 AND entry_id = $2 AND NOT removed \
             ORDER BY seq"
        ))
        .bind(company_id.as_uuid())
        .bind(entry_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(attachment_from_row).collect()
    }

    async fn list_by_entries(
        &self,
        company_id: CompanyId,
        entry_ids: &[EntryId],
    ) -> Result<Vec<Attachment>, RepositoryError> {
        let ids: Vec<uuid::Uuid> = entry_ids.iter().map(|id| id.as_uuid()).collect();
        let rows = sqlx::query(&format!(
            "{SELECT_ATTACHMENTS} WHERE company_id = $1 AND entry_id = ANY($2) AND NOT removed \
             ORDER BY entry_id, seq"
        ))
        .bind(company_id.as_uuid())
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(attachment_from_row).collect()
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{AttachmentService, FinancialEntryService};

    use super::*;
    use crate::contract;
    use crate::{
        InMemoryBlobStore, PostgresBankAccountRepository, PostgresCategoryRepository,
        PostgresCostCenterRepository, PostgresFinancialEntryRepository, PostgresPartyRepository,
        PostgresPaymentMethodRepository, RegisterReferences,
    };

    async fn seed(pool: &PgPool, company: CompanyId) -> contract::EntrySeed {
        contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await
    }

    fn entries(
        pool: &PgPool,
    ) -> FinancialEntryService<
        PostgresFinancialEntryRepository,
        RegisterReferences<
            PostgresCategoryRepository,
            PostgresCostCenterRepository,
            PostgresPartyRepository,
            PostgresBankAccountRepository,
            PostgresPaymentMethodRepository,
            crate::PostgresPeriodLockRepository,
        >,
    > {
        FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
                crate::PostgresPeriodLockRepository::new(pool.clone()),
            ),
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn attaches_files() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let blobs = InMemoryBlobStore::default();
        let service = AttachmentService::new(
            PostgresAttachmentRepository::new(pool.clone()),
            blobs.clone(),
            PostgresFinancialEntryRepository::new(pool.clone()),
        );
        contract::attaches_files(
            &service,
            &PostgresAttachmentRepository::new(pool.clone()),
            &blobs,
            &entries(&pool),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_and_an_entry_of_another_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let entry = entries(&pool)
            .register(
                company,
                &contract::actor(),
                contract::entry_draft(&seed, "Compra", "2026-11-10", &[("2026-11-20", "10")]),
            )
            .await
            .unwrap();
        let store = PostgresAttachmentRepository::new(pool.clone());
        let attachment = |company: CompanyId, name: &str| {
            Attachment::new(
                AttachmentId::generate(),
                company,
                entry.id(),
                AttachmentDraft {
                    file_name: name.into(),
                    content_type: "application/pdf".into(),
                    size: 1,
                },
            )
            .unwrap()
        };
        let audit = |attachment: &Attachment| AuditEntry {
            company_id: attachment.company_id(),
            actor: contract::actor(),
            entity: AuditEntity::Attachment,
            entity_id: attachment.id().as_uuid(),
            action: conta_verde_domain::AuditAction::Created,
            changes: Vec::new(),
        };

        // An entry of another company is refused by the composite key, and leaves no trail.
        let foreign = attachment(other_company, "Nota.pdf");
        assert!(store.save(&foreign, &audit(&foreign)).await.is_err());
        assert!(
            store
                .history(other_company, foreign.id())
                .await
                .unwrap()
                .is_empty()
        );

        // What the domain would refuse, the table refuses too, whatever the adapter was told.
        for statement in [
            "UPDATE entry_attachments SET file_name = 'a/b.pdf'",
            "UPDATE entry_attachments SET file_name = 'a' || chr(92) || 'b.pdf'",
            "UPDATE entry_attachments SET file_name = 'a' || chr(10) || 'b.pdf'",
            "UPDATE entry_attachments SET file_name = ' a.pdf'",
            "UPDATE entry_attachments SET file_name = '..'",
            "UPDATE entry_attachments SET content_type = 'text/html'",
            "UPDATE entry_attachments SET size_bytes = 0",
            "UPDATE entry_attachments SET size_bytes = 10485761",
        ] {
            let kept = attachment(company, "Boa.pdf");
            store.save(&kept, &audit(&kept)).await.unwrap();
            let refused = sqlx::query(&format!("{statement} WHERE id = $1"))
                .bind(kept.id().as_uuid())
                .execute(&pool)
                .await;
            assert!(refused.is_err(), "{statement}");
        }
    }
}
