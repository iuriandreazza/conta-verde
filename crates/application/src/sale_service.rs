use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CategoryId, CompanyId, ContractCycle, CostCenterId, Decimal,
    DomainError, EntryOrigin, FinancialEntry, FinancialEntryDraft, PartyId, PaymentTermId, Quote,
    QuoteId, QuoteStatus, Sale, SaleDraft, SaleId, SaleReturn, SaleReturnDraft, SaleReturnId,
    SaleStatus, quote_changes, sale_changes, sale_creation_changes, sale_return_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, DocumentReferences, DocumentStock, EntryReferences, FinancialEntryRepository,
    FinancialEntryService, LinkChange, NumberCursor, QuoteRepository, RepositoryError, SaleQuery,
    SaleRepository,
};

#[derive(Debug, Clone)]
pub struct SalePage {
    pub items: Vec<Sale>,
    /// Present when more sales exist after this page.
    pub next: Option<NumberCursor>,
}

/// What a quote becomes a sale with: what the quote does not know.
#[derive(Debug, Clone)]
pub struct Conversion {
    pub sold_on: String,
    /// The category of the receivable.
    pub category_id: CategoryId,
    pub cost_center_id: Option<CostCenterId>,
    pub seller_id: Option<PartyId>,
    /// The quote's own payment term unless another is said.
    pub payment_term_id: Option<PaymentTermId>,
}

/// Where a refund is booked and when it falls due.
#[derive(Debug, Clone)]
pub struct Refund {
    pub category_id: CategoryId,
    pub due_on: String,
}

/// A return and the payable that gives the money back.
#[derive(Debug, Clone)]
pub struct ReturnRecord {
    pub sale_return: SaleReturn,
    /// `None` only when the write of the payable failed after the return was stored.
    pub refund: Option<FinancialEntry>,
}

/// Orders and sales of a company ([ADR 0039]): their life, the receivable a completed one makes, and what
/// comes back.
pub struct SaleService<S, Q, E, F, R, T> {
    sales: S,
    quotes: Q,
    entries: FinancialEntryService<E, F>,
    entry_references: F,
    references: R,
    stock: T,
}

impl<S, Q, E, F, R, T> SaleService<S, Q, E, F, R, T>
where
    S: SaleRepository,
    Q: QuoteRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
{
    /// The receivable is made through `entries`; `entry_references` is asked about the category and the cost
    /// center when the sale is written, so a mistake is refused when it is made and not when it is billed.
    pub fn new(
        sales: S,
        quotes: Q,
        entries: FinancialEntryService<E, F>,
        entry_references: F,
        references: R,
        stock: T,
    ) -> Self {
        Self {
            sales,
            quotes,
            entries,
            entry_references,
            references,
            stock,
        }
    }

    /// Validates the draft and what it points at, and stores the order with the next number of the company.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: SaleDraft,
    ) -> Result<Sale, ApplicationError> {
        let sale = Sale::new(SaleId::generate(), company_id, draft)?;
        self.check_references(&sale, None).await?;
        self.save(actor, sale).await
    }

    async fn save(&self, actor: &Actor, sale: Sale) -> Result<Sale, ApplicationError> {
        let audit = audit_entry(
            &sale,
            actor,
            AuditAction::Created,
            sale_creation_changes(&sale),
        );
        let number = self.sales.save(&sale, &audit).await?;
        Ok(sale.with_number(number))
    }

    /// Replaces the content of an order that is still open; one that changes nothing writes nothing.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: SaleId,
        draft: SaleDraft,
    ) -> Result<Sale, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        if after == before {
            return Ok(before);
        }
        self.check_references(&after, Some(&before)).await?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            sale_changes(&before, &after),
        );
        self.sales
            .update(&after, before.status(), &audit)
            .await
            .map_err(changed_or_missing)?;
        Ok(after)
    }

    /// The sale of one cycle of a contract, made and completed ([ADR 0040]): a contract has one sale per cycle
    /// day, so asking again for a cycle finishes whatever a failure left undone (the sale not completed, its
    /// receivable not made) and changes nothing when it is all there.
    pub async fn bill_cycle(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: SaleDraft,
        cycle: ContractCycle,
    ) -> Result<Sale, ApplicationError> {
        let sale = match self.sales.find_by_cycle(company_id, cycle).await? {
            Some(existing) => existing,
            None => {
                let sale = Sale::new(SaleId::generate(), company_id, draft)?.billing(cycle);
                self.check_references(&sale, None).await?;
                match self.save(actor, sale).await {
                    Ok(sale) => sale,
                    Err(ApplicationError::Repository(RepositoryError::DuplicateCycle)) => self
                        .sales
                        .find_by_cycle(company_id, cycle)
                        .await?
                        .ok_or(ApplicationError::SaleNotFound)?,
                    Err(error) => return Err(error),
                }
            }
        };
        // A cycle whose sale was cancelled by a person is consumed: it is not billed again, and it does not stop
        // the cycles after it.
        if sale.status() == SaleStatus::Cancelled {
            return Ok(sale);
        }
        self.complete(company_id, actor, sale.id()).await
    }

    /// Turns a quote into an order that is open ([ADR 0039]): its customer, term, notes and lines are copied,
    /// and the quote becomes `converted`. A quote makes one sale: asking again finishes what a failure left
    /// half done and gives the same sale back.
    pub async fn convert_quote(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        quote_id: QuoteId,
        conversion: Conversion,
    ) -> Result<Sale, ApplicationError> {
        let quote = self
            .quotes
            .find(company_id, quote_id)
            .await?
            .ok_or(ApplicationError::QuoteNotFound)?;
        if let Some(existing) = self.sales.find_by_quote(company_id, quote_id).await? {
            self.mark_converted(actor, &quote).await?;
            return Ok(existing);
        }
        // A quote that was declined cannot become a sale: refused before anything is written.
        quote.converted()?;
        let draft = SaleDraft {
            customer_id: quote.customer_id(),
            seller_id: conversion.seller_id,
            sold_on: conversion.sold_on,
            payment_term_id: conversion.payment_term_id.or(quote.payment_term_id()),
            category_id: conversion.category_id,
            cost_center_id: conversion.cost_center_id,
            notes: quote.notes().map(str::to_owned),
            lines: quote.lines().iter().map(|line| line.to_draft()).collect(),
        };
        let sale = Sale::new(SaleId::generate(), company_id, draft)?.from_quote(quote_id);
        self.check_references(&sale, None).await?;
        let sale = match self.save(actor, sale).await {
            Ok(sale) => sale,
            Err(ApplicationError::Repository(RepositoryError::DuplicateQuoteSale)) => {
                return Err(ApplicationError::QuoteAlreadyConverted);
            }
            Err(error) => return Err(error),
        };
        self.mark_converted(actor, &quote).await?;
        Ok(sale)
    }

    /// The quote marked converted, which a quote already converted is not asked again.
    async fn mark_converted(&self, actor: &Actor, quote: &Quote) -> Result<(), ApplicationError> {
        if quote.status() == QuoteStatus::Converted {
            return Ok(());
        }
        let after = quote.converted()?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            quote_changes(quote, &after),
        );
        self.quotes
            .update(&after, quote.status(), LinkChange::Keep, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::QuoteNotFound,
                RepositoryError::QuoteChanged => ApplicationError::QuoteChanged,
                other => other.into(),
            })
    }

    /// Bills an open order: its receivable is made from its payment term, and its items are fixed from then
    /// on. The receivable is checked **before** the order changes; only the status is written, and the
    /// receivable is worked out from the sale **as it was stored** once it is completed, so an edit that
    /// landed meanwhile is never billed from an older copy. Asking again for a completed sale finishes a
    /// receivable a failure left unmade, and changes nothing when it exists.
    pub async fn complete(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: SaleId,
    ) -> Result<Sale, ApplicationError> {
        let sale = self.find(company_id, id).await?;
        match sale.status() {
            SaleStatus::Open => {
                let completed = sale.completed()?;
                // The books of the day are what a completed sale changes: a closed day refuses it ([ADR 0068]).
                self.entry_references
                    .ensure_open(company_id, completed.sold_on())
                    .await?;
                let receivable = self.receivable_of(&completed).await?;
                self.entries.validate(company_id, receivable).await?;
                // A company that refuses a sale without balance finds out before the sale changes.
                self.stock
                    .ensure_available_for_sale(company_id, &completed)
                    .await?;
                let audit = audit_entry(
                    &completed,
                    actor,
                    AuditAction::Updated,
                    sale_changes(&sale, &completed),
                );
                match self
                    .sales
                    .set_status(&completed, SaleStatus::Open, &audit)
                    .await
                {
                    Ok(()) => {}
                    // Someone completed it a moment ago: what is left is the receivable, which is made once.
                    Err(RepositoryError::SaleChanged) => {}
                    Err(error) => return Err(changed_or_missing(error)),
                }
            }
            SaleStatus::Completed => {}
            SaleStatus::Cancelled => return Err(DomainError::SaleIsLocked.into()),
        }
        let stored = self.find(company_id, id).await?;
        if stored.status() == SaleStatus::Cancelled {
            return Err(DomainError::SaleIsLocked.into());
        }
        if self
            .entries
            .made_by(company_id, EntryOrigin::Sale(id))
            .await?
            .is_none()
        {
            let receivable = self.receivable_of(&stored).await?;
            match self
                .entries
                .register_with_origin(company_id, actor, receivable, EntryOrigin::Sale(id))
                .await
            {
                Ok(entry) => {
                    // Cancelled between the status and the entry: the receivable is not left alive.
                    if self.find(company_id, id).await?.status() == SaleStatus::Cancelled {
                        self.entries.void(company_id, actor, entry.id()).await?;
                        return Err(DomainError::SaleIsLocked.into());
                    }
                }
                Err(ApplicationError::Repository(RepositoryError::DuplicateOrigin)) => {}
                Err(error) => return Err(error),
            }
        }
        // The exits are made once, however often this is asked, and last: a sale cancelled while they were
        // being made has them undone at once.
        self.stock.take_for_sale(company_id, actor, &stored).await?;
        if self.find(company_id, id).await?.status() == SaleStatus::Cancelled {
            self.stock
                .put_back_cancelled_sale(company_id, actor, id)
                .await?;
            return Err(DomainError::SaleIsLocked.into());
        }
        Ok(stored)
    }

    async fn receivable_of(&self, sale: &Sale) -> Result<FinancialEntryDraft, ApplicationError> {
        let term = match sale.payment_term_id() {
            Some(id) => Some(
                self.references
                    .payment_term(sale.company_id(), id)
                    .await?
                    .ok_or(ApplicationError::UnusableReference("payment term"))?,
            ),
            None => None,
        };
        Ok(sale.receivable(term.as_ref())?)
    }

    /// Cancels a sale. A completed one has its receivable cancelled with it, which is refused while the
    /// receivable has settlements that are not reversed, and while the sale has returns: what was already
    /// given back cannot be taken from a sale that never was. Asking again finishes a cancellation a failure
    /// left half done.
    pub async fn cancel(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: SaleId,
    ) -> Result<Sale, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.cancelled()?;
        if before.status() == SaleStatus::Completed {
            self.entry_references
                .ensure_open(company_id, before.sold_on())
                .await?;
            if !self.sales.returns_of(company_id, id).await?.is_empty() {
                return Err(ApplicationError::SaleHasReturns);
            }
            if let Some(entry) = self
                .entries
                .made_by(company_id, EntryOrigin::Sale(id))
                .await?
                .filter(|entry| !entry.is_voided())
            {
                self.entries
                    .void(company_id, actor, entry.id())
                    .await
                    .map_err(|error| match error {
                        ApplicationError::EntryIsSettled => ApplicationError::SaleHasSettlements,
                        other => other,
                    })?;
            }
            self.stock
                .put_back_cancelled_sale(company_id, actor, id)
                .await?;
        }
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            sale_changes(&before, &after),
        );
        self.sales
            .set_status(&after, before.status(), &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::SaleHasReturns => ApplicationError::SaleHasReturns,
                other => changed_or_missing(other),
            })?;
        Ok(after)
    }

    /// Records what came back of a completed sale and makes the payable that refunds it ([ADR 0039]). The
    /// refund is checked before the return is stored; if the write of the payable fails after that, the
    /// return stays and says it has no refund yet.
    pub async fn register_return(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        sale_id: SaleId,
        draft: SaleReturnDraft,
        refund: Refund,
    ) -> Result<ReturnRecord, ApplicationError> {
        let sale = self.find(company_id, sale_id).await?;
        let made_before = self.sales.returns_of(company_id, sale_id).await?;
        let before: Vec<(u16, Decimal)> = made_before
            .iter()
            .flat_map(|made| made.items().iter().map(|(line, q)| (*line, q.as_decimal())))
            .collect();
        let sale_return = SaleReturn::new(SaleReturnId::generate(), &sale, draft, &before)?;
        // Before the return is stored: it is stored first, and a refund that a closed day then refused would be
        // left without one.
        self.entry_references
            .ensure_open(company_id, sale_return.returned_on())
            .await?;
        let payable = sale_return.refund_entry(&sale, refund.category_id, &refund.due_on)?;
        self.entries.validate(company_id, payable.clone()).await?;
        let audit = audit_entry(
            &sale_return,
            actor,
            AuditAction::Created,
            sale_return_creation_changes(&sale_return),
        );
        self.sales
            .save_return(&sale_return, made_before.len(), &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::ReturnTooMuch => DomainError::ReturnExceedsSold.into(),
                RepositoryError::NotFound => ApplicationError::SaleNotFound,
                other => changed_or_missing(other),
            })?;
        let entry = self
            .entries
            .register_with_origin(
                company_id,
                actor,
                payable,
                EntryOrigin::SaleReturn(sale_return.id()),
            )
            .await
            .ok();
        // Like the refund, a failure here leaves the return stored: the stock is put right by hand.
        self.stock
            .put_back_return(company_id, actor, &sale, &sale_return, &before)
            .await
            .ok();
        Ok(ReturnRecord {
            sale_return,
            refund: entry,
        })
    }

    /// The returns of a sale, oldest first, each with the payable that refunded it.
    pub async fn returns(
        &self,
        company_id: CompanyId,
        sale_id: SaleId,
    ) -> Result<Vec<ReturnRecord>, ApplicationError> {
        self.find(company_id, sale_id).await?;
        let mut records = Vec::new();
        for sale_return in self.sales.returns_of(company_id, sale_id).await? {
            let refund = self
                .entries
                .made_by(company_id, EntryOrigin::SaleReturn(sale_return.id()))
                .await?;
            records.push(ReturnRecord {
                sale_return,
                refund,
            });
        }
        Ok(records)
    }

    /// The receivable of a sale, if it was billed.
    pub async fn receivable(
        &self,
        company_id: CompanyId,
        sale_id: SaleId,
    ) -> Result<Option<FinancialEntry>, ApplicationError> {
        self.find(company_id, sale_id).await?;
        self.entries
            .made_by(company_id, EntryOrigin::Sale(sale_id))
            .await
    }

    pub async fn get(&self, company_id: CompanyId, id: SaleId) -> Result<Sale, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the sale, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: SaleId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's sale answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.sales.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &SaleQuery,
        after: Option<&NumberCursor>,
        limit: Option<usize>,
    ) -> Result<SalePage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .sales
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(SalePage { items, next })
    }

    async fn find(&self, company_id: CompanyId, id: SaleId) -> Result<Sale, ApplicationError> {
        self.sales
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::SaleNotFound)
    }

    /// What the sale points at must be usable in the company, the answer naming the field. When the sale was
    /// already stored, only what the edit **changes** is checked.
    pub(crate) async fn check_references(
        &self,
        sale: &Sale,
        before: Option<&Sale>,
    ) -> Result<(), ApplicationError> {
        let company = sale.company_id();
        let changed =
            |same: fn(&Sale, &Sale) -> bool| before.is_none_or(|stored| !same(stored, sale));
        if changed(|a, b| a.customer_id() == b.customer_id())
            && !self
                .references
                .customer_exists(company, sale.customer_id())
                .await?
        {
            return Err(ApplicationError::UnusableReference("customer"));
        }
        if let Some(seller) = sale.seller_id()
            && changed(|a, b| a.seller_id() == b.seller_id())
            && !self.references.seller_is_usable(company, seller).await?
        {
            return Err(ApplicationError::UnusableReference("seller"));
        }
        if let Some(term) = sale.payment_term_id()
            && changed(|a, b| a.payment_term_id() == b.payment_term_id())
            && !self
                .references
                .payment_term_is_usable(company, term)
                .await?
        {
            return Err(ApplicationError::UnusableReference("payment term"));
        }
        if changed(|a, b| a.category_id() == b.category_id())
            && !self
                .entry_references
                .category_is_usable(company, sale.category_id())
                .await?
        {
            return Err(ApplicationError::UnusableReference("category"));
        }
        if let Some(center) = sale.cost_center_id()
            && changed(|a, b| a.cost_center_id() == b.cost_center_id())
            && !self
                .entry_references
                .cost_center_is_usable(company, center)
                .await?
        {
            return Err(ApplicationError::UnusableReference("cost center"));
        }
        for line in sale.lines() {
            let already_there = before.is_some_and(|stored| {
                stored
                    .lines()
                    .iter()
                    .any(|old| old.product_id() == line.product_id())
            });
            if !already_there
                && !self
                    .references
                    .product_is_usable(company, line.product_id())
                    .await?
            {
                return Err(ApplicationError::UnusableReference("product"));
            }
        }
        Ok(())
    }
}

fn changed_or_missing(error: RepositoryError) -> ApplicationError {
    match error {
        RepositoryError::NotFound => ApplicationError::SaleNotFound,
        RepositoryError::SaleChanged => ApplicationError::SaleChanged,
        other => other.into(),
    }
}
