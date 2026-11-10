use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CompanyId, DeliveryStatus, SealedSecret, WebhookDelivery,
    WebhookDeliveryId, WebhookEventKind, WebhookSubscription, WebhookSubscriptionId,
};
use uuid::Uuid;

use crate::RepositoryError;

/// The sealed secrets of a subscription: the one that signs, and the one that was replaced and still signs until `until`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookSecrets {
    pub current: SealedSecret,
    /// The replaced secret and the moment (Unix milliseconds) it stops signing.
    pub previous: Option<(SealedSecret, i64)>,
}

/// Where a page of deliveries goes on: the newest first, so `created_at` then the id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryCursor {
    pub created_at: i64,
    pub id: Uuid,
}

impl From<&WebhookDelivery> for DeliveryCursor {
    fn from(delivery: &WebhookDelivery) -> Self {
        Self {
            created_at: delivery.created_at,
            id: delivery.id.as_uuid(),
        }
    }
}

/// Port to the subscriptions of a company and the log of what was sent to them ([ADR 0088]).
///
/// Writes to a subscription take the audit entry that describes them and store both **atomically** ([ADR 0008]); the deliveries
/// are an operational log and have no audit entry. Time is Unix milliseconds from the caller, never the database's clock.
pub trait WebhookRepository: Send + Sync {
    /// Stores a new subscription with its sealed secret.
    fn save(
        &self,
        subscription: &WebhookSubscription,
        secret: &SealedSecret,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Writes the address, the events and whether it is on. `NotFound` when the company has no such subscription.
    fn update(
        &self,
        subscription: &WebhookSubscription,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Puts `secret` in the place of the current one, which goes on signing until `previous_until`. `NotFound` when the company
    /// has no such subscription.
    fn replace_secret(
        &self,
        subscription: &WebhookSubscription,
        secret: &SealedSecret,
        previous_until: i64,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn find(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> impl Future<Output = Result<Option<WebhookSubscription>, RepositoryError>> + Send;

    /// The sealed secrets, for the one use that signs with them.
    fn secrets_of(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> impl Future<Output = Result<Option<WebhookSecrets>, RepositoryError>> + Send;

    /// Oldest first. A company has at most [`conta_verde_domain::MAX_WEBHOOK_SUBSCRIPTIONS`], so there is no paging.
    fn list(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<WebhookSubscription>, RepositoryError>> + Send;

    fn history(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// Makes a pending delivery of the event for each **active subscription of the company that wants it**, in one statement
    /// per subscription, and gives back **only the deliveries that were made now**: the same `event_id` for the same
    /// subscription is made once, so publishing an event twice sends it once.
    fn publish(
        &self,
        company_id: CompanyId,
        event_id: Uuid,
        kind: WebhookEventKind,
        payload: &str,
        now: i64,
    ) -> impl Future<Output = Result<Vec<WebhookDelivery>, RepositoryError>> + Send;

    /// Makes a pending delivery of the event for this one subscription, on or off (a test is sent to a subscription that is
    /// off too). `None` when it was made before.
    fn deliver_to(
        &self,
        subscription: &WebhookSubscription,
        event_id: Uuid,
        kind: WebhookEventKind,
        payload: &str,
        now: i64,
    ) -> impl Future<Output = Result<Option<WebhookDelivery>, RepositoryError>> + Send;

    /// Of any company: the worker is given an id by the queue and nothing else.
    fn find_delivery(
        &self,
        id: WebhookDeliveryId,
    ) -> impl Future<Output = Result<Option<WebhookDelivery>, RepositoryError>> + Send;

    /// Says how a try went: the new status and attempts, the status code the subscriber answered and a short reason.
    fn record_attempt(
        &self,
        id: WebhookDeliveryId,
        status: DeliveryStatus,
        attempts: u32,
        last_status: Option<u16>,
        last_error: Option<&str>,
        now: i64,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Puts a delivery of the company back to pending with no tries, to be sent again by hand. `None` when the company has no
    /// such delivery of that subscription.
    fn reset_delivery(
        &self,
        company_id: CompanyId,
        subscription_id: WebhookSubscriptionId,
        id: WebhookDeliveryId,
        now: i64,
    ) -> impl Future<Output = Result<Option<WebhookDelivery>, RepositoryError>> + Send;

    /// Newest first.
    fn list_deliveries(
        &self,
        company_id: CompanyId,
        subscription_id: WebhookSubscriptionId,
        after: Option<&DeliveryCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<WebhookDelivery>, RepositoryError>> + Send;

    /// Deletes the deliveries made before the moment, giving back how many.
    fn prune_before(
        &self,
        before: i64,
    ) -> impl Future<Output = Result<u64, RepositoryError>> + Send;
}
