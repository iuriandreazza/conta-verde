use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, FiscalSettings, FiscalSettingsDraft,
    fiscal_settings_changes, fiscal_settings_creation_changes,
};

use crate::audit::audit_entry;
use crate::{ApplicationError, FiscalSettingsRepository};

/// How a company issues fiscal documents ([ADR 0074]): the issuer, the regime, the environment, the series, the day
/// the certificate expires and the operation natures with their tax rules.
pub struct FiscalSettingsService<R> {
    repository: R,
}

impl<R: FiscalSettingsRepository> FiscalSettingsService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// The company's settings, or `None` while it has not set them: and then no document can be issued.
    pub async fn get(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<FiscalSettings>, ApplicationError> {
        Ok(self.repository.find(company_id).await?)
    }

    /// Replaces the settings as a whole. The same settings again write nothing.
    pub async fn replace(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: FiscalSettingsDraft,
    ) -> Result<FiscalSettings, ApplicationError> {
        let after = FiscalSettings::new(company_id, draft)?;
        let before = self.repository.find(company_id).await?;
        if before.as_ref() == Some(&after) {
            return Ok(after);
        }
        let (action, changes) = match &before {
            Some(stored) => (
                AuditAction::Updated,
                fiscal_settings_changes(stored, &after),
            ),
            None => (
                AuditAction::Created,
                fiscal_settings_creation_changes(&after),
            ),
        };
        let audit = audit_entry(&after, actor, action, changes);
        self.repository.save(&after, &audit).await?;
        Ok(after)
    }

    /// What was recorded about the company's settings, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        Ok(self.repository.history(company_id).await?)
    }
}
