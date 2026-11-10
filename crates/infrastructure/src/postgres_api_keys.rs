use conta_verde_application::{ApiKeyRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    Actor, ApiKey, ApiKeyId, ApiScope, AuditEntity, AuditEntry, AuditRecord, CalendarDate,
    CompanyId, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

const SELECT_KEYS: &str = "SELECT id, company_id, name, label, scopes, expires_at, revoked, \
     created_by_id, created_by_name, created_at, rotated_from, \
     to_char(last_used_on, 'YYYY-MM-DD') AS last_used_on FROM api_keys";

pub struct PostgresApiKeyRepository {
    pool: PgPool,
}

impl PostgresApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

async fn insert(
    connection: &mut PgConnection,
    key: &ApiKey,
    digest: &str,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO api_keys (id, company_id, name, label, digest, scopes, expires_at, revoked, \
             created_by_id, created_by_name, created_at, rotated_from) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(key.id().as_uuid())
    .bind(key.company_id().as_uuid())
    .bind(key.name())
    .bind(key.label())
    .bind(digest)
    .bind(key.scope_keys())
    .bind(key.expires_at())
    .bind(key.is_revoked())
    .bind(key.created_by().id())
    .bind(key.created_by().name())
    .bind(key.created_at())
    .bind(key.rotated_from().map(|id| id.as_uuid()))
    .execute(connection)
    .await
    .map_err(storage_error)?;
    Ok(())
}

/// Writes the life of the key: nothing else of a stored key changes.
async fn write_life(connection: &mut PgConnection, key: &ApiKey) -> Result<(), RepositoryError> {
    let result = sqlx::query(
        "UPDATE api_keys SET expires_at = $3, revoked = $4 WHERE id = $1 AND company_id = $2",
    )
    .bind(key.id().as_uuid())
    .bind(key.company_id().as_uuid())
    .bind(key.expires_at())
    .bind(key.is_revoked())
    .execute(connection)
    .await
    .map_err(storage_error)?;
    // Matching no row means the key is not in that company: report it, do not pretend.
    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound);
    }
    Ok(())
}

impl ApiKeyRepository for PostgresApiKeyRepository {
    async fn save(
        &self,
        key: &ApiKey,
        digest: &str,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, key)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        insert(&mut tx, key, digest).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn rotate(
        &self,
        new: &ApiKey,
        digest: &str,
        new_audit: &AuditEntry,
        old: &ApiKey,
        old_audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(new_audit, new)?;
        ensure_audit_describes(old_audit, old)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The old key first: when it is not there nothing is written, and the new key's foreign key needs it anyway.
        write_life(&mut tx, old).await?;
        insert(&mut tx, new, digest).await?;
        postgres_audit::insert(&mut tx, new_audit).await?;
        postgres_audit::insert(&mut tx, old_audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn update(&self, key: &ApiKey, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, key)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        write_life(&mut tx, key).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
    ) -> Result<Option<ApiKey>, RepositoryError> {
        let row = sqlx::query(&format!("{SELECT_KEYS} WHERE company_id = $1 AND id = $2"))
            .bind(company_id.as_uuid())
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        row.as_ref().map(key_from_row).transpose()
    }

    async fn find_by_digest(&self, digest: &str) -> Result<Option<ApiKey>, RepositoryError> {
        let row = sqlx::query(&format!("{SELECT_KEYS} WHERE digest = $1"))
            .bind(digest)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        row.as_ref().map(key_from_row).transpose()
    }

    async fn list(&self, company_id: CompanyId) -> Result<Vec<ApiKey>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_KEYS} WHERE company_id = $1 ORDER BY created_at DESC, id DESC"
        ))
        .bind(company_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(key_from_row).collect()
    }

    async fn record_use(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
        day: CalendarDate,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE api_keys SET last_used_on = CAST($3 AS DATE) WHERE id = $1 AND company_id = $2",
        )
        .bind(id.as_uuid())
        .bind(company_id.as_uuid())
        .bind(day.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::ApiKey, id.as_uuid()).await
    }
}

fn key_from_row(row: &PgRow) -> Result<ApiKey, RepositoryError> {
    let id = ApiKeyId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let scopes: Vec<String> = row.try_get("scopes").map_err(storage_error)?;
    let scopes = scopes
        .iter()
        .map(|text| ApiScope::parse(text))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            RepositoryError::Storage(format!("stored API key {id:?} is invalid: {error}"))
        })?;
    let last_used_on: Option<String> = row.try_get("last_used_on").map_err(storage_error)?;
    let rotated_from: Option<uuid::Uuid> = row.try_get("rotated_from").map_err(storage_error)?;
    Ok(ApiKey::restored(
        id,
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        row.try_get("name").map_err(storage_error)?,
        row.try_get("label").map_err(storage_error)?,
        scopes,
        row.try_get("expires_at").map_err(storage_error)?,
        row.try_get("revoked").map_err(storage_error)?,
        Actor::User {
            id: UserId::from_uuid(row.try_get("created_by_id").map_err(storage_error)?),
            name: row.try_get("created_by_name").map_err(storage_error)?,
        },
        row.try_get("created_at").map_err(storage_error)?,
        rotated_from.map(ApiKeyId::from_uuid),
        last_used_on.as_deref().and_then(CalendarDate::parse),
    ))
}

#[cfg(test)]
mod tests {
    use conta_verde_application::ApiKeyService;

    use super::*;
    use crate::{Argon2SecretService, contract, contract_integrations};

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_keys_by_the_digest_of_their_secret_and_never_the_secret() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = ApiKeyService::new(
            PostgresApiKeyRepository::new(pool),
            Argon2SecretService::new().unwrap(),
        );
        contract_integrations::keeps_api_keys_by_the_digest_of_their_secret(
            &service,
            company,
            other_company,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn a_rotation_and_a_revocation_end_a_key_and_are_in_the_trail() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = ApiKeyService::new(
            PostgresApiKeyRepository::new(pool),
            Argon2SecretService::new().unwrap(),
        );
        contract_integrations::rotates_and_revokes_api_keys(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_a_key_with_a_scope_a_key_may_not_have_or_a_bad_digest() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        for (scopes, digest) in [
            (vec!["fiscal:read"], "0".repeat(64)),
            (vec!["members:write"], "1".repeat(64)),
            (vec![], "2".repeat(64)),
            (vec!["sales:read"], "not a digest".to_owned()),
        ] {
            let outcome = sqlx::query(
                "INSERT INTO api_keys (id, company_id, name, label, digest, scopes, created_by_id, created_by_name, created_at) \
                 VALUES ($1, $2, 'k', 'cvk_abcdef', $3, $4, $1, 'Ana', 0)",
            )
            .bind(uuid::Uuid::new_v4())
            .bind(company.as_uuid())
            .bind(&digest)
            .bind(scopes.iter().map(|s| (*s).to_owned()).collect::<Vec<String>>())
            .execute(&pool)
            .await;
            assert!(outcome.is_err(), "{scopes:?} {digest}");
        }
    }
}
