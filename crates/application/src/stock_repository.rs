use std::future::Future;

use conta_verde_domain::{
    AuditEntry, CalendarDate, CompanyId, Decimal, ProductId, StockBalance, StockMovement,
    StockMovementId, StockMovementKind, StockOriginKind,
};
use uuid::Uuid;

use crate::{DateCursor, RepositoryError};

/// How low a write may take the stock of a product ([ADR 0054]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockFloor {
    /// Below zero is allowed: a sale made without a balance leaves it negative.
    Open,
    /// The balance of every product stays at zero or more, or the write is refused.
    Zero,
}

/// What moved of a product over a span, by kind: signed like the ledger, so what left is below zero. A movement that
/// undoes another has the opposite kind and counts under it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MovementTotals {
    pub entered_quantity: Decimal,
    pub entered_value: Decimal,
    pub exited_quantity: Decimal,
    pub exited_value: Decimal,
    pub adjusted_quantity: Decimal,
    pub adjusted_value: Decimal,
}

/// How the ledger is narrowed, following ADR 0018. It is listed newest day first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StockMovementQuery {
    product: Option<ProductId>,
    kind: Option<StockMovementKind>,
    from: Option<CalendarDate>,
    to: Option<CalendarDate>,
}

impl StockMovementQuery {
    pub fn of_product(mut self, product: ProductId) -> Self {
        self.product = Some(product);
        self
    }

    pub fn of_kind(mut self, kind: StockMovementKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// From a day on, the day included.
    pub fn from(mut self, day: CalendarDate) -> Self {
        self.from = Some(day);
        self
    }

    /// Up to a day, the day included.
    pub fn to(mut self, day: CalendarDate) -> Self {
        self.to = Some(day);
        self
    }

    /// The same product and days with any kind, which is what a count per kind needs.
    pub fn any_kind(mut self) -> Self {
        self.kind = None;
        self
    }

    pub fn product(&self) -> Option<ProductId> {
        self.product
    }

    pub fn kind(&self) -> Option<StockMovementKind> {
        self.kind
    }

    pub fn first_day(&self) -> Option<CalendarDate> {
        self.from
    }

    pub fn last_day(&self) -> Option<CalendarDate> {
        self.to
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, movement: &StockMovement) -> bool {
        self.product
            .is_none_or(|product| movement.product_id() == product)
            && self.kind.is_none_or(|kind| movement.kind() == kind)
            && self.from.is_none_or(|from| movement.occurred_on() >= from)
            && self.to.is_none_or(|to| movement.occurred_on() <= to)
    }
}

impl From<&StockMovement> for DateCursor {
    fn from(movement: &StockMovement) -> Self {
        Self {
            date: movement.occurred_on(),
            id: movement.id().as_uuid(),
        }
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// The ledger is written only through `record`, which stores the movements and their audit entries **atomically**
/// ([ADR 0008]).
pub trait StockMovementRepository: Send + Sync {
    /// Stores the movements, in order, with their audit entries, all or none, after checking with the stock of the
    /// products locked that:
    /// - each product still has the balance in `seen` (`StockChanged`: the use case valued the movements against
    ///   another, read again and try again);
    /// - with `StockFloor::Zero`, no product ends below zero (`StockInsufficient`);
    /// - no movement repeats an origin that has its movement (`DuplicateOrigin`), and none reverses what was
    ///   reversed already (`DuplicateReversal`).
    fn record(
        &self,
        company_id: CompanyId,
        movements: &[StockMovement],
        seen: &[(ProductId, StockBalance)],
        floor: StockFloor,
        audits: &[AuditEntry],
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What each of these products has, through `on` when it is given (every movement otherwise), in the order of
    /// `products`; a product with no movement has the empty balance. Another company's product is empty too.
    fn balances(
        &self,
        company_id: CompanyId,
        products: &[ProductId],
        on: Option<CalendarDate>,
    ) -> impl Future<Output = Result<Vec<(ProductId, StockBalance)>, RepositoryError>> + Send;

    /// What moved of each product from a day through a day, both included, by kind. Only products with a movement in
    /// the span are listed, in no particular order.
    fn totals_by_product(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> impl Future<Output = Result<Vec<(ProductId, MovementTotals)>, RepositoryError>> + Send;

    /// The movement, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: StockMovementId,
    ) -> impl Future<Output = Result<Option<StockMovement>, RepositoryError>> + Send;

    /// Every movement an event made, reversals included, oldest first.
    fn of_origin(
        &self,
        company_id: CompanyId,
        kind: StockOriginKind,
        id: Uuid,
    ) -> impl Future<Output = Result<Vec<StockMovement>, RepositoryError>> + Send;

    /// The movement that undoes this one, if one was written.
    fn reversal_of(
        &self,
        company_id: CompanyId,
        id: StockMovementId,
    ) -> impl Future<Output = Result<Option<StockMovement>, RepositoryError>> + Send;

    /// At most `limit` movements of the company that match `query`, newest day first, starting right after `after`.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &StockMovementQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<StockMovement>, RepositoryError>> + Send;

    /// How many movements of the company match `query` of each kind, ignoring the query's own kind; kinds with
    /// none are left out.
    fn count_by_kind(
        &self,
        company_id: CompanyId,
        query: &StockMovementQuery,
    ) -> impl Future<Output = Result<Vec<(StockMovementKind, u64)>, RepositoryError>> + Send;
}
