use conta_verde_application::{ConnectorRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    Actor, ApiKeyId, AuditEntity, AuditEntry, AuditRecord, CompanyId, Connector, ConnectorId,
    ConnectorKind, UserId, WebhookSubscriptionId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

const NAME_INDEX: &str = "connectors_name_key";

const SELECT_CONNECTORS: &str = "SELECT id, company_id, kind, name, active, api_key_id, webhook_subscription_id, \
     created_by_id, created_by_name, created_at FROM connectors";

pub struct PostgresConnectorRepository {
    pool: PgPool,
}

impl PostgresConnectorRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn connector_from_row(row: &PgRow) -> Result<Connector, RepositoryError> {
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let kind = ConnectorKind::parse(&kind)
        .ok_or_else(|| RepositoryError::Storage(format!("stored connector has kind {kind:?}")))?;
    let key: Option<uuid::Uuid> = row.try_get("api_key_id").map_err(storage_error)?;
    let subscription: Option<uuid::Uuid> = row
        .try_get("webhook_subscription_id")
        .map_err(storage_error)?;
    Ok(Connector::restored(
        ConnectorId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        kind,
        row.try_get("name").map_err(storage_error)?,
        row.try_get("active").map_err(storage_error)?,
        key.map(ApiKeyId::from_uuid),
        subscription.map(WebhookSubscriptionId::from_uuid),
        Actor::User {
            id: UserId::from_uuid(row.try_get("created_by_id").map_err(storage_error)?),
            name: row.try_get("created_by_name").map_err(storage_error)?,
        },
        row.try_get("created_at").map_err(storage_error)?,
    ))
}

impl ConnectorRepository for PostgresConnectorRepository {
    async fn save(&self, connector: &Connector, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, connector)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO connectors (id, company_id, kind, name, active, api_key_id, webhook_subscription_id, \
                 created_by_id, created_by_name, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(connector.id().as_uuid())
        .bind(connector.company_id().as_uuid())
        .bind(connector.kind().as_str())
        .bind(connector.name())
        .bind(connector.is_active())
        .bind(connector.api_key_id().map(|id| id.as_uuid()))
        .bind(connector.webhook_subscription_id().map(|id| id.as_uuid()))
        .bind(connector.created_by().id())
        .bind(connector.created_by().name())
        .bind(connector.created_at())
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(NAME_INDEX) => {
                RepositoryError::DuplicateName
            }
            _ => storage_error(error),
        })?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn switch(
        &self,
        connector: &Connector,
        was_active: bool,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, connector)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The state it was read in is part of the write: of two people turning it on at once, only one finds it off.
        let result = sqlx::query(
            "UPDATE connectors SET active = $4, api_key_id = $5, webhook_subscription_id = $6 \
             WHERE id = $1 AND company_id = $2 AND active = $3",
        )
        .bind(connector.id().as_uuid())
        .bind(connector.company_id().as_uuid())
        .bind(was_active)
        .bind(connector.is_active())
        .bind(connector.api_key_id().map(|id| id.as_uuid()))
        .bind(connector.webhook_subscription_id().map(|id| id.as_uuid()))
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM connectors WHERE id = $1 AND company_id = $2)",
            )
            .bind(connector.id().as_uuid())
            .bind(connector.company_id().as_uuid())
            .fetch_one(&mut *tx)
            .await
            .map_err(storage_error)?;
            return Err(if exists {
                RepositoryError::ConnectorChanged
            } else {
                RepositoryError::NotFound
            });
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ConnectorId,
    ) -> Result<Option<Connector>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_CONNECTORS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(connector_from_row).transpose()
    }

    async fn list(&self, company_id: CompanyId) -> Result<Vec<Connector>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_CONNECTORS} WHERE company_id = $1 ORDER BY created_at DESC, id DESC"
        ))
        .bind(company_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(connector_from_row).collect()
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ConnectorId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Connector, id.as_uuid()).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use conta_verde_application::{ApiKeyService, ConnectorService, WebhookService};

    use super::*;
    use crate::{
        Argon2SecretService, PostgresApiKeyRepository, PostgresJobQueue, PostgresWebhookRepository,
        RingSecretCipher, contract, contract_integrations,
    };

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_connectors() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let keys = Arc::new(ApiKeyService::new(
            PostgresApiKeyRepository::new(pool.clone()),
            Argon2SecretService::new().unwrap(),
        ));
        let hooks = Arc::new(WebhookService::new(
            PostgresWebhookRepository::new(pool.clone()),
            RingSecretCipher::from_hex(&"11".repeat(32)).unwrap(),
            PostgresJobQueue::new(pool.clone()),
            Argon2SecretService::new().unwrap(),
            false,
        ));
        let service = ConnectorService::new(
            PostgresConnectorRepository::new(pool.clone()),
            keys.clone(),
            hooks.clone(),
        );
        contract_integrations::runs_connectors(
            &service,
            &PostgresConnectorRepository::new(pool.clone()),
            &keys,
            &hooks,
            company,
            other_company,
        )
        .await;
    }
}
