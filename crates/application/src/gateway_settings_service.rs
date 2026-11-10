use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, FieldChange, GatewayKind, GatewaySecret,
    GatewaySettings, GatewaySettingsDraft, SealedSecret, SecretText,
};

use crate::audit::audit_entry;
use crate::{ApplicationError, GatewaySettingsRepository, SecretCipher};

/// What to do with one secret when the settings are replaced: the API never hands a secret back, so a change says whether the one
/// stored stays, is replaced or is cleared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretChange {
    Keep,
    Set(SecretText),
    Clear,
}

/// What the person asks for. Nothing here is a secret that is kept: the secrets are a [`SecretChange`] each.
#[derive(Debug, Clone)]
pub struct GatewaySettingsInput {
    pub kind: GatewayKind,
    pub focus_token_homologation: SecretChange,
    pub focus_token_production: SecretChange,
    pub nfeio_api_key: SecretChange,
    pub webhook_secret: SecretChange,
    pub nfeio_company_homologation: Option<String>,
    pub nfeio_company_production: Option<String>,
    pub nfeio_search_confirmed: bool,
}

impl GatewaySettingsInput {
    /// Whether saving this would change **which gateway account the company's documents go through**: a first row (it replaces the
    /// server's gateway), another kind, another token, key or company id. The webhook secret and the NFE.io search flag do not.
    pub fn changes_the_gateway(&self, before: Option<&GatewaySettings>) -> bool {
        let Some(before) = before else {
            return true;
        };
        let replaced = |change: &SecretChange| !matches!(change, SecretChange::Keep);
        self.kind != before.kind()
            || replaced(&self.focus_token_homologation)
            || replaced(&self.focus_token_production)
            || replaced(&self.nfeio_api_key)
            || self.nfeio_company_homologation.as_deref() != before.nfeio_company_homologation()
            || self.nfeio_company_production.as_deref() != before.nfeio_company_production()
    }
}

/// How a company reaches its fiscal gateway ([ADR 0085]): the kind, the secrets (sealed, write-only) and the company's ids at the
/// gateway. **Nothing in the trail or in what this returns is a secret.**
pub struct GatewaySettingsService<R, C> {
    repository: R,
    cipher: C,
}

impl<R: GatewaySettingsRepository, C: SecretCipher> GatewaySettingsService<R, C> {
    pub fn new(repository: R, cipher: C) -> Self {
        Self { repository, cipher }
    }

    /// Whether secrets can be saved at all: the environment has the key ([ADR 0085]).
    pub fn encryption_available(&self) -> bool {
        self.cipher.is_available()
    }

    /// The company's settings, or `None` while it has set none (the environment's, if any, is used).
    pub async fn get(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<GatewaySettings>, ApplicationError> {
        Ok(self.repository.find(company_id).await?)
    }

    /// A secret of the company's, opened: only for the moment a gateway is built with it. `None` when it is not set; an error when
    /// there is no key or the value does not open under this company and field.
    pub async fn open(
        &self,
        company_id: CompanyId,
        secret: GatewaySecret,
    ) -> Result<Option<SecretText>, ApplicationError> {
        let Some(settings) = self.repository.find(company_id).await? else {
            return Ok(None);
        };
        self.open_from(&settings, secret)
    }

    /// The same for settings that were read already, so everything a gateway is built from comes from one read and one version.
    pub fn open_from(
        &self,
        settings: &GatewaySettings,
        secret: GatewaySecret,
    ) -> Result<Option<SecretText>, ApplicationError> {
        settings
            .secret(secret)
            .map(|sealed| {
                self.cipher
                    .open(settings.company_id(), secret.field(), sealed.as_bytes())
                    .map_err(|_| ApplicationError::SecretUnreadable(secret.field()))
            })
            .transpose()
    }

    /// The secrets stored for the company that cannot be opened now (the key changed or was lost), so the page can say so instead
    /// of showing them as configured. Never their values.
    pub async fn unreadable(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<GatewaySecret>, ApplicationError> {
        let Some(settings) = self.repository.find(company_id).await? else {
            return Ok(Vec::new());
        };
        Ok(GatewaySecret::ALL
            .into_iter()
            .filter(|secret| {
                settings.secret(*secret).is_some_and(|sealed| {
                    self.cipher
                        .open(company_id, secret.field(), sealed.as_bytes())
                        .is_err()
                })
            })
            .collect())
    }

    /// Removes the company's settings: the server's own are used again. Every secret that was set is recorded as cleared, never its
    /// value. Nothing to remove writes nothing.
    pub async fn reset(
        &self,
        company_id: CompanyId,
        actor: &Actor,
    ) -> Result<(), ApplicationError> {
        let Some(before) = self.repository.find(company_id).await? else {
            return Ok(());
        };
        let mut changes = vec![change_of("kind", Some(before.kind().key()), None)];
        for secret in GatewaySecret::ALL {
            if before.secret(secret).is_some() {
                changes.push(change_of(secret.field(), Some("set"), None));
            }
        }
        let audit = audit_entry(&before, actor, AuditAction::Updated, changes);
        self.repository
            .remove(company_id, before.version(), &audit)
            .await?;
        Ok(())
    }

    /// Replaces the settings as a whole. A secret that is `Keep` stays as it is (the same sealed bytes, so nothing is rewritten for
    /// it), `Set` is sealed for this company and field, `Clear` is removed. Setting one without a key is refused. The same
    /// settings again write nothing.
    pub async fn replace(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        input: GatewaySettingsInput,
    ) -> Result<GatewaySettings, ApplicationError> {
        let before = self.repository.find(company_id).await?;
        let mut changes = Vec::new();
        let mut sealed = |secret: GatewaySecret,
                          change: SecretChange|
         -> Result<Option<SealedSecret>, ApplicationError> {
            let held = before
                .as_ref()
                .and_then(|settings| settings.secret(secret))
                .cloned();
            match change {
                SecretChange::Keep => Ok(held),
                SecretChange::Clear => {
                    if held.is_some() {
                        changes.push(change_of(secret.field(), Some("set"), None));
                    }
                    Ok(None)
                }
                SecretChange::Set(value) => {
                    if !self.cipher.is_available() {
                        return Err(ApplicationError::FiscalRule(
                            "FISCAL_SECRETS_KEY is not set on the server: secrets cannot be saved until it is".to_owned(),
                        ));
                    }
                    let bytes = self.cipher.seal(company_id, secret.field(), &value)?;
                    changes.push(change_of(
                        secret.field(),
                        held.as_ref().map(|_| "set"),
                        Some(if held.is_some() { "changed" } else { "set" }),
                    ));
                    Ok(Some(SealedSecret::new(bytes)?))
                }
            }
        };
        let draft = GatewaySettingsDraft {
            kind: input.kind,
            focus_token_homologation: sealed(
                GatewaySecret::FocusTokenHomologation,
                input.focus_token_homologation,
            )?,
            focus_token_production: sealed(
                GatewaySecret::FocusTokenProduction,
                input.focus_token_production,
            )?,
            nfeio_api_key: sealed(GatewaySecret::NfeIoApiKey, input.nfeio_api_key)?,
            webhook_secret: sealed(GatewaySecret::WebhookSecret, input.webhook_secret)?,
            nfeio_company_homologation: input.nfeio_company_homologation,
            nfeio_company_production: input.nfeio_company_production,
            nfeio_search_confirmed: input.nfeio_search_confirmed,
        };
        let version = before.as_ref().map_or(1, |settings| settings.version() + 1);
        let after = GatewaySettings::new(company_id, draft, version)?;
        let mut plain = plain_changes(before.as_ref(), &after);
        plain.append(&mut changes);
        if let Some(before) = &before
            && plain.is_empty()
        {
            return Ok(before.clone());
        }
        let action = if before.is_some() {
            AuditAction::Updated
        } else {
            AuditAction::Created
        };
        let audit = audit_entry(&after, actor, action, plain);
        self.repository.save(&after, &audit).await?;
        Ok(after)
    }

    /// What was recorded about the company's gateway settings, oldest first. It never holds a secret.
    pub async fn history(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        Ok(self.repository.history(company_id).await?)
    }
}

fn change_of(field: &str, from: Option<&str>, to: Option<&str>) -> FieldChange {
    FieldChange {
        field: field.to_owned(),
        from: from.map(str::to_owned),
        to: to.map(str::to_owned),
    }
}

/// What changed that is not a secret: the kind, the ids at the gateway and the search confirmation.
fn plain_changes(before: Option<&GatewaySettings>, after: &GatewaySettings) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    let mut note = |field: &str, from: Option<String>, to: Option<String>| {
        if from != to {
            changes.push(FieldChange {
                field: field.to_owned(),
                from,
                to,
            });
        }
    };
    note(
        "kind",
        before.map(|settings| settings.kind().key().to_owned()),
        Some(after.kind().key().to_owned()),
    );
    note(
        "nfeio_company_homologation",
        before.and_then(|settings| settings.nfeio_company_homologation().map(str::to_owned)),
        after.nfeio_company_homologation().map(str::to_owned),
    );
    note(
        "nfeio_company_production",
        before.and_then(|settings| settings.nfeio_company_production().map(str::to_owned)),
        after.nfeio_company_production().map(str::to_owned),
    );
    note(
        "nfeio_search_confirmed",
        before.map(|settings| settings.nfeio_search_confirmed().to_string()),
        Some(after.nfeio_search_confirmed().to_string()),
    );
    changes
}

#[cfg(test)]
mod tests {
    use conta_verde_domain::{GatewaySettingsDraft, SealedSecret};

    use super::*;

    fn input(kind: GatewayKind) -> GatewaySettingsInput {
        GatewaySettingsInput {
            kind,
            focus_token_homologation: SecretChange::Keep,
            focus_token_production: SecretChange::Keep,
            nfeio_api_key: SecretChange::Keep,
            webhook_secret: SecretChange::Keep,
            nfeio_company_homologation: None,
            nfeio_company_production: None,
            nfeio_search_confirmed: false,
        }
    }

    fn stored(kind: GatewayKind) -> GatewaySettings {
        GatewaySettings::new(
            CompanyId::generate(),
            GatewaySettingsDraft {
                kind,
                focus_token_homologation: Some(SealedSecret::new(vec![1; 30]).unwrap()),
                focus_token_production: None,
                nfeio_api_key: None,
                webhook_secret: None,
                nfeio_company_homologation: None,
                nfeio_company_production: None,
                nfeio_search_confirmed: false,
            },
            1,
        )
        .unwrap()
    }

    /// Only what changes the account the documents go through is held back while documents wait.
    #[test]
    fn only_a_change_of_account_changes_the_gateway() {
        let before = stored(GatewayKind::Focus);
        assert!(input(GatewayKind::Focus).changes_the_gateway(None));
        assert!(!input(GatewayKind::Focus).changes_the_gateway(Some(&before)));
        assert!(input(GatewayKind::NfeIo).changes_the_gateway(Some(&before)));
        let new_token = GatewaySettingsInput {
            focus_token_production: SecretChange::Clear,
            ..input(GatewayKind::Focus)
        };
        assert!(new_token.changes_the_gateway(Some(&before)));
        let new_company = GatewaySettingsInput {
            nfeio_company_production: Some("company-1".to_owned()),
            ..input(GatewayKind::Focus)
        };
        assert!(new_company.changes_the_gateway(Some(&before)));
        let only_the_webhook = GatewaySettingsInput {
            webhook_secret: SecretChange::Clear,
            nfeio_search_confirmed: true,
            ..input(GatewayKind::Focus)
        };
        assert!(!only_the_webhook.changes_the_gateway(Some(&before)));
    }
}
