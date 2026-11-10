use conta_verde_domain::{
    Actor, ApiKey, ApiKeyDraft, ApiKeyId, AuditAction, AuditRecord, CalendarDate, CompanyId,
    DomainError, MAX_API_KEYS, MAX_ROTATION_GRACE_HOURS, api_key_changes, api_key_creation_changes,
};

use crate::audit::audit_entry;
use crate::{ApiKeyRepository, ApplicationError, AuthError, RepositoryError, SecretService};

/// The text every secret starts with, so that a leaked one can be recognised and searched for.
const SECRET_PREFIX: &str = "cvk_";
/// How many characters of the secret, after the prefix, are kept to tell keys apart.
const LABEL_CHARS: usize = 6;
const HOUR_MILLIS: i64 = 3_600_000;

/// A key just made, with the only moment its secret is ever known.
pub struct IssuedKey {
    pub key: ApiKey,
    pub secret: String,
}

// Manual impl so a logged or unwrapped value never prints the secret.
impl std::fmt::Debug for IssuedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedKey")
            .field("key", &self.key)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// The keys of a company: made, rotated and revoked by its owners and admins, and told apart by the secret when a program
/// speaks ([ADR 0087]). The time is the caller's, so a test moves it.
pub struct ApiKeyService<R, S> {
    keys: R,
    secrets: S,
}

impl<R: ApiKeyRepository, S: SecretService> ApiKeyService<R, S> {
    pub fn new(keys: R, secrets: S) -> Self {
        Self { keys, secrets }
    }

    pub async fn create(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: ApiKeyDraft,
        now_millis: i64,
    ) -> Result<IssuedKey, ApplicationError> {
        self.ensure_room(company_id).await?;
        self.issue(company_id, actor, draft, now_millis, None).await
    }

    /// Makes a key like this one, with its own secret, and lets this one go on for `grace_hours` (never longer than it
    /// already had). A revoked or expired key is not rotated: it is made again.
    pub async fn rotate(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ApiKeyId,
        grace_hours: u32,
        now_millis: i64,
    ) -> Result<IssuedKey, ApplicationError> {
        if grace_hours > MAX_ROTATION_GRACE_HOURS {
            return Err(DomainError::InvalidApiKeyGrace.into());
        }
        let old = self.find(company_id, id).await?;
        if !old.is_usable_at(now_millis) {
            return Err(DomainError::ApiKeyIsNotActive.into());
        }
        self.ensure_room(company_id).await?;
        let secret = self.new_secret();
        let draft = ApiKeyDraft {
            name: old.name().to_owned(),
            scopes: old.scope_keys(),
            // A key that was made to expire gives a new one the same life, so a rotation never makes a permanent key.
            expires_in_hours: old.expires_at().map(|at| {
                u32::try_from(((at - old.created_at()) / HOUR_MILLIS).max(1)).unwrap_or(u32::MAX)
            }),
        };
        let new = ApiKey::new(
            ApiKeyId::generate(),
            company_id,
            label_of(&secret),
            draft,
            actor.clone(),
            now_millis,
            Some(old.id()),
        )?;
        let ending = old.expiring_by(now_millis + i64::from(grace_hours) * HOUR_MILLIS);
        let new_audit = audit_entry(
            &new,
            actor,
            AuditAction::Created,
            api_key_creation_changes(&new),
        );
        let old_audit = audit_entry(
            &ending,
            actor,
            AuditAction::Updated,
            api_key_changes(&old, &ending),
        );
        self.keys
            .rotate(
                &new,
                &self.secrets.digest_token(&secret),
                &new_audit,
                &ending,
                &old_audit,
            )
            .await
            .map_err(missing)?;
        Ok(IssuedKey { key: new, secret })
    }

    /// Refuses the key from now on. Asking again for one already revoked changes and records nothing.
    pub async fn revoke(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ApiKeyId,
    ) -> Result<ApiKey, ApplicationError> {
        let before = self.find(company_id, id).await?;
        if before.is_revoked() {
            return Ok(before);
        }
        let after = before.revoked_now();
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            api_key_changes(&before, &after),
        );
        self.keys.update(&after, &audit).await.map_err(missing)?;
        Ok(after)
    }

    pub async fn list(&self, company_id: CompanyId) -> Result<Vec<ApiKey>, ApplicationError> {
        Ok(self.keys.list(company_id).await?)
    }

    pub async fn history(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's key answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.keys.history(company_id, id).await?)
    }

    /// The key behind a secret a program sent, if it may still be used at `now_millis`. Anything else (no such secret, a
    /// revoked or an expired key) is the same refusal, so a secret cannot be probed. The day of use is written when it
    /// changes, so reading never writes more than once a day.
    pub async fn authenticate(
        &self,
        secret: &str,
        now_millis: i64,
        today: CalendarDate,
    ) -> Result<ApiKey, ApplicationError> {
        if !secret.starts_with(SECRET_PREFIX) {
            return Err(AuthError::Unauthenticated.into());
        }
        let key = self
            .keys
            .find_by_digest(&self.secrets.digest_token(secret))
            .await?
            .filter(|key| key.is_usable_at(now_millis))
            .ok_or(AuthError::Unauthenticated)?;
        if key.last_used_on() == Some(today) {
            return Ok(key);
        }
        // Failing to note the day must not fail the request it was noted for.
        let _ = self
            .keys
            .record_use(key.company_id(), key.id(), today)
            .await;
        Ok(key.used_on(today))
    }

    async fn issue(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: ApiKeyDraft,
        now_millis: i64,
        rotated_from: Option<ApiKeyId>,
    ) -> Result<IssuedKey, ApplicationError> {
        let secret = self.new_secret();
        let key = ApiKey::new(
            ApiKeyId::generate(),
            company_id,
            label_of(&secret),
            draft,
            actor.clone(),
            now_millis,
            rotated_from,
        )?;
        let audit = audit_entry(
            &key,
            actor,
            AuditAction::Created,
            api_key_creation_changes(&key),
        );
        self.keys
            .save(&key, &self.secrets.digest_token(&secret), &audit)
            .await?;
        Ok(IssuedKey { key, secret })
    }

    fn new_secret(&self) -> String {
        format!("{SECRET_PREFIX}{}", self.secrets.new_token())
    }

    async fn ensure_room(&self, company_id: CompanyId) -> Result<(), ApplicationError> {
        // A revoked key is kept for its history but is no credential: only the ones that can still be used count.
        let live = self
            .keys
            .list(company_id)
            .await?
            .iter()
            .filter(|key| !key.is_revoked())
            .count();
        if live >= MAX_API_KEYS {
            return Err(DomainError::TooManyApiKeys.into());
        }
        Ok(())
    }

    async fn find(&self, company_id: CompanyId, id: ApiKeyId) -> Result<ApiKey, ApplicationError> {
        self.keys
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::ApiKeyNotFound)
    }
}

fn label_of(secret: &str) -> String {
    secret
        .chars()
        .take(SECRET_PREFIX.len() + LABEL_CHARS)
        .collect()
}

fn missing(error: RepositoryError) -> ApplicationError {
    match error {
        // It vanished between the read and the write.
        RepositoryError::NotFound => ApplicationError::ApiKeyNotFound,
        other => other.into(),
    }
}
