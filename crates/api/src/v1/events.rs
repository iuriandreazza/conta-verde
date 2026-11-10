//! What a subscriber is told ([ADR 0088]): the events the adapters raise, written as the `v1` API writes the resource, wrapped in
//! the envelope of the webhook and handed to the subscriptions that want them.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use conta_verde_application::{
    DomainEvent, EventPublisher, FinancialEntryRepository, JobQueue, ProductRepository,
    SaleClaimRepository, SecretCipher, SecretService, WebhookRepository, WebhookService, envelope,
    event_id,
};
use conta_verde_domain::{Charge, EntryOrigin, WebhookEventKind};
use serde::Serialize;
use uuid::Uuid;

use crate::v1::V1Clock;
use crate::v1::resources::{V1Charge, V1Sale, V1Stock};

/// The object of a `charge.paid`: the charge, what it collects, and the sale when the receivable is a sale's.
#[derive(Serialize)]
struct V1ChargePaid {
    /// The sale the charge collects, when the receivable is the sale's own.
    sale_id: Option<Uuid>,
    /// The receivable (an entry) the charge is of.
    receivable_id: Uuid,
    charge: V1Charge,
}

/// The publisher the adapters of sales, charges and stock hold. It needs what writing the bodies needs: the references of
/// orders, the entries (to find the sale of a charge) and the products (for their minimum).
pub struct V1Events<R, C, Q, S, K, E, P> {
    webhooks: Arc<WebhookService<R, C, Q, S>>,
    claims: K,
    entries: E,
    products: P,
    clock: V1Clock,
}

impl<R, C, Q, S, K, E, P> V1Events<R, C, Q, S, K, E, P> {
    pub fn new(
        webhooks: Arc<WebhookService<R, C, Q, S>>,
        claims: K,
        entries: E,
        products: P,
        clock: V1Clock,
    ) -> Self {
        Self {
            webhooks,
            claims,
            entries,
            products,
            clock,
        }
    }
}

impl<R, C, Q, S, K, E, P> V1Events<R, C, Q, S, K, E, P>
where
    R: WebhookRepository,
    C: SecretCipher,
    Q: JobQueue,
    S: SecretService,
    K: SaleClaimRepository,
    E: FinancialEntryRepository,
    P: ProductRepository,
{
    async fn tell(&self, event: DomainEvent) -> Result<(), String> {
        let now = self.clock.now();
        // A company that has no subscription for this kind costs this one read and nothing else: no reference, no entry, no
        // product (a sale of a hundred lines would otherwise read a hundred products).
        let kind = match &event {
            DomainEvent::Sale { kind, .. } => *kind,
            DomainEvent::ChargePaid(_) => WebhookEventKind::ChargePaid,
            DomainEvent::StockMoved { .. } => WebhookEventKind::StockBelowMinimum,
        };
        let company_id = match &event {
            DomainEvent::Sale { sale, .. } => sale.company_id(),
            DomainEvent::ChargePaid(charge) => charge.company_id(),
            DomainEvent::StockMoved { company_id, .. } => *company_id,
        };
        if !self
            .webhooks
            .is_wanted(company_id, kind)
            .await
            .map_err(|error| error.to_string())?
        {
            return Ok(());
        }
        match event {
            DomainEvent::Sale { kind, sale } => {
                let reference = self
                    .claims
                    .reference_of(sale.company_id(), sale.id())
                    .await
                    .map_err(|error| error.to_string())?;
                let object = json(&V1Sale::new(&sale, reference, None))?;
                let id = event_id(sale.company_id(), kind, sale.id().as_uuid(), "");
                self.send(sale.company_id(), id, kind, &object, now).await
            }
            DomainEvent::ChargePaid(charge) => {
                let object = json(&self.charge_paid(&charge).await?)?;
                let id = event_id(
                    charge.company_id(),
                    WebhookEventKind::ChargePaid,
                    charge.id().as_uuid(),
                    "",
                );
                self.send(
                    charge.company_id(),
                    id,
                    WebhookEventKind::ChargePaid,
                    &object,
                    now,
                )
                .await
            }
            DomainEvent::StockMoved {
                company_id,
                product_id,
                movement_id,
                before,
                after,
            } => {
                let product = self
                    .products
                    .find(company_id, product_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let Some(product) = product else {
                    return Ok(());
                };
                let Some(minimum) = product.minimum_stock() else {
                    return Ok(());
                };
                // Only the move that takes a product below its minimum is news.
                let minimum = minimum.as_decimal();
                if !(before >= minimum && after < minimum) {
                    return Ok(());
                }
                let object = json(&V1Stock::new(
                    product_id.as_uuid(),
                    after,
                    product.minimum_stock().map(|minimum| minimum.to_string()),
                ))?;
                let id = event_id(
                    company_id,
                    WebhookEventKind::StockBelowMinimum,
                    product_id.as_uuid(),
                    &movement_id.to_string(),
                );
                self.send(
                    company_id,
                    id,
                    WebhookEventKind::StockBelowMinimum,
                    &object,
                    now,
                )
                .await
            }
        }
    }

    async fn charge_paid(&self, charge: &Charge) -> Result<V1ChargePaid, String> {
        let entry = self
            .entries
            .find(charge.company_id(), charge.entry_id())
            .await
            .map_err(|error| error.to_string())?;
        let sale_id = entry.and_then(|entry| match entry.origin() {
            Some(EntryOrigin::Sale(sale)) => Some(sale.as_uuid()),
            _ => None,
        });
        Ok(V1ChargePaid {
            sale_id,
            receivable_id: charge.entry_id().as_uuid(),
            charge: V1Charge::from(charge),
        })
    }

    async fn send(
        &self,
        company_id: conta_verde_domain::CompanyId,
        id: Uuid,
        kind: WebhookEventKind,
        object: &str,
        now: i64,
    ) -> Result<(), String> {
        let body = envelope(id, kind, company_id, now, object);
        self.webhooks
            .publish(company_id, id, kind, &body, now)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

impl<R, C, Q, S, K, E, P> EventPublisher for V1Events<R, C, Q, S, K, E, P>
where
    R: WebhookRepository + 'static,
    C: SecretCipher + 'static,
    Q: JobQueue + 'static,
    S: SecretService + 'static,
    K: SaleClaimRepository + 'static,
    E: FinancialEntryRepository + 'static,
    P: ProductRepository + 'static,
{
    fn publish(&self, event: DomainEvent) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            // The write this follows is done and stays done: an event that cannot be told is logged and lost, and a subscriber
            // reconciles with the reads of the API.
            if let Err(error) = self.tell(event).await {
                tracing::error!(%error, "a webhook event could not be published");
            }
        })
    }
}
