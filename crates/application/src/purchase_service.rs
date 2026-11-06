use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, Decimal, DomainError, EntryOrigin, FinancialEntry,
    FinancialEntryDraft, PurchaseOrder, PurchaseOrderDraft, PurchaseOrderId, PurchaseReceipt,
    PurchaseReceiptDraft, PurchaseReceiptId, purchase_order_changes,
    purchase_order_creation_changes, purchase_receipt_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, DocumentReferences, DocumentStock, EntryReferences, FinancialEntryRepository,
    FinancialEntryService, NumberCursor, PurchaseOrderQuery, PurchaseOrderRepository,
    RepositoryError,
};

#[derive(Debug, Clone)]
pub struct PurchaseOrderPage {
    pub items: Vec<PurchaseOrder>,
    /// Present when more orders exist after this page.
    pub next: Option<NumberCursor>,
}

/// A receipt and the payable that pays for it.
#[derive(Debug, Clone)]
pub struct ReceiptRecord {
    pub receipt: PurchaseReceipt,
    /// `None` when the receipt is worth nothing (nothing to pay), or when the write of the payable failed
    /// after the receipt was stored and has not been finished yet.
    pub payable: Option<FinancialEntry>,
}

/// Purchase orders of a company ([ADR 0053]): their life, and the payable each receipt makes.
pub struct PurchaseService<P, E, F, R, T> {
    purchases: P,
    entries: FinancialEntryService<E, F>,
    entry_references: F,
    references: R,
    stock: T,
}

impl<P, E, F, R, T> PurchaseService<P, E, F, R, T>
where
    P: PurchaseOrderRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
{
    /// The payables are made through `entries`; `entry_references` is asked about the category and the cost
    /// center when the order is written, so a mistake is refused when it is made and not when goods arrive.
    pub fn new(
        purchases: P,
        entries: FinancialEntryService<E, F>,
        entry_references: F,
        references: R,
        stock: T,
    ) -> Self {
        Self {
            purchases,
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
        draft: PurchaseOrderDraft,
    ) -> Result<PurchaseOrder, ApplicationError> {
        let order = PurchaseOrder::new(PurchaseOrderId::generate(), company_id, draft)?;
        self.check_references(&order, None).await?;
        let audit = audit_entry(
            &order,
            actor,
            AuditAction::Created,
            purchase_order_creation_changes(&order),
        );
        let number = self.purchases.save(&order, &audit).await?;
        Ok(order.with_number(number))
    }

    /// Replaces the content of a draft; one that changes nothing writes nothing.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: PurchaseOrderId,
        draft: PurchaseOrderDraft,
    ) -> Result<PurchaseOrder, ApplicationError> {
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
            purchase_order_changes(&before, &after),
        );
        self.purchases
            .update(&after, before.status(), &audit)
            .await
            .map_err(changed_or_missing)?;
        Ok(after)
    }

    /// Agrees a draft with the supplier: its items are fixed from then on.
    pub async fn approve(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: PurchaseOrderId,
    ) -> Result<PurchaseOrder, ApplicationError> {
        self.transition(company_id, actor, id, PurchaseOrder::approved)
            .await
    }

    /// Cancels an order before anything arrived.
    pub async fn cancel(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: PurchaseOrderId,
    ) -> Result<PurchaseOrder, ApplicationError> {
        self.transition(company_id, actor, id, PurchaseOrder::cancelled)
            .await
    }

    /// Closes a partly received order: what has not arrived is given up.
    pub async fn close(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: PurchaseOrderId,
    ) -> Result<PurchaseOrder, ApplicationError> {
        self.transition(company_id, actor, id, PurchaseOrder::closed)
            .await
    }

    async fn transition(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: PurchaseOrderId,
        step: fn(&PurchaseOrder) -> Result<PurchaseOrder, DomainError>,
    ) -> Result<PurchaseOrder, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = step(&before)?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            purchase_order_changes(&before, &after),
        );
        self.purchases
            .set_status(&after, before.status(), &audit)
            .await
            .map_err(changed_or_missing)?;
        Ok(after)
    }

    /// Records what arrived of an approved order and makes the payable that pays for it ([ADR 0053]). The
    /// payable is checked before the receipt is stored; if its write fails after that, the receipt stays and
    /// says it has no payable yet, and `finish_receipt` makes it.
    pub async fn receive(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        order_id: PurchaseOrderId,
        draft: PurchaseReceiptDraft,
    ) -> Result<ReceiptRecord, ApplicationError> {
        let order = self.find(company_id, order_id).await?;
        let made_before = self.purchases.receipts_of(company_id, order_id).await?;
        let before: Vec<(u16, Decimal)> = made_before
            .iter()
            .flat_map(|made| {
                made.items()
                    .iter()
                    .map(|item| (item.line, item.quantity.as_decimal()))
            })
            .collect();
        let receipt = PurchaseReceipt::new(PurchaseReceiptId::generate(), &order, draft, &before)?;
        if let Some(payable) = self.payable_of(&order, &receipt).await? {
            self.entries.validate(company_id, payable).await?;
        }
        let after = order.with_status(order.status_after(&receipt, &before));
        let receipt_audit = audit_entry(
            &receipt,
            actor,
            AuditAction::Created,
            purchase_receipt_creation_changes(&receipt),
        );
        let order_audit = (after.status() != order.status()).then(|| {
            audit_entry(
                &after,
                actor,
                AuditAction::Updated,
                purchase_order_changes(&order, &after),
            )
        });
        self.purchases
            .save_receipt(
                &receipt,
                &after,
                made_before.len(),
                &receipt_audit,
                order_audit.as_ref(),
            )
            .await
            .map_err(|error| match error {
                RepositoryError::ReceiptTooMuch => DomainError::ReceiptExceedsOrdered.into(),
                other => changed_or_missing(other),
            })?;
        let payable = self
            .finish(company_id, actor, &order, &receipt)
            .await
            .ok()
            .flatten();
        Ok(ReceiptRecord { receipt, payable })
    }

    /// Makes what a receipt still lacks, which is safe to ask again: its payable and its stock entries are made
    /// once, and nothing changes when it is all there.
    pub async fn finish_receipt(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        order_id: PurchaseOrderId,
        id: PurchaseReceiptId,
    ) -> Result<ReceiptRecord, ApplicationError> {
        let order = self.find(company_id, order_id).await?;
        let receipt = self
            .purchases
            .find_receipt(company_id, order_id, id)
            .await?
            .ok_or(ApplicationError::PurchaseReceiptNotFound)?;
        let payable = self.finish(company_id, actor, &order, &receipt).await?;
        Ok(ReceiptRecord { receipt, payable })
    }

    async fn finish(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        order: &PurchaseOrder,
        receipt: &PurchaseReceipt,
    ) -> Result<Option<FinancialEntry>, ApplicationError> {
        let payable = self.pay_for(company_id, actor, order, receipt).await?;
        // The goods enter stock once, however often this is asked.
        self.stock
            .receive_purchase(company_id, actor, order, receipt)
            .await?;
        Ok(payable)
    }

    /// The payable of the receipt, made if it was not yet.
    async fn pay_for(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        order: &PurchaseOrder,
        receipt: &PurchaseReceipt,
    ) -> Result<Option<FinancialEntry>, ApplicationError> {
        let origin = EntryOrigin::PurchaseReceipt(receipt.id());
        if let Some(entry) = self.entries.made_by(company_id, origin).await? {
            return Ok(Some(entry));
        }
        let Some(payable) = self.payable_of(order, receipt).await? else {
            return Ok(None);
        };
        match self
            .entries
            .register_with_origin(company_id, actor, payable, origin)
            .await
        {
            Ok(entry) => Ok(Some(entry)),
            Err(ApplicationError::Repository(RepositoryError::DuplicateOrigin)) => {
                Ok(self.entries.made_by(company_id, origin).await?)
            }
            Err(error) => Err(error),
        }
    }

    /// The payable of a receipt: none for one worth nothing, such as goods given by the supplier.
    async fn payable_of(
        &self,
        order: &PurchaseOrder,
        receipt: &PurchaseReceipt,
    ) -> Result<Option<FinancialEntryDraft>, ApplicationError> {
        if receipt.total() <= Decimal::ZERO {
            return Ok(None);
        }
        let term = match order.payment_term_id() {
            Some(id) => Some(
                self.references
                    .payment_term(order.company_id(), id)
                    .await?
                    .ok_or(ApplicationError::UnusableReference("payment term"))?,
            ),
            None => None,
        };
        Ok(Some(order.payable_of(receipt, term.as_ref())?))
    }

    /// The receipts of an order, oldest first, each with its payable.
    pub async fn receipts(
        &self,
        company_id: CompanyId,
        order_id: PurchaseOrderId,
    ) -> Result<Vec<ReceiptRecord>, ApplicationError> {
        self.find(company_id, order_id).await?;
        let mut records = Vec::new();
        for receipt in self.purchases.receipts_of(company_id, order_id).await? {
            let payable = self
                .entries
                .made_by(company_id, EntryOrigin::PurchaseReceipt(receipt.id()))
                .await?;
            records.push(ReceiptRecord { receipt, payable });
        }
        Ok(records)
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> Result<PurchaseOrder, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the order, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's order answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.purchases.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &PurchaseOrderQuery,
        after: Option<&NumberCursor>,
        limit: Option<usize>,
    ) -> Result<PurchaseOrderPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .purchases
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(PurchaseOrderPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> Result<PurchaseOrder, ApplicationError> {
        self.purchases
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::PurchaseNotFound)
    }

    /// What the order points at must be usable in the company, the answer naming the field. When the order was
    /// already stored, only what the edit **changes** is checked.
    async fn check_references(
        &self,
        order: &PurchaseOrder,
        before: Option<&PurchaseOrder>,
    ) -> Result<(), ApplicationError> {
        let company = order.company_id();
        let changed = |same: fn(&PurchaseOrder, &PurchaseOrder) -> bool| {
            before.is_none_or(|stored| !same(stored, order))
        };
        if changed(|a, b| a.supplier_id() == b.supplier_id())
            && !self
                .references
                .supplier_is_usable(company, order.supplier_id())
                .await?
        {
            return Err(ApplicationError::UnusableReference("supplier"));
        }
        if let Some(term) = order.payment_term_id()
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
                .category_is_usable(company, order.category_id())
                .await?
        {
            return Err(ApplicationError::UnusableReference("category"));
        }
        if let Some(center) = order.cost_center_id()
            && changed(|a, b| a.cost_center_id() == b.cost_center_id())
            && !self
                .entry_references
                .cost_center_is_usable(company, center)
                .await?
        {
            return Err(ApplicationError::UnusableReference("cost center"));
        }
        for line in order.lines() {
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
        RepositoryError::NotFound => ApplicationError::PurchaseNotFound,
        RepositoryError::PurchaseChanged => ApplicationError::PurchaseChanged,
        other => other.into(),
    }
}
