use std::future::Future;

use conta_verde_domain::{
    Actor, ApiKeyDraft, ApiKeyId, AuditAction, AuditRecord, CompanyId, Connector, ConnectorId,
    ConnectorKind, MAX_CONNECTORS, WebhookSubscriptionDraft, WebhookSubscriptionId,
    connector_changes, connector_creation_changes,
};

use crate::JobQueue;
use crate::audit::audit_entry;
use crate::{
    ApiKeyRepository, ApiKeyService, ApplicationError, ConnectorRepository, RepositoryError,
    SecretCipher, SecretService, WebhookRepository, WebhookService,
};

/// What a connector asks of the keys of the company ([ADR 0090]): a narrow trait, so that the service does not carry the key
/// service's own type parameters.
pub trait ConnectorKeys: Send + Sync {
    /// Makes a key with `scopes` and returns its id and its secret, which exists only in this answer.
    fn issue(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        name: String,
        scopes: Vec<String>,
        now: i64,
    ) -> impl Future<Output = Result<(ApiKeyId, String), ApplicationError>> + Send;

    fn revoke(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ApiKeyId,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// Whether the key still opens the API: it exists, is not revoked and has not expired.
    fn is_usable(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
        now: i64,
    ) -> impl Future<Output = Result<bool, ApplicationError>> + Send;
}

/// What a connector asks of the webhook subscriptions of the company.
pub trait ConnectorHooks: Send + Sync {
    /// Whether the installation can seal secrets; without it nothing is made.
    fn is_available(&self) -> bool;

    /// Makes a subscription to `events` at `address` and returns its id and its secret, which exists only in this answer.
    fn create(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        address: String,
        events: Vec<String>,
        now: i64,
    ) -> impl Future<Output = Result<(WebhookSubscriptionId, String), ApplicationError>> + Send;

    /// Switches a subscription on again at `address` for `events`, with a new secret returned once.
    fn turn_back_on(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
        address: String,
        events: Vec<String>,
        now: i64,
    ) -> impl Future<Output = Result<String, ApplicationError>> + Send;

    /// Switches a subscription off, keeping its address and events.
    fn switch_off(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// Whether the subscription exists and is on.
    fn is_on(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> impl Future<Output = Result<bool, ApplicationError>> + Send;
}

/// The API holds its services in an `Arc`, shared with the routes that use them directly.
impl<T: ConnectorKeys> ConnectorKeys for std::sync::Arc<T> {
    async fn issue(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        name: String,
        scopes: Vec<String>,
        now: i64,
    ) -> Result<(ApiKeyId, String), ApplicationError> {
        (**self).issue(company_id, actor, name, scopes, now).await
    }

    async fn revoke(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ApiKeyId,
    ) -> Result<(), ApplicationError> {
        ConnectorKeys::revoke(&**self, company_id, actor, id).await
    }

    async fn is_usable(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
        now: i64,
    ) -> Result<bool, ApplicationError> {
        (**self).is_usable(company_id, id, now).await
    }
}

impl<T: ConnectorHooks> ConnectorHooks for std::sync::Arc<T> {
    fn is_available(&self) -> bool {
        ConnectorHooks::is_available(&**self)
    }

    async fn create(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        address: String,
        events: Vec<String>,
        now: i64,
    ) -> Result<(WebhookSubscriptionId, String), ApplicationError> {
        ConnectorHooks::create(&**self, company_id, actor, address, events, now).await
    }

    async fn turn_back_on(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
        address: String,
        events: Vec<String>,
        now: i64,
    ) -> Result<String, ApplicationError> {
        (**self)
            .turn_back_on(company_id, actor, id, address, events, now)
            .await
    }

    async fn switch_off(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
    ) -> Result<(), ApplicationError> {
        (**self).switch_off(company_id, actor, id).await
    }

    async fn is_on(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<bool, ApplicationError> {
        (**self).is_on(company_id, id).await
    }
}

impl<R: ApiKeyRepository, S: SecretService> ConnectorKeys for ApiKeyService<R, S> {
    async fn issue(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        name: String,
        scopes: Vec<String>,
        now: i64,
    ) -> Result<(ApiKeyId, String), ApplicationError> {
        let issued = self
            .create(
                company_id,
                actor,
                ApiKeyDraft {
                    name,
                    scopes,
                    expires_in_hours: None,
                },
                now,
            )
            .await?;
        Ok((issued.key.id(), issued.secret))
    }

    async fn revoke(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ApiKeyId,
    ) -> Result<(), ApplicationError> {
        ApiKeyService::revoke(self, company_id, actor, id)
            .await
            .map(|_| ())
    }

    async fn is_usable(
        &self,
        company_id: CompanyId,
        id: ApiKeyId,
        now: i64,
    ) -> Result<bool, ApplicationError> {
        Ok(self
            .list(company_id)
            .await?
            .iter()
            .any(|key| key.id() == id && key.is_usable_at(now)))
    }
}

impl<R, C, Q, S> ConnectorHooks for WebhookService<R, C, Q, S>
where
    R: WebhookRepository,
    C: SecretCipher,
    Q: JobQueue,
    S: SecretService,
{
    fn is_available(&self) -> bool {
        WebhookService::is_available(self)
    }

    async fn create(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        address: String,
        events: Vec<String>,
        now: i64,
    ) -> Result<(WebhookSubscriptionId, String), ApplicationError> {
        let made = WebhookService::create(
            self,
            company_id,
            actor,
            WebhookSubscriptionDraft {
                url: address,
                events,
                active: true,
            },
            now,
        )
        .await?;
        Ok((made.subscription.id(), made.secret))
    }

    async fn turn_back_on(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
        address: String,
        events: Vec<String>,
        now: i64,
    ) -> Result<String, ApplicationError> {
        self.update(
            company_id,
            actor,
            id,
            WebhookSubscriptionDraft {
                url: address,
                events,
                active: true,
            },
        )
        .await?;
        self.replace_secret(company_id, actor, id, None, now)
            .await?
            .ok_or(ApplicationError::WebhookNotFound)
    }

    async fn switch_off(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
    ) -> Result<(), ApplicationError> {
        let held = self
            .list(company_id)
            .await?
            .into_iter()
            .find(|subscription| subscription.id() == id)
            .ok_or(ApplicationError::WebhookNotFound)?;
        self.update(
            company_id,
            actor,
            id,
            WebhookSubscriptionDraft {
                url: held.url().to_owned(),
                events: held
                    .events()
                    .iter()
                    .map(|kind| kind.as_str().to_owned())
                    .collect(),
                active: false,
            },
        )
        .await
        .map(|_| ())
    }

    async fn is_on(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<bool, ApplicationError> {
        Ok(self
            .list(company_id)
            .await?
            .iter()
            .any(|subscription| subscription.id() == id && subscription.is_active()))
    }
}

/// A connector as it stands now: what is stored, and whether what it was given still works. The person can revoke the key on the
/// keys page or edit the subscription on the webhooks page, so this is read every time and never kept.
#[derive(Debug, Clone)]
pub struct ConnectorView {
    pub connector: Connector,
    pub key_usable: bool,
    pub subscription_on: bool,
}

/// What turning a connector on gives back: the two secrets, shown once, to paste into the package.
pub struct TurnedOn {
    pub connector: Connector,
    pub api_key_secret: String,
    pub webhook_secret: String,
}

// Manual impl so a logged or unwrapped value never prints the secrets.
impl std::fmt::Debug for TurnedOn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnedOn")
            .field("connector", &self.connector)
            .field("secrets", &"<hidden>")
            .finish()
    }
}

/// The connectors of a company and their turning on and off ([ADR 0090]). Turning on makes only what any integrator could make by
/// hand, a key and a subscription, through the same services; turning off revokes the key and switches the subscription off. The
/// time is the caller's.
pub struct ConnectorService<C, K, W> {
    connectors: C,
    keys: K,
    hooks: W,
}

impl<C, K, W> ConnectorService<C, K, W>
where
    C: ConnectorRepository,
    K: ConnectorKeys,
    W: ConnectorHooks,
{
    pub fn new(connectors: C, keys: K, hooks: W) -> Self {
        Self {
            connectors,
            keys,
            hooks,
        }
    }

    pub async fn create(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        kind: ConnectorKind,
        name: &str,
        now: i64,
    ) -> Result<Connector, ApplicationError> {
        if self.connectors.list(company_id).await?.len() >= MAX_CONNECTORS {
            return Err(conta_verde_domain::DomainError::TooManyConnectors.into());
        }
        let connector = Connector::new(
            ConnectorId::generate(),
            company_id,
            kind,
            name,
            actor.clone(),
            now,
        )?;
        let audit = audit_entry(
            &connector,
            actor,
            AuditAction::Created,
            connector_creation_changes(&connector),
        );
        self.connectors.save(&connector, &audit).await?;
        Ok(connector)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        now: i64,
    ) -> Result<Vec<ConnectorView>, ApplicationError> {
        let mut views = Vec::new();
        for connector in self.connectors.list(company_id).await? {
            let key_usable = match connector.api_key_id() {
                Some(id) => self.keys.is_usable(company_id, id, now).await?,
                None => false,
            };
            let subscription_on = match connector.webhook_subscription_id() {
                Some(id) => self.hooks.is_on(company_id, id).await?,
                None => false,
            };
            views.push(ConnectorView {
                connector,
                key_usable,
                subscription_on,
            });
        }
        Ok(views)
    }

    /// Whether the installation can seal the subscription's secret; without it nothing is turned on.
    pub fn is_available(&self) -> bool {
        self.hooks.is_available()
    }

    /// Makes the key and the subscription for `address` (where the package listens) and records them. A key the person cannot
    /// use because the subscription failed, or the other way round, is not left behind: what was made is taken back, so asking
    /// again starts clean. Two at once make one winner and the loser takes back what it made.
    pub async fn turn_on(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ConnectorId,
        address: &str,
        now: i64,
    ) -> Result<TurnedOn, ApplicationError> {
        // Before the key: with webhooks off nothing can be made, and a key made first would be left behind.
        if !self.hooks.is_available() {
            return Err(ApplicationError::WebhooksOff);
        }
        let before = self.find(company_id, id).await?;
        if before.is_active() {
            return Err(RepositoryError::ConnectorChanged.into());
        }
        let kind = before.kind();
        let (key_id, api_key_secret) = self
            .keys
            .issue(
                company_id,
                actor,
                before.key_name(),
                kind.scopes()
                    .iter()
                    .map(|scope| (*scope).to_owned())
                    .collect(),
                now,
            )
            .await?;
        let events: Vec<String> = kind
            .events()
            .iter()
            .map(|event| (*event).to_owned())
            .collect();
        let hook = match before.webhook_subscription_id() {
            None => {
                self.hooks
                    .create(company_id, actor, address.to_owned(), events, now)
                    .await
            }
            Some(subscription) => self
                .hooks
                .turn_back_on(
                    company_id,
                    actor,
                    subscription,
                    address.to_owned(),
                    events,
                    now,
                )
                .await
                .map(|secret| (subscription, secret)),
        };
        let (subscription_id, webhook_secret) = match hook {
            Ok(made) => made,
            Err(error) => {
                self.take_back(company_id, actor, key_id, None).await;
                return Err(error);
            }
        };
        let after = before.turned_on(key_id, subscription_id);
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            connector_changes(&before, &after),
        );
        if let Err(error) = self.connectors.switch(&after, false, &audit).await {
            self.take_back(company_id, actor, key_id, Some(subscription_id))
                .await;
            return Err(missing(error));
        }
        Ok(TurnedOn {
            connector: after,
            api_key_secret,
            webhook_secret,
        })
    }

    /// Revokes the key and switches the subscription off. Asking again for one that is off changes and records nothing. The key
    /// goes first: what the package could use stops working before anything else is touched, and a failure further on leaves a
    /// connector that still says it is on, which asking again finishes.
    pub async fn turn_off(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ConnectorId,
    ) -> Result<Connector, ApplicationError> {
        let before = self.find(company_id, id).await?;
        if !before.is_active() {
            return Ok(before);
        }
        if let Some(key) = before.api_key_id() {
            self.keys.revoke(company_id, actor, key).await?;
        }
        if let Some(subscription) = before.webhook_subscription_id() {
            self.hooks
                .switch_off(company_id, actor, subscription)
                .await?;
        }
        let after = before.turned_off();
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            connector_changes(&before, &after),
        );
        self.connectors
            .switch(&after, true, &audit)
            .await
            .map_err(missing)?;
        Ok(after)
    }

    pub async fn history(
        &self,
        company_id: CompanyId,
        id: ConnectorId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's connector answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.connectors.history(company_id, id).await?)
    }

    /// What was made for a turning on that did not finish: a failure here is logged, since the person's answer is the first error.
    async fn take_back(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        key: ApiKeyId,
        subscription: Option<WebhookSubscriptionId>,
    ) {
        if let Err(error) = self.keys.revoke(company_id, actor, key).await {
            tracing::error!(%error, "a key made for a connector could not be taken back");
        }
        if let Some(subscription) = subscription
            && let Err(error) = self.hooks.switch_off(company_id, actor, subscription).await
        {
            tracing::error!(%error, "a subscription made for a connector could not be switched off");
        }
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ConnectorId,
    ) -> Result<Connector, ApplicationError> {
        self.connectors
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::ConnectorNotFound)
    }
}

fn missing(error: RepositoryError) -> ApplicationError {
    match error {
        // It vanished between the read and the write.
        RepositoryError::NotFound => ApplicationError::ConnectorNotFound,
        other => other.into(),
    }
}
