use conta_verde_application::{FiscalSettingsRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AddressDraft, AuditEntity, AuditEntry, AuditRecord, CompanyId, FiscalEnvironment,
    FiscalSettings, FiscalSettingsDraft, NatureKind, OperationNatureDraft, TaxRegime, TaxRuleDraft,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::{snapshot, storage_error};
use crate::postgres_audit;

#[derive(Clone)]
pub struct PostgresFiscalSettingsRepository {
    pool: PgPool,
}

impl PostgresFiscalSettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn stored<T>(value: Option<T>, what: &str) -> Result<T, RepositoryError> {
    value.ok_or_else(|| RepositoryError::Storage(format!("stored {what} is not known")))
}

fn position(index: usize, what: &str) -> Result<i16, RepositoryError> {
    i16::try_from(index + 1).map_err(|_| RepositoryError::Storage(format!("too many {what}")))
}

fn series(value: i16, what: &str) -> Result<u16, RepositoryError> {
    u16::try_from(value).map_err(|_| RepositoryError::Storage(format!("stored {what} {value}")))
}

impl FiscalSettingsRepository for PostgresFiscalSettingsRepository {
    async fn find(&self, company_id: CompanyId) -> Result<Option<FiscalSettings>, RepositoryError> {
        // One snapshot for the three tables, so natures and rules are the ones of the settings read.
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(
            "SELECT legal_name, cnpj, state_registration, municipal_registration, postal_code, street, \
             street_number, complement, district, city, state, regime, environment, nfe_series, \
             nfce_series, nfse_series, certificate_valid_until::text AS certificate_valid_until \
             FROM fiscal_settings WHERE company_id = $1",
        )
        .bind(company_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let nature_rows = sqlx::query(
            "SELECT position, code, name, kind, cfop_in_state, cfop_interstate, active \
             FROM fiscal_natures WHERE company_id = $1 ORDER BY position",
        )
        .bind(company_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        let rule_rows = sqlx::query(
            "SELECT nature_position, regime, ncm_prefix, icms_code, pis_code, cofins_code, \
             icms_rate::text AS icms_rate, pis_rate::text AS pis_rate, cofins_rate::text AS cofins_rate \
             FROM fiscal_tax_rules WHERE company_id = $1 ORDER BY nature_position, position",
        )
        .bind(company_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;

        let mut natures = Vec::with_capacity(nature_rows.len());
        for nature in &nature_rows {
            let nature_position: i16 = nature.try_get("position").map_err(storage_error)?;
            let mut rules = Vec::new();
            for rule in &rule_rows {
                let owner: i16 = rule.try_get("nature_position").map_err(storage_error)?;
                if owner == nature_position {
                    rules.push(rule_from_row(rule)?);
                }
            }
            natures.push(nature_from_row(nature, rules)?);
        }
        let draft = FiscalSettingsDraft {
            legal_name: row.try_get("legal_name").map_err(storage_error)?,
            cnpj: row.try_get("cnpj").map_err(storage_error)?,
            state_registration: row.try_get("state_registration").map_err(storage_error)?,
            municipal_registration: row
                .try_get("municipal_registration")
                .map_err(storage_error)?,
            address: AddressDraft {
                postal_code: row.try_get("postal_code").map_err(storage_error)?,
                street: row.try_get("street").map_err(storage_error)?,
                number: row.try_get("street_number").map_err(storage_error)?,
                complement: row.try_get("complement").map_err(storage_error)?,
                district: row.try_get("district").map_err(storage_error)?,
                city: row.try_get("city").map_err(storage_error)?,
                state: row.try_get("state").map_err(storage_error)?,
            },
            regime: stored(
                TaxRegime::parse(&row.try_get::<String, _>("regime").map_err(storage_error)?),
                "regime",
            )?,
            environment: stored(
                FiscalEnvironment::parse(
                    &row.try_get::<String, _>("environment")
                        .map_err(storage_error)?,
                ),
                "environment",
            )?,
            nfe_series: series(row.try_get("nfe_series").map_err(storage_error)?, "series")?,
            nfce_series: series(row.try_get("nfce_series").map_err(storage_error)?, "series")?,
            nfse_series: series(row.try_get("nfse_series").map_err(storage_error)?, "series")?,
            certificate_valid_until: row
                .try_get("certificate_valid_until")
                .map_err(storage_error)?,
            natures,
        };
        FiscalSettings::new(company_id, draft)
            .map(Some)
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    async fn save(
        &self,
        settings: &FiscalSettings,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, settings)?;
        let company_id = settings.company_id().as_uuid();
        let address = settings.address();
        // One transaction: the settings, every nature and rule, and the audit entry commit together or not at all.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO fiscal_settings (company_id, legal_name, cnpj, state_registration, \
             municipal_registration, postal_code, street, street_number, complement, district, city, \
             state, regime, environment, nfe_series, nfce_series, nfse_series, certificate_valid_until) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
             CAST($18 AS DATE)) \
             ON CONFLICT (company_id) DO UPDATE SET legal_name = EXCLUDED.legal_name, \
             cnpj = EXCLUDED.cnpj, state_registration = EXCLUDED.state_registration, \
             municipal_registration = EXCLUDED.municipal_registration, \
             postal_code = EXCLUDED.postal_code, street = EXCLUDED.street, \
             street_number = EXCLUDED.street_number, complement = EXCLUDED.complement, \
             district = EXCLUDED.district, city = EXCLUDED.city, state = EXCLUDED.state, \
             regime = EXCLUDED.regime, environment = EXCLUDED.environment, \
             nfe_series = EXCLUDED.nfe_series, nfce_series = EXCLUDED.nfce_series, \
             nfse_series = EXCLUDED.nfse_series, \
             certificate_valid_until = EXCLUDED.certificate_valid_until",
        )
        .bind(company_id)
        .bind(settings.legal_name())
        .bind(settings.cnpj())
        .bind(settings.state_registration())
        .bind(settings.municipal_registration())
        .bind(address.postal_code())
        .bind(address.street())
        .bind(address.number())
        .bind(address.complement())
        .bind(address.district())
        .bind(address.city())
        .bind(address.state())
        .bind(settings.regime().key())
        .bind(settings.environment().key())
        .bind(i16::try_from(settings.nfe_series()).map_err(|_| RepositoryError::Storage("series".into()))?)
        .bind(i16::try_from(settings.nfce_series()).map_err(|_| RepositoryError::Storage("series".into()))?)
        .bind(i16::try_from(settings.nfse_series()).map_err(|_| RepositoryError::Storage("series".into()))?)
        .bind(settings.certificate_valid_until().map(|day| day.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        // The upsert above took the settings row's lock, so a second writer waits for this transaction and then
        // replaces what it wrote, instead of colliding with it. The children are replaced as a whole: rules first,
        // since they point at the natures.
        for table in ["fiscal_tax_rules", "fiscal_natures"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE company_id = $1"))
                .bind(company_id)
                .execute(&mut *tx)
                .await
                .map_err(storage_error)?;
        }
        for (index, nature) in settings.natures().iter().enumerate() {
            let nature_position = position(index, "natures")?;
            sqlx::query(
                "INSERT INTO fiscal_natures (company_id, position, code, name, kind, cfop_in_state, \
                 cfop_interstate, active) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(company_id)
            .bind(nature_position)
            .bind(nature.code())
            .bind(nature.name())
            .bind(nature.kind().key())
            .bind(nature.cfop_in_state())
            .bind(nature.cfop_interstate())
            .bind(nature.is_active())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
            for (rule_index, rule) in nature.rules().iter().enumerate() {
                sqlx::query(
                    "INSERT INTO fiscal_tax_rules (company_id, nature_position, position, regime, \
                     ncm_prefix, icms_code, pis_code, cofins_code, icms_rate, pis_rate, cofins_rate) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CAST($9 AS NUMERIC), CAST($10 AS NUMERIC), \
                     CAST($11 AS NUMERIC))",
                )
                .bind(company_id)
                .bind(nature_position)
                .bind(position(rule_index, "rules")?)
                .bind(rule.regime().key())
                .bind(rule.ncm_prefix())
                .bind(rule.icms_code())
                .bind(rule.pis_code())
                .bind(rule.cofins_code())
                .bind(rule.icms_rate().map(|rate| rate.to_string()))
                .bind(rule.pis_rate().map(|rate| rate.to_string()))
                .bind(rule.cofins_rate().map(|rate| rate.to_string()))
                .execute(&mut *tx)
                .await
                .map_err(storage_error)?;
            }
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(&self, company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::FiscalSettings,
            company_id.as_uuid(),
        )
        .await
    }
}

fn nature_from_row(
    row: &PgRow,
    rules: Vec<TaxRuleDraft>,
) -> Result<OperationNatureDraft, RepositoryError> {
    Ok(OperationNatureDraft {
        code: row.try_get("code").map_err(storage_error)?,
        name: row.try_get("name").map_err(storage_error)?,
        kind: stored(
            NatureKind::parse(&row.try_get::<String, _>("kind").map_err(storage_error)?),
            "nature kind",
        )?,
        cfop_in_state: row.try_get("cfop_in_state").map_err(storage_error)?,
        cfop_interstate: row.try_get("cfop_interstate").map_err(storage_error)?,
        active: row.try_get("active").map_err(storage_error)?,
        rules,
    })
}

fn rule_from_row(row: &PgRow) -> Result<TaxRuleDraft, RepositoryError> {
    Ok(TaxRuleDraft {
        regime: stored(
            TaxRegime::parse(&row.try_get::<String, _>("regime").map_err(storage_error)?),
            "rule regime",
        )?,
        ncm_prefix: row.try_get("ncm_prefix").map_err(storage_error)?,
        icms_code: row.try_get("icms_code").map_err(storage_error)?,
        pis_code: row.try_get("pis_code").map_err(storage_error)?,
        cofins_code: row.try_get("cofins_code").map_err(storage_error)?,
        icms_rate: row.try_get("icms_rate").map_err(storage_error)?,
        pis_rate: row.try_get("pis_rate").map_err(storage_error)?,
        cofins_rate: row.try_get("cofins_rate").map_err(storage_error)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_the_fiscal_settings() {
        let pool = contract::test_pool().await;
        contract::stores_the_fiscal_settings(
            &PostgresFiscalSettingsRepository::new(pool.clone()),
            contract::create_company(&pool).await,
            contract::create_company(&pool).await,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn replaces_and_audits_the_fiscal_settings() {
        let pool = contract::test_pool().await;
        contract::fiscal_settings_are_replaced_and_audited(
            &conta_verde_application::FiscalSettingsService::new(
                PostgresFiscalSettingsRepository::new(pool.clone()),
            ),
            contract::create_company(&pool).await,
        )
        .await;
    }

    /// What the migration's constraints refuse, one by one, as the domain would: the database is the second barrier.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let run = |sql: String| {
            let pool = pool.clone();
            async move {
                sqlx::query(&sql)
                    .bind(company.as_uuid())
                    .execute(&pool)
                    .await
            }
        };
        let refused = |result: Result<_, sqlx::Error>, constraint: &str| match result {
            Err(sqlx::Error::Database(error)) => {
                assert_eq!(error.constraint(), Some(constraint));
            }
            other => panic!("{constraint} was expected, got {other:?}"),
        };
        let settings = |regime: &str, environment: &str, until: &str, series: i16| {
            format!(
                "INSERT INTO fiscal_settings (company_id, legal_name, cnpj, postal_code, street, city, \
                 state, regime, environment, nfe_series, nfce_series, nfse_series, certificate_valid_until) \
                 VALUES ($1, 'Verde Ltda', '11222333000181', '01310100', 'Paulista', 'São Paulo', 'SP', \
                 '{regime}', '{environment}', {series}, 1, 1, {until})"
            )
        };
        refused(
            run(settings("mei", "homologation", "NULL", 1)).await,
            "fiscal_settings_regime_valid",
        );
        refused(
            run(settings("simples_nacional", "production", "NULL", 1)).await,
            "fiscal_settings_production_needs_certificate",
        );
        refused(
            run(settings("simples_nacional", "homologation", "NULL", 1000)).await,
            "fiscal_settings_series_valid",
        );
        run(settings(
            "simples_nacional",
            "production",
            "DATE '2027-03-31'",
            1,
        ))
        .await
        .unwrap();

        // A product nature carries its two CFOPs by where the goods go, and a service carries none.
        let nature = |kind: &str, in_state: &str, interstate: &str, code: &str| {
            format!(
                "INSERT INTO fiscal_natures (company_id, position, code, name, kind, cfop_in_state, \
                 cfop_interstate, active) \
                 VALUES ($1, 1, '{code}', 'Venda', '{kind}', {in_state}, {interstate}, true)"
            )
        };
        refused(
            run(nature("product", "'6102'", "'6102'", "sale")).await,
            "fiscal_natures_cfop_follows_kind",
        );
        refused(
            run(nature("service", "'5102'", "NULL", "service")).await,
            "fiscal_natures_cfop_follows_kind",
        );
        refused(
            run(nature("product", "'5102'", "'6102'", "Has Space")).await,
            "fiscal_natures_code_valid",
        );
        run(nature("product", "'5102'", "'6102'", "sale"))
            .await
            .unwrap();

        // The rule's ICMS code follows its regime, and a rate is a percentage with two decimals.
        let rule = |regime: &str, icms: &str, rate: &str| {
            format!(
                "INSERT INTO fiscal_tax_rules (company_id, nature_position, position, regime, icms_code, \
                 pis_code, cofins_code, icms_rate) VALUES ($1, 1, 1, '{regime}', '{icms}', '07', '07', {rate})"
            )
        };
        refused(
            run(rule("simples_nacional", "00", "NULL")).await,
            "fiscal_tax_rules_icms_code_valid",
        );
        refused(
            run(rule("lucro_presumido", "102", "NULL")).await,
            "fiscal_tax_rules_icms_code_valid",
        );
        refused(
            run(rule("lucro_presumido", "00", "100.5")).await,
            "fiscal_tax_rules_rates_valid",
        );
        refused(
            run(rule("lucro_presumido", "00", "1.234")).await,
            "fiscal_tax_rules_rates_valid",
        );
        run(rule("lucro_presumido", "00", "18")).await.unwrap();
    }
}
