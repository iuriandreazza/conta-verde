use std::fmt;

use crate::{CompanyId, DomainError};

const MAX_COMPANY_ID_CHARS: usize = 100;
/// The most a sealed secret may weigh (a nonce, the ciphertext of at most 200 characters, a tag and a version byte).
pub const MAX_SEALED_BYTES: usize = 400;

/// Which gateway a company issues through ([ADR 0085]). The `fake` gateway is for development and is chosen by the environment,
/// never by a company.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayKind {
    Focus,
    NfeIo,
}

impl GatewayKind {
    /// The key the API sends and the database stores; it does not change.
    pub fn key(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::NfeIo => "nfeio",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "focus" => Some(Self::Focus),
            "nfeio" => Some(Self::NfeIo),
            _ => None,
        }
    }
}

/// The secrets of the gateway's settings, by the name that binds them to their ciphertext and names them in the trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewaySecret {
    FocusTokenHomologation,
    FocusTokenProduction,
    NfeIoApiKey,
    WebhookSecret,
}

impl GatewaySecret {
    pub const ALL: [Self; 4] = [
        Self::FocusTokenHomologation,
        Self::FocusTokenProduction,
        Self::NfeIoApiKey,
        Self::WebhookSecret,
    ];

    /// The name of the field: what a secret is sealed **for** (so it cannot be moved to another field) and what the trail says.
    pub fn field(self) -> &'static str {
        match self {
            Self::FocusTokenHomologation => "focus_token_homologation",
            Self::FocusTokenProduction => "focus_token_production",
            Self::NfeIoApiKey => "nfeio_api_key",
            Self::WebhookSecret => "webhook_secret",
        }
    }
}

/// A secret in the clear, for the moment between the person typing it and its being sealed, or the gateway being built with it.
/// It has no `Display`, its `Debug` hides it and it is never serialized: a value that shows up in a log or a trail is a bug.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretText(String);

impl SecretText {
    /// 8 to 200 characters with no space and no control character: what a token or a key looks like, so a mistake (a pasted
    /// sentence, a trailing newline) is refused instead of stored.
    pub fn new(raw: &str) -> Result<Self, DomainError> {
        let text = raw.trim();
        let valid = (8..=200).contains(&text.chars().count())
            && !text.chars().any(|c| c.is_whitespace() || c.is_control());
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(DomainError::InvalidGatewaySettings("secret"))
        }
    }

    /// The value, for the one place that seals it or sends it to the gateway.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretText(<hidden>)")
    }
}

/// A secret as it is stored: sealed by the cipher, opaque to everything else.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedSecret(Vec<u8>);

impl SealedSecret {
    pub fn new(bytes: Vec<u8>) -> Result<Self, DomainError> {
        if bytes.is_empty() || bytes.len() > MAX_SEALED_BYTES {
            return Err(DomainError::InvalidGatewaySettings("sealed secret"));
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SealedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SealedSecret(<sealed>)")
    }
}

/// What a company has chosen for its gateway ([ADR 0085]): the kind, the secrets (sealed) and what is not secret. Not here: the
/// issuer's CNPJ (it is the company's fiscal settings'), the gateway's address and the hosts files are fetched from (the
/// environment's: a company must not be able to point the server elsewhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewaySettings {
    company_id: CompanyId,
    kind: GatewayKind,
    focus_token_homologation: Option<SealedSecret>,
    focus_token_production: Option<SealedSecret>,
    nfeio_api_key: Option<SealedSecret>,
    webhook_secret: Option<SealedSecret>,
    nfeio_company_homologation: Option<String>,
    nfeio_company_production: Option<String>,
    nfeio_search_confirmed: bool,
}

/// What is asked for, with the secrets already sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewaySettingsDraft {
    pub kind: GatewayKind,
    pub focus_token_homologation: Option<SealedSecret>,
    pub focus_token_production: Option<SealedSecret>,
    pub nfeio_api_key: Option<SealedSecret>,
    pub webhook_secret: Option<SealedSecret>,
    pub nfeio_company_homologation: Option<String>,
    pub nfeio_company_production: Option<String>,
    pub nfeio_search_confirmed: bool,
}

impl GatewaySettings {
    pub fn new(company_id: CompanyId, draft: GatewaySettingsDraft) -> Result<Self, DomainError> {
        Ok(Self {
            company_id,
            kind: draft.kind,
            focus_token_homologation: draft.focus_token_homologation,
            focus_token_production: draft.focus_token_production,
            nfeio_api_key: draft.nfeio_api_key,
            webhook_secret: draft.webhook_secret,
            nfeio_company_homologation: gateway_company_id(draft.nfeio_company_homologation)?,
            nfeio_company_production: gateway_company_id(draft.nfeio_company_production)?,
            nfeio_search_confirmed: draft.nfeio_search_confirmed,
        })
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn kind(&self) -> GatewayKind {
        self.kind
    }

    pub fn secret(&self, which: GatewaySecret) -> Option<&SealedSecret> {
        match which {
            GatewaySecret::FocusTokenHomologation => self.focus_token_homologation.as_ref(),
            GatewaySecret::FocusTokenProduction => self.focus_token_production.as_ref(),
            GatewaySecret::NfeIoApiKey => self.nfeio_api_key.as_ref(),
            GatewaySecret::WebhookSecret => self.webhook_secret.as_ref(),
        }
    }

    pub fn nfeio_company_homologation(&self) -> Option<&str> {
        self.nfeio_company_homologation.as_deref()
    }

    pub fn nfeio_company_production(&self) -> Option<&str> {
        self.nfeio_company_production.as_deref()
    }

    pub fn nfeio_search_confirmed(&self) -> bool {
        self.nfeio_search_confirmed
    }
}

/// The company's id at the gateway: letters, digits, `-` and `_`, 1 to 100 characters.
fn gateway_company_id(raw: Option<String>) -> Result<Option<String>, DomainError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let text = raw.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let valid = text.chars().count() <= MAX_COMPANY_ID_CHARS
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(Some(text.to_owned()))
    } else {
        Err(DomainError::InvalidGatewaySettings(
            "company id at the gateway",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> GatewaySettingsDraft {
        GatewaySettingsDraft {
            kind: GatewayKind::Focus,
            focus_token_homologation: Some(SealedSecret::new(vec![1; 40]).unwrap()),
            focus_token_production: None,
            nfeio_api_key: None,
            webhook_secret: None,
            nfeio_company_homologation: Some(" abc-123_X ".into()),
            nfeio_company_production: Some("  ".into()),
            nfeio_search_confirmed: false,
        }
    }

    #[test]
    fn a_secret_is_a_token_and_never_shows() {
        assert!(SecretText::new("sk_test_1234567890").is_ok());
        assert_eq!(
            SecretText::new("  sk_test_1234567890\n").unwrap().expose(),
            "sk_test_1234567890"
        );
        for bad in [
            "short",
            "has a space inside it",
            "tab\tinside_the_token",
            &"x".repeat(201),
            "",
        ] {
            assert!(SecretText::new(bad).is_err(), "{bad:?}");
        }
        let secret = SecretText::new("sk_test_1234567890").unwrap();
        assert!(!format!("{secret:?}").contains("sk_test"));
        let sealed = SealedSecret::new(vec![7; 40]).unwrap();
        assert!(!format!("{sealed:?}").contains('7'));
        assert!(SealedSecret::new(Vec::new()).is_err());
        assert!(SealedSecret::new(vec![0; MAX_SEALED_BYTES + 1]).is_err());
    }

    #[test]
    fn the_settings_keep_what_is_not_secret_in_the_clear_and_clean() {
        let settings = GatewaySettings::new(CompanyId::generate(), draft()).unwrap();
        assert_eq!(settings.kind(), GatewayKind::Focus);
        assert_eq!(settings.nfeio_company_homologation(), Some("abc-123_X"));
        assert_eq!(settings.nfeio_company_production(), None, "blank is none");
        assert!(
            settings
                .secret(GatewaySecret::FocusTokenHomologation)
                .is_some()
        );
        assert!(settings.secret(GatewaySecret::WebhookSecret).is_none());
        let rendered = format!("{settings:?}");
        assert!(rendered.contains("<sealed>"), "{rendered}");
        let mut bad = draft();
        bad.nfeio_company_production = Some("has space".into());
        assert!(GatewaySettings::new(CompanyId::generate(), bad).is_err());
        assert_eq!(GatewayKind::parse("nfeio"), Some(GatewayKind::NfeIo));
        assert_eq!(
            GatewayKind::parse("fake"),
            None,
            "the fake is never a company's choice"
        );
    }

    #[test]
    fn every_secret_has_a_field_name_of_its_own() {
        let names: std::collections::HashSet<_> = GatewaySecret::ALL
            .iter()
            .map(|secret| secret.field())
            .collect();
        assert_eq!(names.len(), GatewaySecret::ALL.len());
    }
}
