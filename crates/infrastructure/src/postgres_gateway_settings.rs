use conta_verde_application::{GatewaySettingsRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, GatewayKind, GatewaySecret, GatewaySettings,
    GatewaySettingsDraft, SealedSecret,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

#[derive(Clone)]
pub struct PostgresGatewaySettingsRepository {
    pool: PgPool,
}

impl PostgresGatewaySettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUMNS: &str = "version, kind, focus_token_homologation, focus_token_production, nfeio_api_key, \
     webhook_secret, nfeio_company_homologation, nfeio_company_production, nfeio_search_confirmed";

fn settings_from_row(
    company_id: CompanyId,
    row: &PgRow,
) -> Result<GatewaySettings, RepositoryError> {
    let sealed = |column: &str| -> Result<Option<SealedSecret>, RepositoryError> {
        let bytes: Option<Vec<u8>> = row.try_get(column).map_err(storage_error)?;
        bytes
            .map(|bytes| {
                SealedSecret::new(bytes)
                    .map_err(|error| RepositoryError::Storage(error.to_string()))
            })
            .transpose()
    };
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let version: i32 = row.try_get("version").map_err(storage_error)?;
    GatewaySettings::new(
        company_id,
        GatewaySettingsDraft {
            kind: GatewayKind::parse(&kind).ok_or_else(|| {
                RepositoryError::Storage("stored gateway kind is not known".to_owned())
            })?,
            focus_token_homologation: sealed("focus_token_homologation")?,
            focus_token_production: sealed("focus_token_production")?,
            nfeio_api_key: sealed("nfeio_api_key")?,
            webhook_secret: sealed("webhook_secret")?,
            nfeio_company_homologation: row
                .try_get("nfeio_company_homologation")
                .map_err(storage_error)?,
            nfeio_company_production: row
                .try_get("nfeio_company_production")
                .map_err(storage_error)?,
            nfeio_search_confirmed: row
                .try_get("nfeio_search_confirmed")
                .map_err(storage_error)?,
        },
        u32::try_from(version)
            .map_err(|_| RepositoryError::Storage("stored version".to_owned()))?,
    )
    .map_err(|error| RepositoryError::Storage(error.to_string()))
}

impl GatewaySettingsRepository for PostgresGatewaySettingsRepository {
    async fn find(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<GatewaySettings>, RepositoryError> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM gateway_settings WHERE company_id = $1"
        ))
        .bind(company_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref()
            .map(|row| settings_from_row(company_id, row))
            .transpose()
    }

    async fn save(
        &self,
        settings: &GatewaySettings,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, settings)?;
        let bytes = |secret: GatewaySecret| {
            settings
                .secret(secret)
                .map(|sealed| sealed.as_bytes().to_vec())
        };
        let version = i32::try_from(settings.version())
            .map_err(|_| RepositoryError::Storage("version".to_owned()))?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The first write inserts and finds none; a later one updates only the version it was made from, so a write from an old
        // read changes nothing (it is refused) instead of undoing what another wrote.
        let written = if version == 1 {
            sqlx::query(
                "INSERT INTO gateway_settings (company_id, version, kind, focus_token_homologation, \
                 focus_token_production, nfeio_api_key, webhook_secret, nfeio_company_homologation, \
                 nfeio_company_production, nfeio_search_confirmed) \
                 VALUES ($1, 1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (company_id) DO NOTHING",
            )
        } else {
            sqlx::query(
                "UPDATE gateway_settings SET version = version + 1, kind = $2, focus_token_homologation = $3, \
                 focus_token_production = $4, nfeio_api_key = $5, webhook_secret = $6, \
                 nfeio_company_homologation = $7, nfeio_company_production = $8, nfeio_search_confirmed = $9 \
                 WHERE company_id = $1 AND version = $10 - 1",
            )
        }
        .bind(settings.company_id().as_uuid())
        .bind(settings.kind().key())
        .bind(bytes(GatewaySecret::FocusTokenHomologation))
        .bind(bytes(GatewaySecret::FocusTokenProduction))
        .bind(bytes(GatewaySecret::NfeIoApiKey))
        .bind(bytes(GatewaySecret::WebhookSecret))
        .bind(settings.nfeio_company_homologation())
        .bind(settings.nfeio_company_production())
        .bind(settings.nfeio_search_confirmed());
        let written = if version == 1 {
            written
        } else {
            written.bind(version)
        }
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if written.rows_affected() != 1 {
            return Err(RepositoryError::GatewaySettingsChanged);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn remove(
        &self,
        company_id: CompanyId,
        version: u32,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        let version =
            i32::try_from(version).map_err(|_| RepositoryError::Storage("version".to_owned()))?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let removed =
            sqlx::query("DELETE FROM gateway_settings WHERE company_id = $1 AND version = $2")
                .bind(company_id.as_uuid())
                .bind(version)
                .execute(&mut *tx)
                .await
                .map_err(storage_error)?;
        if removed.rows_affected() != 1 {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM gateway_settings WHERE company_id = $1)",
            )
            .bind(company_id.as_uuid())
            .fetch_one(&mut *tx)
            .await
            .map_err(storage_error)?;
            return Err(if exists {
                RepositoryError::GatewaySettingsChanged
            } else {
                RepositoryError::NotFound
            });
        }
        if audit.company_id != company_id {
            return Err(RepositoryError::Storage(
                "audit entry does not describe the record being removed".to_owned(),
            ));
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(&self, company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::GatewaySettings,
            company_id.as_uuid(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RingSecretCipher, contract};

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_gateway_settings_sealed_and_audited_without_a_secret() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other = contract::create_company(&pool).await;
        contract::gateway_settings_are_sealed_audited_and_never_shown(
            &conta_verde_application::GatewaySettingsService::new(
                PostgresGatewaySettingsRepository::new(pool.clone()),
                RingSecretCipher::from_hex(contract::SECRETS_KEY).unwrap(),
            ),
            &conta_verde_application::GatewaySettingsService::new(
                PostgresGatewaySettingsRepository::new(pool.clone()),
                crate::NoSecretCipher,
            ),
            company,
            other,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn refuses_a_write_from_an_old_read() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        contract::gateway_settings_refuse_a_write_from_an_old_read(
            &PostgresGatewaySettingsRepository::new(pool.clone()),
            company,
        )
        .await;
    }

    /// The raw column holds the sealed bytes and nothing readable: what is stored is as long as the text plus 29, and has no text of it.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_raw_columns_hold_nothing_readable() {
        use conta_verde_application::{GatewaySettingsInput, GatewaySettingsService, SecretChange};
        use conta_verde_domain::{GatewayKind, SecretText};

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = GatewaySettingsService::new(
            PostgresGatewaySettingsRepository::new(pool.clone()),
            RingSecretCipher::from_hex(contract::SECRETS_KEY).unwrap(),
        );
        service
            .replace(
                company,
                &contract::actor(),
                GatewaySettingsInput {
                    kind: GatewayKind::Focus,
                    focus_token_homologation: SecretChange::Keep,
                    focus_token_production: SecretChange::Set(
                        SecretText::new("tok_prod_1234567890").unwrap(),
                    ),
                    nfeio_api_key: SecretChange::Keep,
                    webhook_secret: SecretChange::Keep,
                    nfeio_company_homologation: None,
                    nfeio_company_production: None,
                    nfeio_search_confirmed: false,
                },
            )
            .await
            .unwrap();
        let stored: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT focus_token_production FROM gateway_settings WHERE company_id = $1",
        )
        .bind(company.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        let stored = stored.expect("the production token is stored");
        assert!(!String::from_utf8_lossy(&stored).contains("tok_prod"));
        assert_eq!(stored.len(), 1 + 12 + "tok_prod_1234567890".len() + 16);
    }
}
