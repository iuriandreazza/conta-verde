use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, CountStatus, Decimal, DomainError, InventoryCount,
    InventoryCountDraft, InventoryCountId, ProductId, inventory_count_changes,
    inventory_count_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, CountStock, InventoryCountQuery, InventoryCountRepository, NumberCursor,
    RepositoryError,
};

#[derive(Debug, Clone)]
pub struct InventoryCountPage {
    pub items: Vec<InventoryCount>,
    /// Present when more counts exist after this page.
    pub next: Option<NumberCursor>,
}

/// What a count says of one product, and what it means for the stock.
#[derive(Debug, Clone)]
pub struct CountLineView {
    pub product_id: ProductId,
    pub counted: Decimal,
    /// What the stock held: now while the count is open, what it held when the count was applied afterwards;
    /// `None` for a cancelled count, which has no meaning for the stock.
    pub expected: Option<Decimal>,
    /// What was, or will be, adjusted: counted less expected.
    pub difference: Option<Decimal>,
}

/// A count with what it means for the stock.
#[derive(Debug, Clone)]
pub struct CountView {
    pub count: InventoryCount,
    pub lines: Vec<CountLineView>,
}

/// Counts of the stock of a company ([ADR 0056]): written, looked at against what the stock holds, and applied as
/// adjustments.
pub struct InventoryCountService<C, T> {
    counts: C,
    stock: T,
}

impl<C, T> InventoryCountService<C, T>
where
    C: InventoryCountRepository,
    T: CountStock,
{
    pub fn new(counts: C, stock: T) -> Self {
        Self { counts, stock }
    }

    /// Validates the draft and that each product has stock, and stores the count with the next number.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: InventoryCountDraft,
    ) -> Result<InventoryCount, ApplicationError> {
        let count = InventoryCount::new(InventoryCountId::generate(), company_id, draft)?;
        self.check_products(&count, None).await?;
        let audit = audit_entry(
            &count,
            actor,
            AuditAction::Created,
            inventory_count_creation_changes(&count),
        );
        let number = self.counts.save(&count, &audit).await?;
        Ok(count.with_number(number))
    }

    /// Replaces the content of a count that is open; one that changes nothing writes nothing.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: InventoryCountId,
        draft: InventoryCountDraft,
    ) -> Result<InventoryCount, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        if after == before {
            return Ok(before);
        }
        self.check_products(&after, Some(&before)).await?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            inventory_count_changes(&before, &after),
        );
        self.counts
            .update(&after, before.status(), &audit)
            .await
            .map_err(changed_or_missing)?;
        Ok(after)
    }

    /// Writes the differences as adjustments of the stock, on the day of the count, and closes the count. The
    /// adjustments are written first and once (a repeat finds them), then the status: asking again for a count a
    /// failure left open finishes it.
    pub async fn apply(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: InventoryCountId,
    ) -> Result<InventoryCount, ApplicationError> {
        let before = self.find(company_id, id).await?;
        match before.status() {
            CountStatus::Applied => return Ok(before),
            CountStatus::Cancelled => return Err(DomainError::CountIsLocked.into()),
            CountStatus::Open => {}
        }
        let after = before.applied()?;
        self.stock.apply_count(company_id, actor, &before).await?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            inventory_count_changes(&before, &after),
        );
        match self
            .counts
            .set_status(&after, CountStatus::Open, &audit)
            .await
        {
            // Someone applied it a moment ago: the adjustments are written once, so it is done.
            Ok(()) | Err(RepositoryError::CountChanged) => Ok(after),
            Err(error) => Err(changed_or_missing(error)),
        }
    }

    /// Cancels a count that is open: nothing is written to the stock.
    pub async fn cancel(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: InventoryCountId,
    ) -> Result<InventoryCount, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.cancelled()?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            inventory_count_changes(&before, &after),
        );
        self.counts
            .set_status(&after, before.status(), &audit)
            .await
            .map_err(changed_or_missing)?;
        Ok(after)
    }

    /// The count with what it means for the stock: against what the stock holds now while it is open, and as it
    /// was adjusted once it is applied.
    pub async fn view(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> Result<CountView, ApplicationError> {
        let count = self.find(company_id, id).await?;
        let mut lines = Vec::with_capacity(count.lines().len());
        match count.status() {
            CountStatus::Open => {
                let ids: Vec<ProductId> = count.lines().iter().map(|l| l.product_id()).collect();
                let held = self.stock.quantities(company_id, &ids).await?;
                for (line, (_, on_hand)) in count.lines().iter().zip(held) {
                    lines.push(CountLineView {
                        product_id: line.product_id(),
                        counted: line.counted().as_decimal(),
                        expected: Some(on_hand.normalize()),
                        difference: Some(InventoryCount::difference(line, on_hand)),
                    });
                }
            }
            CountStatus::Applied => {
                let adjusted = self.stock.adjusted_by(company_id, count.id()).await?;
                for (index, line) in count.lines().iter().enumerate() {
                    let position = u16::try_from(index + 1).unwrap_or(u16::MAX);
                    let difference = adjusted
                        .iter()
                        .find(|(number, _)| *number == position)
                        .map_or(Decimal::ZERO, |(_, quantity)| *quantity);
                    lines.push(CountLineView {
                        product_id: line.product_id(),
                        counted: line.counted().as_decimal(),
                        expected: Some((line.counted().as_decimal() - difference).normalize()),
                        difference: Some(difference.normalize()),
                    });
                }
            }
            CountStatus::Cancelled => {
                for line in count.lines() {
                    lines.push(CountLineView {
                        product_id: line.product_id(),
                        counted: line.counted().as_decimal(),
                        expected: None,
                        difference: None,
                    });
                }
            }
        }
        Ok(CountView { count, lines })
    }

    /// What was recorded about the count, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's count answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.counts.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &InventoryCountQuery,
        after: Option<&NumberCursor>,
        limit: Option<usize>,
    ) -> Result<InventoryCountPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .counts
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(InventoryCountPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: InventoryCountId,
    ) -> Result<InventoryCount, ApplicationError> {
        self.counts
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::CountNotFound)
    }

    /// Each product counted must be one that has stock; when the count was already stored, only the products the
    /// edit adds are checked.
    async fn check_products(
        &self,
        count: &InventoryCount,
        before: Option<&InventoryCount>,
    ) -> Result<(), ApplicationError> {
        for line in count.lines() {
            let already_there = before.is_some_and(|stored| {
                stored
                    .lines()
                    .iter()
                    .any(|old| old.product_id() == line.product_id())
            });
            if !already_there {
                self.stock
                    .ensure_stocked(count.company_id(), line.product_id())
                    .await?;
            }
        }
        Ok(())
    }
}

fn changed_or_missing(error: RepositoryError) -> ApplicationError {
    match error {
        RepositoryError::NotFound => ApplicationError::CountNotFound,
        RepositoryError::CountChanged => ApplicationError::CountChanged,
        other => other.into(),
    }
}
