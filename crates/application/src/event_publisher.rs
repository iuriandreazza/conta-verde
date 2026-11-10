use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use conta_verde_domain::{Charge, CompanyId, Decimal, ProductId, Sale, WebhookEventKind};
use uuid::Uuid;

/// Something that happened to the company's books that a subscriber may be told about ([ADR 0088]). The adapters raise it
/// **after** the write that made it succeeded, with the record as it was written; what it becomes in a body is the API's.
#[derive(Debug, Clone)]
pub enum DomainEvent {
    /// A sale was made, completed or cancelled: `kind` says which.
    Sale {
        kind: WebhookEventKind,
        sale: Sale,
    },
    ChargePaid(Charge),
    /// Stock moved for a product: the quantity it had and the one it has. Whether that crosses its minimum is for the
    /// publisher to say, since the minimum is the product's.
    StockMoved {
        company_id: CompanyId,
        product_id: ProductId,
        /// One of the movements, for an id that is the same however many times it is raised.
        movement_id: Uuid,
        before: Decimal,
        after: Decimal,
    },
}

/// Port to what takes an event and tells the subscribers. It cannot fail: an event that cannot be stored is logged and lost
/// (the write it follows is not undone), and a subscriber reconciles with the API's reads ([ADR 0088]).
pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: DomainEvent) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// What the adapters hold: shared, and absent when nobody is subscribed to anything (tests, a company with no webhooks).
pub type SharedEvents = Arc<dyn EventPublisher>;

/// The events of a write to the stock ledger: for each product the writer saw (the books it valued the movements against), the
/// quantity it had and the one it has after the movements, with the id of the first of its movements. A product that does not
/// move gives none.
pub fn stock_events(
    company_id: CompanyId,
    movements: &[conta_verde_domain::StockMovement],
    seen: &[(ProductId, conta_verde_domain::StockBalance)],
) -> Vec<DomainEvent> {
    seen.iter()
        .filter_map(|(product_id, before)| {
            let mine: Vec<_> = movements
                .iter()
                .filter(|movement| movement.product_id() == *product_id)
                .collect();
            let first = mine.first()?;
            let after = mine
                .iter()
                .fold(*before, |books, movement| books.after(movement));
            Some(DomainEvent::StockMoved {
                company_id,
                product_id: *product_id,
                movement_id: first.id().as_uuid(),
                before: before.quantity(),
                after: after.quantity(),
            })
        })
        .collect()
}
