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
        settings
            .secret(secret)
            .map(|sealed| {
                self.cipher
                    .open(company_id, secret.field(), sealed.as_bytes())
                    .map_err(ApplicationError::from)
            })
            .transpose()
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
        let after = GatewaySettings::new(company_id, draft)?;
        let mut plain = plain_changes(before.as_ref(), &after);
        plain.append(&mut changes);
        if before.is_some() && plain.is_empty() {
            return Ok(after);
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
