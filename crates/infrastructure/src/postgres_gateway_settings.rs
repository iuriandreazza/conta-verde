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

const COLUMNS: &str = "kind, focus_token_homologation, focus_token_production, nfeio_api_key, \
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
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO gateway_settings (company_id, kind, focus_token_homologation, focus_token_production, \
             nfeio_api_key, webhook_secret, nfeio_company_homologation, nfeio_company_production, \
             nfeio_search_confirmed) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (company_id) DO UPDATE SET kind = EXCLUDED.kind, \
             focus_token_homologation = EXCLUDED.focus_token_homologation, \
             focus_token_production = EXCLUDED.focus_token_production, \
             nfeio_api_key = EXCLUDED.nfeio_api_key, webhook_secret = EXCLUDED.webhook_secret, \
             nfeio_company_homologation = EXCLUDED.nfeio_company_homologation, \
             nfeio_company_production = EXCLUDED.nfeio_company_production, \
             nfeio_search_confirmed = EXCLUDED.nfeio_search_confirmed",
        )
        .bind(settings.company_id().as_uuid())
        .bind(settings.kind().key())
        .bind(bytes(GatewaySecret::FocusTokenHomologation))
        .bind(bytes(GatewaySecret::FocusTokenProduction))
        .bind(bytes(GatewaySecret::NfeIoApiKey))
        .bind(bytes(GatewaySecret::WebhookSecret))
        .bind(settings.nfeio_company_homologation())
        .bind(settings.nfeio_company_production())
        .bind(settings.nfeio_search_confirmed())
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
        // The raw columns hold nothing readable: what is stored is the sealed bytes, whose length is the text's plus 29.
        let raw: Vec<(String, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT kind, focus_token_production FROM gateway_settings WHERE company_id = $1",
        )
        .bind(company.as_uuid())
        .fetch_all(&pool)
        .await
        .unwrap();
        let stored = raw[0].1.clone().expect("the production token is stored");
        assert!(!String::from_utf8_lossy(&stored).contains("tok_prod"));
        assert_eq!(stored.len(), 1 + 12 + "tok_prod_other_567890".len() + 16);
    }
}
