use std::collections::HashMap;
use std::future::Future;

use conta_verde_domain::{
    Actor, AuditAction, CalendarDate, CompanyId, Decimal, DomainError, InventoryCount,
    InventoryCountId, Product, ProductId, ProductKind, PurchaseOrder, PurchaseReceipt, Sale,
    SaleId, SaleReturn, StockBalance, StockMovement, StockMovementDraft, StockMovementId,
    StockMovementKind, StockOrigin, StockOriginKind, StockSettings, StockSituation, round_to_cents,
    stock_movement_creation_changes, stock_settings_changes, stock_settings_creation_changes,
};
use uuid::Uuid;

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, DateCursor, NameCursor, ProductQuery, ProductRepository, RepositoryError,
    StockFloor, StockMovementQuery, StockMovementRepository, StockSettingsRepository,
};
use conta_verde_domain::AuditRecord;

/// How many times a write is valued again when another one changed the stock under it.
const ATTEMPTS: usize = 5;

/// What a count of the stock asks of it ([ADR 0056]).
pub trait CountStock: Send + Sync {
    /// The product is one that has stock.
    fn ensure_stocked(
        &self,
        company_id: CompanyId,
        product_id: ProductId,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// What each product held through a day, in the order asked.
    fn quantities(
        &self,
        company_id: CompanyId,
        products: &[ProductId],
        on: CalendarDate,
    ) -> impl Future<Output = Result<Vec<(ProductId, Decimal)>, ApplicationError>> + Send;

    /// What a count adjusted, as `(line, quantity)`; a line that agreed with the stock has none.
    fn adjusted_by(
        &self,
        company_id: CompanyId,
        count_id: InventoryCountId,
    ) -> impl Future<Output = Result<Vec<(u16, Decimal)>, ApplicationError>> + Send;

    /// Writes the adjustments of a count, all or none, on the day of the count. A count that has its adjustments
    /// writes none again, so it is safe to ask twice.
    fn apply_count(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        count: &InventoryCount,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;
}

/// What sales and purchases ask of the stock ([ADR 0054]). Every step is idempotent: asking again writes only what
/// is missing, so a failure between a document and its stock is finished by asking again.
pub trait DocumentStock: Send + Sync {
    /// Whether the stock can cover the sale, when the company refuses a sale without balance: asked before the sale
    /// is completed so that a refusal changes nothing. It answers the same `StockInsufficient` the exits would.
    fn ensure_available_for_sale(
        &self,
        company_id: CompanyId,
        sale: &Sale,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// The exits of a completed sale: one per line that is a product, for the day of the sale.
    fn take_for_sale(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        sale: &Sale,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// Undoes the exits of a sale that is cancelled.
    fn put_back_cancelled_sale(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        sale_id: SaleId,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// The entries of a return, at the cost the sale took, given what was returned of each line before.
    fn put_back_return(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        sale: &Sale,
        sale_return: &SaleReturn,
        returned_before: &[(u16, Decimal)],
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// The entries of a receipt of goods, valued by what the receipt stored.
    fn receive_purchase(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        order: &PurchaseOrder,
        receipt: &PurchaseReceipt,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;
}

#[derive(Debug, Clone)]
pub struct StockMovementPage {
    pub items: Vec<StockMovement>,
    /// Present when more movements exist after this page.
    pub next: Option<DateCursor>,
}

/// A product and what it has.
#[derive(Debug, Clone)]
pub struct StockPosition {
    pub product: Product,
    pub balance: StockBalance,
}

impl From<&StockPosition> for NameCursor {
    fn from(position: &StockPosition) -> Self {
        Self::from(&position.product)
    }
}

impl StockPosition {
    pub fn is(&self, situation: StockSituation) -> bool {
        let minimum = self
            .product
            .minimum_stock()
            .map(|amount| amount.as_decimal());
        situation.holds(&self.balance, minimum)
    }
}

/// The products that match a search, counted by situation and what they are worth.
#[derive(Debug, Clone, Default)]
pub struct PositionSummary {
    pub total: u64,
    pub in_stock: u64,
    pub below_minimum: u64,
    pub depleted: u64,
    /// The value of what is on hand, in cents.
    pub value: Decimal,
}

#[derive(Debug, Clone)]
pub struct StockPositionPage {
    pub items: Vec<StockPosition>,
    /// Present when more products exist after this page.
    pub next: Option<NameCursor>,
}

/// A product that has less than its minimum.
#[derive(Debug, Clone)]
pub struct StockAlert {
    pub product: Product,
    pub quantity: Decimal,
}

/// A product in the valuation of the stock on a day.
#[derive(Debug, Clone)]
pub struct ValuationRow {
    pub product: Product,
    pub balance: StockBalance,
    /// Its part of the total value, in percent with at most two decimals; 0 when the total is not above 0.
    pub share: Decimal,
}

/// What the stock was worth on a day, by product and in all ([ADR 0058]).
#[derive(Debug, Clone)]
pub struct StockValuation {
    pub on: CalendarDate,
    /// The products that had something on hand or some value, biggest value first, then by name.
    pub rows: Vec<ValuationRow>,
    /// The sum of the values, in cents.
    pub total: Decimal,
    /// How many of the rows had less than nothing on hand.
    pub negative: usize,
}

/// What happened to a product's stock over a span, with what it had before and after.
#[derive(Debug, Clone)]
pub struct MovementSummary {
    pub product: Product,
    pub opening: StockBalance,
    /// What came in, as positive quantity and value.
    pub entered_quantity: Decimal,
    pub entered_value: Decimal,
    /// What went out, as positive quantity and value.
    pub exited_quantity: Decimal,
    pub exited_value: Decimal,
    /// What counts and adjustments added (negative when they took away).
    pub adjusted_quantity: Decimal,
    pub adjusted_value: Decimal,
    pub closing: StockBalance,
}

/// The longest span a movements report covers, in days.
const MAX_REPORT_DAYS: i64 = 366;

/// One movement to write, before it is valued.
struct NewMovement {
    draft: StockMovementDraft,
    origin: Option<StockOrigin>,
    /// When it is a count: what the product is found to hold, and the quantity of the draft is worked out against
    /// the balance read for the write (0 apart: nothing to write).
    counted: Option<Decimal>,
}

/// What a reversal undoes.
enum Undo {
    /// Every movement an event made.
    Origin(StockOriginKind, Uuid),
    /// One movement.
    One(StockMovementId),
}

/// The stock of a company ([ADR 0054]): the ledger of its movements, what each product has, and what the documents
/// do to it.
pub struct StockService<M, P, S> {
    movements: M,
    products: P,
    settings: S,
}

impl<M, P, S> StockService<M, P, S>
where
    M: StockMovementRepository,
    P: ProductRepository,
    S: StockSettingsRepository,
{
    pub fn new(movements: M, products: P, settings: S) -> Self {
        Self {
            movements,
            products,
            settings,
        }
    }

    /// How the company's stock behaves; a company that never said has the default.
    pub async fn settings(&self, company_id: CompanyId) -> Result<StockSettings, ApplicationError> {
        Ok(self
            .settings
            .find(company_id)
            .await?
            .unwrap_or_else(|| StockSettings::open(company_id)))
    }

    /// Replaces the settings; one that changes nothing writes nothing.
    pub async fn replace_settings(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        block_sale_without_balance: bool,
    ) -> Result<StockSettings, ApplicationError> {
        let before = self.settings.find(company_id).await?;
        let after = StockSettings::new(company_id, block_sale_without_balance);
        if before.as_ref() == Some(&after) {
            return Ok(after);
        }
        let (action, changes) = match &before {
            Some(stored) => (AuditAction::Updated, stock_settings_changes(stored, &after)),
            None => (
                AuditAction::Created,
                stock_settings_creation_changes(&after),
            ),
        };
        let audit = audit_entry(&after, actor, action, changes);
        self.settings.save(&after, &audit).await?;
        Ok(after)
    }

    /// What was recorded about the company's stock settings, oldest first.
    pub async fn settings_history(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        Ok(self.settings.history(company_id).await?)
    }

    /// The active products that have less than their minimum now, by name. A product with no minimum, a service and
    /// an inactive product are never listed.
    pub async fn alerts(&self, company_id: CompanyId) -> Result<Vec<StockAlert>, ApplicationError> {
        let query = ProductQuery::default()
            .of_kind(ProductKind::Product)
            .with_active(true);
        let mut alerts = Vec::new();
        let mut after: Option<NameCursor> = None;
        loop {
            let page = self
                .products
                .list_by_company(company_id, &query, after.as_ref(), 200 + 1)
                .await?;
            let (products, next): (Vec<Product>, Option<NameCursor>) = split_page(page, 200);
            let tracked: Vec<Product> = products
                .into_iter()
                .filter(|product| product.minimum_stock().is_some())
                .collect();
            let ids: Vec<ProductId> = tracked.iter().map(Product::id).collect();
            let balances = self.movements.balances(company_id, &ids, None).await?;
            for (product, (_, balance)) in tracked.into_iter().zip(balances) {
                let minimum = product
                    .minimum_stock()
                    .map_or(Decimal::ZERO, |m| m.as_decimal());
                if balance.quantity() < minimum {
                    alerts.push(StockAlert {
                        product,
                        quantity: balance.quantity(),
                    });
                }
            }
            match next {
                Some(cursor) => after = Some(cursor),
                None => return Ok(alerts),
            }
        }
    }

    /// A movement a person writes by hand. An exit or an adjustment down that would leave less than nothing is
    /// refused.
    pub async fn record(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: StockMovementDraft,
    ) -> Result<StockMovement, ApplicationError> {
        self.stocked(company_id, draft.product_id).await?;
        let written = self
            .write(
                company_id,
                actor,
                vec![NewMovement {
                    draft,
                    origin: None,
                    counted: None,
                }],
                StockFloor::Zero,
            )
            .await?;
        written
            .into_iter()
            .next()
            .ok_or(ApplicationError::StockChanged)
    }

    /// Undoes a movement a person wrote by hand, once; the day is the person's.
    pub async fn reverse(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: StockMovementId,
        day: &str,
        note: Option<String>,
    ) -> Result<StockMovement, ApplicationError> {
        let day = CalendarDate::parse(day).ok_or(DomainError::InvalidDate("occurred_on"))?;
        let original = self.get(company_id, id).await?;
        if original.reverses().is_some() {
            return Err(DomainError::StockMovementIsAReversal.into());
        }
        if original.origin().is_some() {
            return Err(ApplicationError::StockMovementFollowsADocument);
        }
        let written = self
            .undo(
                company_id,
                actor,
                &Undo::One(id),
                Some(day),
                note,
                StockFloor::Zero,
            )
            .await?;
        written
            .into_iter()
            .next()
            .ok_or(ApplicationError::Repository(
                RepositoryError::DuplicateReversal,
            ))
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        id: StockMovementId,
    ) -> Result<StockMovement, ApplicationError> {
        self.movements
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::StockMovementNotFound)
    }

    /// The ledger, newest day first.
    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &StockMovementQuery,
        after: Option<&DateCursor>,
        limit: Option<usize>,
    ) -> Result<StockMovementPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .movements
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(StockMovementPage { items, next })
    }

    /// What the products of a page have, through `on` when it is given (every movement otherwise), optionally only
    /// those in `situation`. Only products have stock: services are left out. The products are scanned in the
    /// order of the query until the page is full, so a situation that few products are in costs more reads.
    pub async fn positions(
        &self,
        company_id: CompanyId,
        query: ProductQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
        on: Option<CalendarDate>,
        situation: Option<StockSituation>,
    ) -> Result<StockPositionPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let query = query.of_kind(ProductKind::Product);
        let mut cursor = after.cloned();
        let mut found: Vec<StockPosition> = Vec::new();
        loop {
            let rows = self
                .products
                .list_by_company(company_id, &query, cursor.as_ref(), limit + 1)
                .await?;
            // The extra row is read too, so that `found` holding more than a page says exactly that more exists.
            let exhausted = rows.len() <= limit;
            cursor = rows.last().map(NameCursor::from);
            let ids: Vec<ProductId> = rows.iter().map(Product::id).collect();
            let balances = self.movements.balances(company_id, &ids, on).await?;
            found.extend(
                rows.into_iter()
                    .zip(balances)
                    .map(|(product, (_, balance))| StockPosition { product, balance })
                    .filter(|position| situation.is_none_or(|wanted| position.is(wanted))),
            );
            if exhausted || found.len() > limit {
                break;
            }
        }
        let (items, next) = split_page(found, limit);
        Ok(StockPositionPage { items, next })
    }

    /// How many of the products that match `query` are in each situation through `on`, and what they hold, for the
    /// tiles and the footer of the position. Every product is read, like the valuation.
    pub async fn position_summary(
        &self,
        company_id: CompanyId,
        query: ProductQuery,
        on: Option<CalendarDate>,
    ) -> Result<PositionSummary, ApplicationError> {
        let products = self.all_products(company_id, query).await?;
        let ids: Vec<ProductId> = products.iter().map(Product::id).collect();
        let balances = self.movements.balances(company_id, &ids, on).await?;
        let mut summary = PositionSummary::default();
        for (product, (_, balance)) in products.into_iter().zip(balances) {
            let position = StockPosition { product, balance };
            summary.total += 1;
            summary.in_stock += u64::from(position.is(StockSituation::InStock));
            summary.below_minimum += u64::from(position.is(StockSituation::BelowMinimum));
            summary.depleted += u64::from(position.is(StockSituation::Depleted));
            summary.value += position.balance.value();
        }
        summary.value = summary.value.normalize();
        Ok(summary)
    }

    /// Every product of kind product, by name: the products are read a page at a time.
    async fn all_products(
        &self,
        company_id: CompanyId,
        query: ProductQuery,
    ) -> Result<Vec<Product>, ApplicationError> {
        let query = query.of_kind(ProductKind::Product);
        let mut all = Vec::new();
        let mut after: Option<NameCursor> = None;
        loop {
            let page = self
                .products
                .list_by_company(company_id, &query, after.as_ref(), 200 + 1)
                .await?;
            let (products, next): (Vec<Product>, Option<NameCursor>) = split_page(page, 200);
            all.extend(products);
            match next {
                Some(cursor) => after = Some(cursor),
                None => return Ok(all),
            }
        }
    }

    /// What the stock was worth on a day: each product with something on hand or some value, and the total.
    pub async fn valuation(
        &self,
        company_id: CompanyId,
        on: CalendarDate,
    ) -> Result<StockValuation, ApplicationError> {
        let products = self
            .all_products(company_id, ProductQuery::default())
            .await?;
        let ids: Vec<ProductId> = products.iter().map(Product::id).collect();
        let balances = self.movements.balances(company_id, &ids, Some(on)).await?;
        let mut held: Vec<(Product, StockBalance)> = products
            .into_iter()
            .zip(balances)
            .map(|(product, (_, balance))| (product, balance))
            .filter(|(_, balance)| !balance.quantity().is_zero() || !balance.value().is_zero())
            .collect();
        held.sort_by(|(a, x), (b, y)| {
            y.value()
                .cmp(&x.value())
                .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase()))
                .then_with(|| a.id().as_uuid().cmp(&b.id().as_uuid()))
        });
        let total: Decimal = held.iter().map(|(_, balance)| balance.value()).sum();
        let negative = held
            .iter()
            .filter(|(_, balance)| balance.quantity() < Decimal::ZERO)
            .count();
        let rows = held
            .into_iter()
            .map(|(product, balance)| {
                let share = if total > Decimal::ZERO {
                    (balance.value() * Decimal::ONE_HUNDRED / total)
                        .round_dp(2)
                        .normalize()
                } else {
                    Decimal::ZERO
                };
                ValuationRow {
                    product,
                    balance,
                    share,
                }
            })
            .collect();
        Ok(StockValuation {
            on,
            rows,
            total: total.normalize(),
            negative,
        })
    }

    /// What moved of each product from a day through a day: what came in, went out and was adjusted, with what the
    /// product had the day before and at the end. Only products with a movement in the span are listed, by name.
    pub async fn movement_summary(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<Vec<MovementSummary>, ApplicationError> {
        if from > to {
            return Err(ApplicationError::InvalidSpan("from is after to"));
        }
        if to.to_unix_days() - from.to_unix_days() >= MAX_REPORT_DAYS {
            return Err(ApplicationError::InvalidSpan("at most 366 days"));
        }
        let totals = self
            .movements
            .totals_by_product(company_id, from, to)
            .await?;
        let moved: Vec<ProductId> = totals.iter().map(|(id, _)| *id).collect();
        let closing = self
            .movements
            .balances(company_id, &moved, Some(to))
            .await?;
        let opening = match from.pred() {
            Some(before) => {
                self.movements
                    .balances(company_id, &moved, Some(before))
                    .await?
            }
            None => moved.iter().map(|id| (*id, StockBalance::EMPTY)).collect(),
        };
        let products = self
            .all_products(company_id, ProductQuery::default())
            .await?;
        let mut rows = Vec::with_capacity(totals.len());
        for (((id, sums), (_, opened)), (_, closed)) in totals.into_iter().zip(opening).zip(closing)
        {
            let Some(product) = products.iter().find(|product| product.id() == id) else {
                continue;
            };
            rows.push(MovementSummary {
                product: product.clone(),
                opening: opened,
                entered_quantity: sums.entered_quantity.normalize(),
                entered_value: sums.entered_value.normalize(),
                exited_quantity: (-sums.exited_quantity).normalize(),
                exited_value: (-sums.exited_value).normalize(),
                adjusted_quantity: sums.adjusted_quantity.normalize(),
                adjusted_value: sums.adjusted_value.normalize(),
                closing: closed,
            });
        }
        rows.sort_by(|a, b| {
            a.product
                .name()
                .to_lowercase()
                .cmp(&b.product.name().to_lowercase())
                .then_with(|| a.product.id().as_uuid().cmp(&b.product.id().as_uuid()))
        });
        Ok(rows)
    }

    /// What a product has now.
    pub async fn balance_of(
        &self,
        company_id: CompanyId,
        product_id: ProductId,
    ) -> Result<StockBalance, ApplicationError> {
        self.stocked(company_id, product_id).await?;
        Ok(self
            .movements
            .balances(company_id, &[product_id], None)
            .await?
            .into_iter()
            .next()
            .map(|(_, balance)| balance)
            .unwrap_or(StockBalance::EMPTY))
    }

    /// The refusal for a product that has `on_hand` and is asked for more: it names the product.
    async fn short_of(
        &self,
        company_id: CompanyId,
        product_id: ProductId,
        on_hand: Decimal,
    ) -> ApplicationError {
        let name = match self.products.find(company_id, product_id).await {
            Ok(Some(product)) => product.name().to_owned(),
            _ => "the product".to_owned(),
        };
        ApplicationError::StockInsufficient {
            product: name,
            available: on_hand.max(Decimal::ZERO).normalize().to_string(),
        }
    }

    /// The product, if it is one that has stock.
    async fn stocked(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<Product, ApplicationError> {
        let product = self
            .products
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::ProductNotFound)?;
        if product.kind() == ProductKind::Service {
            return Err(DomainError::NotStocked.into());
        }
        Ok(product)
    }

    /// Whether a line's product has stock; one that is gone, or is a service, has none to move.
    async fn has_stock(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<bool, ApplicationError> {
        Ok(self
            .products
            .find(company_id, id)
            .await?
            .is_some_and(|product| product.kind() == ProductKind::Product))
    }

    /// Writes the movements, all or none, valued against the balances read just before. What already has its
    /// movement (an origin and a line) is left out, so the call is safe to repeat. Another write that changed a
    /// balance in between makes the movements be valued again.
    async fn write(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        items: Vec<NewMovement>,
        floor: StockFloor,
    ) -> Result<Vec<StockMovement>, ApplicationError> {
        let mut products: Vec<ProductId> = items.iter().map(|item| item.draft.product_id).collect();
        products.sort_by_key(|id| id.as_uuid());
        products.dedup();
        for _ in 0..ATTEMPTS {
            let mut made: Vec<(StockOriginKind, Uuid)> = Vec::new();
            for origin in items.iter().filter_map(|item| item.origin) {
                if !made.contains(&(origin.kind, origin.id)) {
                    made.push((origin.kind, origin.id));
                }
            }
            let mut existing: Vec<StockMovement> = Vec::new();
            for (kind, id) in made {
                existing.extend(self.movements.of_origin(company_id, kind, id).await?);
            }
            let seen = self.movements.balances(company_id, &products, None).await?;
            let mut books: HashMap<ProductId, StockBalance> = seen.iter().copied().collect();
            let mut movements: Vec<StockMovement> = Vec::new();
            for item in &items {
                let done = item.origin.is_some_and(|origin| {
                    existing.iter().any(|stored| {
                        stored.reverses().is_none() && stored.origin() == Some(origin)
                    })
                });
                if done {
                    continue;
                }
                let balance = books.entry(item.draft.product_id).or_default();
                let mut draft = item.draft.clone();
                if let Some(counted) = item.counted {
                    // A count says what was found on its day: the adjustment, booked on that day, is the difference to
                    // what the stock held **through that day**. Nothing is written when it already held it.
                    let day = CalendarDate::parse(&draft.occurred_on)
                        .ok_or(DomainError::InvalidDate("occurred_on"))?;
                    let held = self
                        .movements
                        .balances(company_id, &[draft.product_id], Some(day))
                        .await?
                        .first()
                        .map_or(Decimal::ZERO, |(_, held)| held.quantity());
                    let difference = counted - held;
                    if difference.is_zero() {
                        continue;
                    }
                    draft.quantity = difference.normalize().to_string();
                }
                let movement = StockMovement::new(
                    StockMovementId::generate(),
                    company_id,
                    draft,
                    item.origin,
                    balance,
                )?;
                *balance = balance.after(&movement);
                movements.push(movement);
            }
            if movements.is_empty() {
                return Ok(movements);
            }
            match self
                .record_valued(company_id, actor, &movements, &seen, &books, floor)
                .await
            {
                Ok(()) => return Ok(movements),
                Err(ApplicationError::Repository(
                    RepositoryError::StockChanged | RepositoryError::DuplicateOrigin,
                )) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(ApplicationError::StockChanged)
    }

    /// Undoes what `target` names that was not undone yet, all or none, with the same retry as a write.
    async fn undo(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        target: &Undo,
        day: Option<CalendarDate>,
        note: Option<String>,
        floor: StockFloor,
    ) -> Result<Vec<StockMovement>, ApplicationError> {
        for _ in 0..ATTEMPTS {
            let (originals, reversals) = match target {
                Undo::Origin(kind, id) => {
                    let all = self.movements.of_origin(company_id, *kind, *id).await?;
                    let (reversals, originals): (Vec<_>, Vec<_>) =
                        all.into_iter().partition(|m| m.reverses().is_some());
                    (originals, reversals)
                }
                Undo::One(id) => (
                    vec![self.get(company_id, *id).await?],
                    self.movements
                        .reversal_of(company_id, *id)
                        .await?
                        .into_iter()
                        .collect(),
                ),
            };
            let to_undo: Vec<&StockMovement> = originals
                .iter()
                .filter(|original| {
                    original.reverses().is_none()
                        && !reversals
                            .iter()
                            .any(|reversal| reversal.reverses() == Some(original.id()))
                })
                .collect();
            if to_undo.is_empty() {
                return Ok(Vec::new());
            }
            let mut products: Vec<ProductId> = to_undo.iter().map(|m| m.product_id()).collect();
            products.sort_by_key(|id| id.as_uuid());
            products.dedup();
            let seen = self.movements.balances(company_id, &products, None).await?;
            let mut books: HashMap<ProductId, StockBalance> = seen.iter().copied().collect();
            let mut movements = Vec::new();
            for original in to_undo {
                let movement = StockMovement::reversing(
                    StockMovementId::generate(),
                    original,
                    day.unwrap_or_else(|| original.occurred_on()),
                    note.clone(),
                )?;
                let balance = books.entry(movement.product_id()).or_default();
                *balance = balance.after(&movement);
                movements.push(movement);
            }
            match self
                .record_valued(company_id, actor, &movements, &seen, &books, floor)
                .await
            {
                Ok(()) => return Ok(movements),
                Err(ApplicationError::Repository(
                    RepositoryError::StockChanged | RepositoryError::DuplicateReversal,
                )) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(ApplicationError::StockChanged)
    }

    /// Stores valued movements, after refusing a floor they break with the name of the product and what it has.
    async fn record_valued(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        movements: &[StockMovement],
        seen: &[(ProductId, StockBalance)],
        books: &HashMap<ProductId, StockBalance>,
        floor: StockFloor,
    ) -> Result<(), ApplicationError> {
        if floor == StockFloor::Zero {
            for (product_id, before) in seen {
                let after = books.get(product_id).copied().unwrap_or(*before);
                if after.quantity() < Decimal::ZERO && after.quantity() < before.quantity() {
                    return Err(self
                        .short_of(company_id, *product_id, before.quantity())
                        .await);
                }
            }
        }
        let audits: Vec<_> = movements
            .iter()
            .map(|movement| {
                audit_entry(
                    movement,
                    actor,
                    AuditAction::Created,
                    stock_movement_creation_changes(movement),
                )
            })
            .collect();
        self.movements
            .record(company_id, movements, seen, floor, &audits)
            .await
            .map_err(Into::into)
    }
}

/// The unit of a line's quantity written as the draft's text.
fn quantity_text(quantity: Decimal) -> String {
    quantity.normalize().to_string()
}

impl<M, P, S> DocumentStock for StockService<M, P, S>
where
    M: StockMovementRepository,
    P: ProductRepository,
    S: StockSettingsRepository,
{
    async fn ensure_available_for_sale(
        &self,
        company_id: CompanyId,
        sale: &Sale,
    ) -> Result<(), ApplicationError> {
        if !self
            .settings(company_id)
            .await?
            .blocks_sales_without_balance()
        {
            return Ok(());
        }
        let mut wanted: Vec<(ProductId, Decimal)> = Vec::new();
        for line in sale.lines() {
            if !self.has_stock(company_id, line.product_id()).await? {
                continue;
            }
            match wanted.iter_mut().find(|(id, _)| *id == line.product_id()) {
                Some((_, quantity)) => *quantity += line.quantity().as_decimal(),
                None => wanted.push((line.product_id(), line.quantity().as_decimal())),
            }
        }
        let ids: Vec<ProductId> = wanted.iter().map(|(id, _)| *id).collect();
        let balances = self.movements.balances(company_id, &ids, None).await?;
        for ((product_id, needed), (_, balance)) in wanted.into_iter().zip(balances) {
            if balance.quantity() < needed {
                return Err(self
                    .short_of(company_id, product_id, balance.quantity())
                    .await);
            }
        }
        Ok(())
    }

    async fn take_for_sale(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        sale: &Sale,
    ) -> Result<(), ApplicationError> {
        let mut items = Vec::new();
        for (index, line) in sale.lines().iter().enumerate() {
            if !self.has_stock(company_id, line.product_id()).await? {
                continue;
            }
            items.push(NewMovement {
                draft: StockMovementDraft {
                    product_id: line.product_id(),
                    kind: StockMovementKind::Out,
                    quantity: line.quantity().to_string(),
                    value: None,
                    unit_cost: None,
                    occurred_on: sale.sold_on().to_string(),
                    note: Some(format!("Venda nº {}", sale.number())),
                },
                origin: Some(StockOrigin {
                    kind: StockOriginKind::Sale,
                    id: sale.id().as_uuid(),
                    line: u16::try_from(index + 1).unwrap_or(u16::MAX),
                }),
                counted: None,
            });
        }
        let floor = if self
            .settings(company_id)
            .await?
            .blocks_sales_without_balance()
        {
            StockFloor::Zero
        } else {
            StockFloor::Open
        };
        self.write(company_id, actor, items, floor).await.map(drop)
    }

    async fn put_back_cancelled_sale(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        sale_id: SaleId,
    ) -> Result<(), ApplicationError> {
        self.undo(
            company_id,
            actor,
            &Undo::Origin(StockOriginKind::Sale, sale_id.as_uuid()),
            None,
            None,
            StockFloor::Open,
        )
        .await
        .map(drop)
    }

    async fn put_back_return(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        sale: &Sale,
        sale_return: &SaleReturn,
        returned_before: &[(u16, Decimal)],
    ) -> Result<(), ApplicationError> {
        let exits = self
            .movements
            .of_origin(company_id, StockOriginKind::Sale, sale.id().as_uuid())
            .await?;
        let mut items = Vec::new();
        for (line, quantity) in sale_return.items() {
            let Some(exit) = exits.iter().find(|movement| {
                movement.reverses().is_none()
                    && movement.origin().is_some_and(|origin| origin.line == *line)
            }) else {
                continue;
            };
            let Some(sold) = usize::from(*line)
                .checked_sub(1)
                .and_then(|index| sale.lines().get(index))
                .map(|sold| sold.quantity().as_decimal())
            else {
                continue;
            };
            // What the sale took of the line, returned part by part, cumulatively, so the parts add up to it.
            let before: Decimal = returned_before
                .iter()
                .filter(|(number, _)| number == line)
                .map(|(_, quantity)| *quantity)
                .sum();
            let taken = -exit.value();
            let up_to = |returned: Decimal| round_to_cents(taken * returned / sold);
            let value = up_to(before + quantity.as_decimal()) - up_to(before);
            items.push(NewMovement {
                draft: StockMovementDraft {
                    product_id: exit.product_id(),
                    kind: StockMovementKind::In,
                    quantity: quantity_text(quantity.as_decimal()),
                    value: Some(value.normalize().to_string()),
                    unit_cost: None,
                    occurred_on: sale_return.returned_on().to_string(),
                    note: Some(format!("Devolução da venda nº {}", sale.number())),
                },
                origin: Some(StockOrigin {
                    kind: StockOriginKind::SaleReturn,
                    id: sale_return.id().as_uuid(),
                    line: *line,
                }),
                counted: None,
            });
        }
        self.write(company_id, actor, items, StockFloor::Open)
            .await
            .map(drop)
    }

    async fn receive_purchase(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        order: &PurchaseOrder,
        receipt: &PurchaseReceipt,
    ) -> Result<(), ApplicationError> {
        let mut items = Vec::new();
        for item in receipt.items() {
            let Some(line) = usize::from(item.line)
                .checked_sub(1)
                .and_then(|index| order.lines().get(index))
            else {
                continue;
            };
            if !self.has_stock(company_id, line.product_id()).await? {
                continue;
            }
            items.push(NewMovement {
                draft: StockMovementDraft {
                    product_id: line.product_id(),
                    kind: StockMovementKind::In,
                    quantity: item.quantity.to_string(),
                    value: Some(item.value.to_string()),
                    unit_cost: None,
                    occurred_on: receipt.received_on().to_string(),
                    note: Some(format!("Compra nº {}", order.number())),
                },
                origin: Some(StockOrigin {
                    kind: StockOriginKind::PurchaseReceipt,
                    id: receipt.id().as_uuid(),
                    line: item.line,
                }),
                counted: None,
            });
        }
        self.write(company_id, actor, items, StockFloor::Open)
            .await
            .map(drop)
    }
}

impl<M, P, S> CountStock for StockService<M, P, S>
where
    M: StockMovementRepository,
    P: ProductRepository,
    S: StockSettingsRepository,
{
    async fn ensure_stocked(
        &self,
        company_id: CompanyId,
        product_id: ProductId,
    ) -> Result<(), ApplicationError> {
        self.stocked(company_id, product_id).await.map(drop)
    }

    async fn quantities(
        &self,
        company_id: CompanyId,
        products: &[ProductId],
        on: CalendarDate,
    ) -> Result<Vec<(ProductId, Decimal)>, ApplicationError> {
        Ok(self
            .movements
            .balances(company_id, products, Some(on))
            .await?
            .into_iter()
            .map(|(product, balance)| (product, balance.quantity()))
            .collect())
    }

    async fn adjusted_by(
        &self,
        company_id: CompanyId,
        count_id: InventoryCountId,
    ) -> Result<Vec<(u16, Decimal)>, ApplicationError> {
        Ok(self
            .movements
            .of_origin(
                company_id,
                StockOriginKind::InventoryCount,
                count_id.as_uuid(),
            )
            .await?
            .into_iter()
            .filter(|movement| movement.reverses().is_none())
            .filter_map(|movement| {
                movement
                    .origin()
                    .map(|origin| (origin.line, movement.quantity()))
            })
            .collect())
    }

    async fn apply_count(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        count: &InventoryCount,
    ) -> Result<(), ApplicationError> {
        let items = count
            .lines()
            .iter()
            .enumerate()
            .map(|(index, line)| NewMovement {
                draft: StockMovementDraft {
                    product_id: line.product_id(),
                    kind: StockMovementKind::Adjustment,
                    // Replaced by the difference to what the stock holds when the write is valued.
                    quantity: "1".to_owned(),
                    value: None,
                    unit_cost: None,
                    occurred_on: count.counted_on().to_string(),
                    note: Some(format!("Inventário nº {}", count.number())),
                },
                origin: Some(StockOrigin {
                    kind: StockOriginKind::InventoryCount,
                    id: count.id().as_uuid(),
                    line: u16::try_from(index + 1).unwrap_or(u16::MAX),
                }),
                counted: Some(line.counted().as_decimal()),
            })
            .collect();
        self.write(company_id, actor, items, StockFloor::Open)
            .await
            .map(drop)
    }
}
