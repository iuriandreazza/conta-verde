use std::future::Future;

use conta_verde_domain::{CompanyId, PaymentEvent, PaymentEventId, PaymentEventOutcome};

use crate::RepositoryError;

/// What storing a notice found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// First time the provider's id was seen: this is the stored event, pending.
    New(PaymentEvent),
    /// The provider sent it before: this is what was stored then, with what came of it.
    Existing(PaymentEvent),
}

/// Port for the notices payment providers send ([ADR 0046]). A log, not a register: nothing here is
/// audited, and a notice is never edited, only finished.
pub trait PaymentEventRepository: Send + Sync {
    /// Stores the event unless the company already has one with the same provider and provider id, which is
    /// returned instead. Two requests with the same notice at once get one `New` and one `Existing`.
    fn record(
        &self,
        event: &PaymentEvent,
    ) -> impl Future<Output = Result<Recorded, RepositoryError>> + Send;

    /// Says what came of a stored event. `NotFound` when the company has no such event.
    fn finish(
        &self,
        company_id: CompanyId,
        id: PaymentEventId,
        outcome: PaymentEventOutcome,
        note: Option<&str>,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
}
