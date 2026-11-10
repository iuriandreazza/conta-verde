use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CalendarDate, CompanyId, DeliveryStatus, DomainError, JobKind,
    MAX_WEBHOOK_SUBSCRIPTIONS, NewJob, SECRET_GRACE_MILLIS, SealedSecret, SecretText,
    WebhookDelivery, WebhookDeliveryId, WebhookEventKind, WebhookSubscription,
    WebhookSubscriptionDraft, WebhookSubscriptionId, delivery_error, webhook_secret_change,
    webhook_subscription_changes, webhook_subscription_creation_changes,
};
use uuid::Uuid;

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, DeliveryCursor, JobQueue, JobReport, Outgoing, RepositoryError, SecretCipher,
    SecretService, WebhookRepository, WebhookSender,
};

/// Whose ids the events have: a fixed namespace, so that the id of an event is a function of what happened and the same thing
/// happening twice (a step that asks again) is the same event ([ADR 0088]).
const EVENT_NAMESPACE: Uuid = Uuid::from_u128(0x7c8f_3b1e_5a42_4d9a_9e0b_2f64_a1d3_c5e7);
const LEASE_SECONDS: i64 = 60;

/// The id of an event: a UUID v5 of the company, the kind, the record it is about and what, of that record, it is about.
pub fn event_id(
    company_id: CompanyId,
    kind: WebhookEventKind,
    record: Uuid,
    qualifier: &str,
) -> Uuid {
    let name = format!(
        "{}|{}|{}|{}",
        company_id.as_uuid(),
        kind.as_str(),
        record,
        qualifier
    );
    Uuid::new_v5(&EVENT_NAMESPACE, name.as_bytes())
}

/// The body of a delivery: `{ id, type, created_at, company_id, data: { object } }`, `object` being the JSON of the resource as
/// the `v1` API writes it. Every other part is made of an id, a fixed word and a moment, so none needs escaping.
pub fn envelope(
    event_id: Uuid,
    kind: WebhookEventKind,
    company_id: CompanyId,
    created_at_millis: i64,
    object: &str,
) -> String {
    format!(
        r#"{{"id":"{event_id}","type":"{}","created_at":"{}","company_id":"{}","data":{{"object":{object}}}}}"#,
        kind.as_str(),
        rfc3339(created_at_millis),
        company_id.as_uuid(),
    )
}

/// `2026-11-10T19:56:41Z` for a moment in Unix milliseconds.
pub fn rfc3339(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let seconds = millis.rem_euclid(86_400_000) / 1000;
    let day = CalendarDate::from_unix_days(days).unwrap_or(CalendarDate::EARLIEST);
    format!(
        "{day}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

/// A subscription just made, or whose secret was just generated: the only answer that carries the secret.
pub struct Subscribed {
    pub subscription: WebhookSubscription,
    pub secret: String,
}

impl std::fmt::Debug for Subscribed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscribed")
            .field("subscription", &self.subscription)
            .field("secret", &"<hidden>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct DeliveryPage {
    pub items: Vec<WebhookDelivery>,
    pub next: Option<DeliveryCursor>,
}

/// The subscriptions of a company, the deliveries to them and the worker that sends those ([ADR 0088]). Secrets are sealed
/// with the cipher, write-only like every secret of the product: made here or typed by the person, shown once when made here,
/// never read back. The time is the caller's.
pub struct WebhookService<R, C, Q, S> {
    repository: R,
    cipher: C,
    queue: Q,
    secrets: S,
    allow_private_targets: bool,
}

impl<R, C, Q, S> WebhookService<R, C, Q, S>
where
    R: WebhookRepository,
    C: SecretCipher,
    Q: JobQueue,
    S: SecretService,
{
    /// `allow_private_targets` is the operator's development and test opt-in: `http`, and an address that is not public.
    pub fn new(
        repository: R,
        cipher: C,
        queue: Q,
        secrets: S,
        allow_private_targets: bool,
    ) -> Self {
        Self {
            repository,
            cipher,
            queue,
            secrets,
            allow_private_targets,
        }
    }

    /// Whether secrets can be sealed: without the key of the installation webhooks are off.
    pub fn is_available(&self) -> bool {
        self.cipher.is_available()
    }

    pub async fn create(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: WebhookSubscriptionDraft,
        now: i64,
    ) -> Result<Subscribed, ApplicationError> {
        self.ensure_available()?;
        if self.repository.list(company_id).await?.len() >= MAX_WEBHOOK_SUBSCRIPTIONS {
            return Err(DomainError::TooManyWebhookSubscriptions.into());
        }
        let subscription = WebhookSubscription::new(
            WebhookSubscriptionId::generate(),
            company_id,
            draft,
            self.allow_private_targets,
            actor.clone(),
            now,
        )?;
        let secret = self.generate_secret()?;
        let sealed = self.seal(&subscription, &secret)?;
        let audit = audit_entry(
            &subscription,
            actor,
            AuditAction::Created,
            webhook_subscription_creation_changes(&subscription),
        );
        self.repository.save(&subscription, &sealed, &audit).await?;
        Ok(Subscribed {
            subscription,
            secret: secret.expose().to_owned(),
        })
    }

    /// Replaces the address, the events and whether it is on. A draft that changes nothing writes nothing.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
        draft: WebhookSubscriptionDraft,
    ) -> Result<WebhookSubscription, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft, self.allow_private_targets)?;
        let changes = webhook_subscription_changes(&before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(missing)?;
        Ok(after)
    }

    /// Whether an address could be the one of a subscription here, without making one.
    pub fn check_address(&self, address: &str) -> Result<(), ApplicationError> {
        Ok(WebhookSubscription::check_address(
            address,
            self.allow_private_targets,
        )?)
    }

    /// Switches a subscription off, keeping its address and events. It never fails on the address, which is not looked at again.
    pub async fn switch_off(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
    ) -> Result<WebhookSubscription, ApplicationError> {
        let before = self.find(company_id, id).await?;
        if !before.is_active() {
            return Ok(before);
        }
        let after = before.switched_off();
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            webhook_subscription_changes(&before, &after),
        );
        self.repository
            .update(&after, &audit)
            .await
            .map_err(missing)?;
        Ok(after)
    }

    /// Replaces the secret with the one typed (`Some`, never given back) or with a new one made here (`None`, given back once).
    /// The one that was replaced goes on signing for 24 hours, so a receiver can switch without a gap.
    pub async fn replace_secret(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: WebhookSubscriptionId,
        typed: Option<&str>,
        now: i64,
    ) -> Result<Option<String>, ApplicationError> {
        self.ensure_available()?;
        let subscription = self.find(company_id, id).await?;
        let (secret, shown) = match typed {
            Some(raw) => (SecretText::new(raw)?, None),
            None => {
                let secret = self.generate_secret()?;
                let shown = secret.expose().to_owned();
                (secret, Some(shown))
            }
        };
        let sealed = self.seal(&subscription, &secret)?;
        let audit = audit_entry(
            &subscription,
            actor,
            AuditAction::Updated,
            vec![webhook_secret_change()],
        );
        self.repository
            .replace_secret(&subscription, &sealed, now + SECRET_GRACE_MILLIS, &audit)
            .await
            .map_err(missing)?;
        Ok(shown)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<WebhookSubscription>, ApplicationError> {
        Ok(self.repository.list(company_id).await?)
    }

    pub async fn history(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's subscription answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn deliveries(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
        after: Option<&DeliveryCursor>,
        limit: Option<usize>,
    ) -> Result<DeliveryPage, ApplicationError> {
        self.find(company_id, id).await?;
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_deliveries(company_id, id, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(DeliveryPage { items, next })
    }

    /// Sends a `webhook.test` to one subscription, on or off, so the person sees that it works. It is a delivery like any other:
    /// it is in the log, signed, tried and retried.
    pub async fn send_test(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
        now: i64,
    ) -> Result<WebhookDelivery, ApplicationError> {
        let subscription = self.find(company_id, id).await?;
        let event = Uuid::new_v4();
        let body = envelope(
            event,
            WebhookEventKind::Test,
            company_id,
            now,
            r#"{"message":"This is a test event from conta-verde."}"#,
        );
        let delivery = self
            .repository
            .deliver_to(&subscription, event, WebhookEventKind::Test, &body, now)
            .await?
            .ok_or(ApplicationError::WebhookNotFound)?;
        self.enqueue(&delivery, now).await?;
        Ok(delivery)
    }

    /// Sends a delivery again, by hand: it goes back to pending with no tries and the same body and event id.
    pub async fn redeliver(
        &self,
        company_id: CompanyId,
        subscription_id: WebhookSubscriptionId,
        id: WebhookDeliveryId,
        now: i64,
    ) -> Result<WebhookDelivery, ApplicationError> {
        let delivery = self
            .repository
            .reset_delivery(company_id, subscription_id, id, now)
            .await?
            .ok_or(ApplicationError::WebhookDeliveryNotFound)?;
        self.enqueue(&delivery, now).await?;
        Ok(delivery)
    }

    /// Tells the subscribers of the company of an event: one delivery for each that wants it, once however many times the same
    /// event is published, and a job for each to send it. `body` is the whole [`envelope`].
    pub async fn publish(
        &self,
        company_id: CompanyId,
        event: Uuid,
        kind: WebhookEventKind,
        body: &str,
        now: i64,
    ) -> Result<usize, ApplicationError> {
        let made = self
            .repository
            .publish(company_id, event, kind, body, now)
            .await?;
        for delivery in &made {
            self.enqueue(delivery, now).await?;
        }
        Ok(made.len())
    }

    /// Does the deliveries that are due at `now_seconds`, at most `limit`, one after the other. A try that fails waits as long
    /// as the kind says; the last one gives the delivery up (`failed`). Run by several workers at once, so a slow subscriber
    /// holds up only its own.
    pub async fn run_due(
        &self,
        now_seconds: i64,
        limit: usize,
        sender: &impl WebhookSender,
    ) -> Result<JobReport, ApplicationError> {
        let mut report = JobReport::default();
        for _ in 0..limit {
            let Some(job) = self
                .queue
                .claim_next_of(now_seconds, LEASE_SECONDS, &[JobKind::DeliverWebhook])
                .await?
            else {
                break;
            };
            let now = now_seconds.saturating_mul(1000);
            // A worker that died on this job every time it tried never got to say so: it has had its tries.
            if job.attempts > job.kind.max_attempts() {
                if let Some(id) = job
                    .args
                    .first()
                    .and_then(|text| Uuid::parse_str(text).ok())
                    .map(WebhookDeliveryId::from_uuid)
                {
                    self.give_up(id, now).await?;
                }
                self.queue
                    .fail(job.id, "gave up after repeated crashes", None)
                    .await?;
                report.dead += 1;
                continue;
            }
            let delivery_id = job
                .args
                .first()
                .and_then(|text| Uuid::parse_str(text).ok())
                .map(WebhookDeliveryId::from_uuid);
            let tried = match delivery_id {
                Some(id) => self.try_delivery(id, now, sender).await,
                None => Tried::Gone,
            };
            match tried {
                Tried::Done | Tried::Gone => {
                    self.queue.complete(job.id).await?;
                    report.done += 1;
                }
                Tried::Failed(reason) => {
                    let retry_at = job.retry_after().map(|wait| now_seconds + wait);
                    if retry_at.is_none() {
                        // The last try: the delivery says so, and the job is kept without its arguments.
                        if let Some(id) = delivery_id {
                            self.give_up(id, now).await?;
                        }
                        report.dead += 1;
                    } else {
                        report.retried += 1;
                    }
                    self.queue.fail(job.id, &reason, retry_at).await?;
                }
            }
        }
        Ok(report)
    }

    /// Queues again the pending deliveries that have no job (a failure between making a delivery and queuing it left them): the
    /// ones nobody has written to for two hours, longer than any wait of a retry. Each is written to, so it is not queued again
    /// next hour. Gives how many were queued.
    pub async fn requeue_stale(&self, now: i64) -> Result<usize, ApplicationError> {
        const STALE_MILLIS: i64 = 2 * 3_600_000;
        let stale = self
            .repository
            .stale_pending(now - STALE_MILLIS, 100)
            .await?;
        for delivery in &stale {
            self.enqueue(delivery, now).await?;
            self.repository
                .record_attempt(
                    delivery.id,
                    DeliveryStatus::Pending,
                    delivery.attempts,
                    delivery.last_status,
                    delivery.last_error.as_deref(),
                    now,
                )
                .await?;
        }
        Ok(stale.len())
    }

    /// Whether any subscription of the company wants an event of this kind: one small read, so that a company with no webhooks
    /// costs a write only that.
    pub async fn is_wanted(
        &self,
        company_id: CompanyId,
        kind: WebhookEventKind,
    ) -> Result<bool, ApplicationError> {
        Ok(self
            .repository
            .list(company_id)
            .await?
            .iter()
            .any(|subscription| subscription.wants(kind)))
    }

    /// Deletes what is older than 30 days from the log.
    pub async fn prune(&self, now: i64) -> Result<u64, ApplicationError> {
        const KEPT_MILLIS: i64 = 30 * 24 * 3_600_000;
        Ok(self.repository.prune_before(now - KEPT_MILLIS).await?)
    }

    async fn try_delivery(
        &self,
        id: WebhookDeliveryId,
        now: i64,
        sender: &impl WebhookSender,
    ) -> Tried {
        match self.attempt(id, now, sender).await {
            Ok(tried) => tried,
            Err(error) => {
                // A storage failure: the job tries again, and the log is not written (there is nothing to write it with).
                tracing::error!(%error, "a webhook delivery could not be tried");
                Tried::Failed("storage error".to_owned())
            }
        }
    }

    async fn attempt(
        &self,
        id: WebhookDeliveryId,
        now: i64,
        sender: &impl WebhookSender,
    ) -> Result<Tried, ApplicationError> {
        let Some(delivery) = self.repository.find_delivery(id).await? else {
            // Pruned or never made: nothing to send.
            return Ok(Tried::Gone);
        };
        if delivery.status != DeliveryStatus::Pending {
            // Sent meanwhile (a job that ran twice): at least once is not more than once when it can be told.
            return Ok(Tried::Done);
        }
        let attempts = delivery.attempts + 1;
        let subscription = self
            .repository
            .find(delivery.company_id, delivery.subscription_id)
            .await?;
        let Some(subscription) =
            subscription.filter(|s| s.is_active() || delivery.kind == WebhookEventKind::Test)
        else {
            self.record(
                &delivery,
                DeliveryStatus::Failed,
                attempts,
                None,
                "the subscription is off or gone",
                now,
            )
            .await?;
            return Ok(Tried::Gone);
        };
        let secrets = match self.open_secrets(&subscription, now).await {
            Ok(secrets) => secrets,
            Err(reason) => {
                self.record(
                    &delivery,
                    DeliveryStatus::Pending,
                    attempts,
                    None,
                    reason,
                    now,
                )
                .await?;
                return Ok(Tried::Failed(reason.to_owned()));
            }
        };
        let outcome = sender
            .send(&Outgoing {
                url: subscription.url(),
                event_id: delivery.event_id,
                timestamp: now / 1000,
                body: &delivery.payload,
                secrets: &secrets,
            })
            .await;
        if outcome.is_success() {
            self.record(
                &delivery,
                DeliveryStatus::Delivered,
                attempts,
                outcome.status,
                "",
                now,
            )
            .await?;
            return Ok(Tried::Done);
        }
        let reason = delivery_error(outcome.error.as_deref().unwrap_or("failed"));
        self.record(
            &delivery,
            DeliveryStatus::Pending,
            attempts,
            outcome.status,
            &reason,
            now,
        )
        .await?;
        Ok(Tried::Failed(reason))
    }

    async fn record(
        &self,
        delivery: &WebhookDelivery,
        status: DeliveryStatus,
        attempts: u32,
        last_status: Option<u16>,
        error: &str,
        now: i64,
    ) -> Result<(), ApplicationError> {
        let error = (!error.is_empty()).then_some(error);
        self.repository
            .record_attempt(delivery.id, status, attempts, last_status, error, now)
            .await?;
        Ok(())
    }

    async fn give_up(&self, id: WebhookDeliveryId, now: i64) -> Result<(), ApplicationError> {
        let pending = self
            .repository
            .find_delivery(id)
            .await?
            .filter(|delivery| delivery.status == DeliveryStatus::Pending);
        if let Some(delivery) = pending {
            self.repository
                .record_attempt(
                    id,
                    DeliveryStatus::Failed,
                    delivery.attempts,
                    delivery.last_status,
                    delivery.last_error.as_deref(),
                    now,
                )
                .await?;
        }
        Ok(())
    }

    /// The secrets that sign now: the current one and, while it still counts, the one that was replaced.
    async fn open_secrets(
        &self,
        subscription: &WebhookSubscription,
        now: i64,
    ) -> Result<Vec<SecretText>, &'static str> {
        let stored = self
            .repository
            .secrets_of(subscription.company_id(), subscription.id())
            .await
            .map_err(|_| "storage error")?
            .ok_or("the subscription has no secret")?;
        let field = field_of(subscription.id());
        let mut opened = vec![
            self.cipher
                .open(subscription.company_id(), &field, stored.current.as_bytes())
                .map_err(
                    |_| "the secret cannot be opened: the server's key changed; replace the secret",
                )?,
        ];
        if let Some((previous, until)) = stored.previous
            && now < until
            && let Ok(secret) =
                self.cipher
                    .open(subscription.company_id(), &field, previous.as_bytes())
        {
            opened.push(secret);
        }
        Ok(opened)
    }

    async fn enqueue(&self, delivery: &WebhookDelivery, now: i64) -> Result<(), ApplicationError> {
        let job = NewJob::new(
            JobKind::DeliverWebhook,
            vec![delivery.id.as_uuid().to_string()],
            now.div_euclid(1000),
        )?;
        self.queue.enqueue(&job).await?;
        Ok(())
    }

    fn generate_secret(&self) -> Result<SecretText, ApplicationError> {
        Ok(SecretText::new(&format!(
            "whsec_{}",
            self.secrets.new_token()
        ))?)
    }

    fn seal(
        &self,
        subscription: &WebhookSubscription,
        secret: &SecretText,
    ) -> Result<SealedSecret, ApplicationError> {
        let bytes = self.cipher.seal(
            subscription.company_id(),
            &field_of(subscription.id()),
            secret,
        )?;
        Ok(SealedSecret::new(bytes)?)
    }

    fn ensure_available(&self) -> Result<(), ApplicationError> {
        if self.cipher.is_available() {
            Ok(())
        } else {
            Err(ApplicationError::WebhooksOff)
        }
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<WebhookSubscription, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::WebhookNotFound)
    }
}

/// What a try came to, for the queue.
enum Tried {
    /// Delivered, or not pending any more.
    Done,
    /// The delivery or its subscription is gone or off: nothing to retry.
    Gone,
    /// To be tried again later, or given up on if that was the last try.
    Failed(String),
}

/// The field the secret is sealed for: the cipher binds it, with the company, so a secret moved to another subscription does
/// not open.
fn field_of(id: WebhookSubscriptionId) -> String {
    format!("webhook_secret:{}", id.as_uuid())
}

fn missing(error: RepositoryError) -> ApplicationError {
    match error {
        // It vanished between the read and the write.
        RepositoryError::NotFound => ApplicationError::WebhookNotFound,
        other => other.into(),
    }
}
