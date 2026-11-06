use std::collections::HashMap;
use std::future::Future;

use conta_verde_domain::{
    Actor, AuditAction, CalendarDate, CompanyId, Decimal, DomainError, Product, ProductId,
    ProductKind, PurchaseOrder, PurchaseReceipt, Sale, SaleId, SaleReturn, StockBalance,
    StockMovement, StockMovementDraft, StockMovementId, StockMovementKind, StockOrigin,
    StockOriginKind, round_to_cents, stock_movement_creation_changes,
};
use uuid::Uuid;

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, DateCursor, NameCursor, ProductQuery, ProductRepository, RepositoryError,
    StockFloor, StockMovementQuery, StockMovementRepository,
};

/// How many times a write is valued again when another one changed the stock under it.
const ATTEMPTS: usize = 5;

/// What sales and purchases ask of the stock ([ADR 0054]). Every step is idempotent: asking again writes only what
/// is missing, so a failure between a document and its stock is finished by asking again.
pub trait DocumentStock: Send + Sync {
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

#[derive(Debug, Clone)]
pub struct StockPositionPage {
    pub items: Vec<StockPosition>,
    /// Present when more products exist after this page.
    pub next: Option<NameCursor>,
}

/// One movement to write, before it is valued.
struct NewMovement {
    draft: StockMovementDraft,
    origin: Option<StockOrigin>,
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
pub struct StockService<M, P> {
    movements: M,
    products: P,
}

impl<M, P> StockService<M, P>
where
    M: StockMovementRepository,
    P: ProductRepository,
{
    pub fn new(movements: M, products: P) -> Self {
        Self {
            movements,
            products,
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

    /// What the products of a page have, through `on` when it is given (every movement otherwise). Only products
    /// have stock: services are left out.
    pub async fn positions(
        &self,
        company_id: CompanyId,
        query: ProductQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
        on: Option<CalendarDate>,
    ) -> Result<StockPositionPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .products
            .list_by_company(
                company_id,
                &query.of_kind(ProductKind::Product),
                after,
                limit + 1,
            )
            .await?;
        let (products, next): (Vec<Product>, Option<NameCursor>) = split_page(rows, limit);
        let ids: Vec<ProductId> = products.iter().map(Product::id).collect();
        let balances = self.movements.balances(company_id, &ids, on).await?;
        let items = products
            .into_iter()
            .zip(balances)
            .map(|(product, (_, balance))| StockPosition { product, balance })
            .collect();
        Ok(StockPositionPage { items, next })
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
                let movement = StockMovement::new(
                    StockMovementId::generate(),
                    company_id,
                    item.draft.clone(),
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
                    let name = match self.products.find(company_id, *product_id).await? {
                        Some(product) => product.name().to_owned(),
                        None => "the product".to_owned(),
                    };
                    return Err(ApplicationError::StockInsufficient {
                        product: name,
                        available: before.quantity().max(Decimal::ZERO).to_string(),
                    });
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

impl<M, P> DocumentStock for StockService<M, P>
where
    M: StockMovementRepository,
    P: ProductRepository,
{
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
            });
        }
        self.write(company_id, actor, items, StockFloor::Open)
            .await
            .map(drop)
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
            });
        }
        self.write(company_id, actor, items, StockFloor::Open)
            .await
            .map(drop)
    }
}
