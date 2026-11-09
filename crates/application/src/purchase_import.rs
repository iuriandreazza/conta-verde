use conta_verde_domain::{
    Actor, CategoryId, CompanyId, CostCenterId, DomainError, Invoice, InvoiceDraft, PartyId,
    PaymentTermId, ProductId, PurchaseOrder, PurchaseOrderDraft, PurchaseReceiptDraft,
    PurchaseStatus, ReceiptItemDraft, ScheduleDraft, purchase_order_creation_changes,
};
use conta_verde_domain::{AuditAction, CalendarDate, PurchaseOrderId};

use crate::audit::audit_entry;
use crate::{
    ApplicationError, DocumentReferences, DocumentStock, EntryReferences, FinancialEntryRepository,
    PurchaseOrderRepository, PurchaseService, ReceiptRecord, RepositoryError,
};

/// What an invoice says and how it meets the company's registers, before anything is written ([ADR 0057]).
#[derive(Debug, Clone)]
pub struct InvoicePreview {
    pub invoice: Invoice,
    /// The supplier registered with the issuer's registry number, if there is one.
    pub supplier: Option<PartyId>,
    /// The product registered with the supplier's code of each item as its SKU, in the order of the items.
    pub products: Vec<Option<ProductId>>,
    /// The purchase the invoice was entered as, when it was.
    pub entered: Option<PurchaseOrder>,
}

/// What the company says about an invoice to enter it: where the payables are booked, when the goods arrived and what
/// each item is.
#[derive(Debug, Clone)]
pub struct InvoiceEntry {
    pub category_id: CategoryId,
    pub cost_center_id: Option<CostCenterId>,
    /// Used for the payable when the invoice gives no installments of its own.
    pub payment_term_id: Option<PaymentTermId>,
    /// The day the goods arrived (and entered stock); the day of the invoice when none is said.
    pub received_on: Option<String>,
    /// The supplier, when it is not the one the issuer's registry number finds.
    pub supplier_id: Option<PartyId>,
    /// The product each item becomes, in the order of the items. An item the SKU finds may be left out.
    pub products: Vec<Option<ProductId>>,
}

/// An invoice entered: the purchase, its one receipt, and whether it already was.
#[derive(Debug, Clone)]
pub struct InvoiceOutcome {
    pub order: PurchaseOrder,
    pub receipt: ReceiptRecord,
    /// The invoice had been entered before: nothing was made twice, and what a failure left undone was finished.
    pub already_entered: bool,
}

impl<P, E, F, R, T> PurchaseService<P, E, F, R, T>
where
    P: PurchaseOrderRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
{
    /// Reads an invoice against the company: its supplier, its products and whether it was entered.
    pub async fn preview_invoice(
        &self,
        company_id: CompanyId,
        draft: InvoiceDraft,
    ) -> Result<InvoicePreview, ApplicationError> {
        let invoice = Invoice::new(draft)?;
        let supplier = self
            .references()
            .party_with_document(company_id, invoice.issuer())
            .await?;
        let mut products = Vec::with_capacity(invoice.items().len());
        for item in invoice.items() {
            products.push(
                self.references()
                    .product_with_code(company_id, item.code())
                    .await?,
            );
        }
        let entered = self
            .purchases()
            .find_by_invoice_key(company_id, invoice.access_key())
            .await?;
        Ok(InvoicePreview {
            invoice,
            supplier,
            products,
            entered,
        })
    }

    /// Enters a supplier's invoice as a purchase that arrived whole: the order, its receipt, the payable (on the
    /// installments the invoice gives, or by the payment term) and the stock entries ([ADR 0057]). **The access key
    /// makes it once**: the order is the claim (a key has at most one), every later step is idempotent and picks up
    /// where a failure stopped, so entering the same file again changes nothing and finishes what was left.
    pub async fn enter_invoice(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: InvoiceDraft,
        entry: InvoiceEntry,
    ) -> Result<InvoiceOutcome, ApplicationError> {
        let invoice = Invoice::new(draft)?;
        let (order, already_entered) = match self
            .purchases()
            .find_by_invoice_key(company_id, invoice.access_key())
            .await?
        {
            Some(order) => (order, true),
            None => {
                // Before the order that claims the key is stored: a day in a closed month would otherwise leave
                // an approved order behind a refused request ([ADR 0068]).
                let arrived = match entry.received_on.as_deref() {
                    Some(raw) => CalendarDate::parse(raw),
                    None => Some(invoice.issued_on()),
                };
                if let Some(day) = arrived {
                    self.ensure_open(company_id, day).await?;
                }
                match self
                    .claim_invoice(company_id, actor, &invoice, &entry)
                    .await
                {
                    Ok(order) => (order, false),
                    // Entered by someone else at the same moment: this is the repeat.
                    Err(ApplicationError::Repository(RepositoryError::DuplicateInvoice)) => (
                        self.purchases()
                            .find_by_invoice_key(company_id, invoice.access_key())
                            .await?
                            .ok_or(ApplicationError::PurchaseNotFound)?,
                        true,
                    ),
                    Err(error) => return Err(error),
                }
            }
        };
        let receipt = self
            .complete_invoice(company_id, actor, &invoice, &entry, order.id())
            .await?;
        let order = self.get(company_id, order.id()).await?;
        Ok(InvoiceOutcome {
            order,
            receipt,
            already_entered,
        })
    }

    /// Writes the order the invoice becomes, a draft that says which invoice it is.
    async fn claim_invoice(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        invoice: &Invoice,
        entry: &InvoiceEntry,
    ) -> Result<PurchaseOrder, ApplicationError> {
        let supplier = match entry.supplier_id {
            Some(supplier) => supplier,
            None => self
                .references()
                .party_with_document(company_id, invoice.issuer())
                .await?
                .ok_or(ApplicationError::SupplierNotRegistered)?,
        };
        let mut products: Vec<ProductId> = Vec::with_capacity(invoice.items().len());
        for (index, item) in invoice.items().iter().enumerate() {
            let chosen = match entry.products.get(index).copied().flatten() {
                Some(product) => Some(product),
                None => {
                    self.references()
                        .product_with_code(company_id, item.code())
                        .await?
                }
            };
            products.push(chosen.ok_or(ApplicationError::InvoiceItemNeedsAProduct(item.number()))?);
        }
        let draft = PurchaseOrderDraft {
            supplier_id: supplier,
            ordered_on: invoice.issued_on().to_string(),
            expected_on: None,
            payment_term_id: entry.payment_term_id,
            category_id: entry.category_id,
            cost_center_id: entry.cost_center_id,
            notes: Some(format!(
                "NF-e {} série {}, {}",
                invoice.number(),
                invoice.series(),
                invoice.issuer_name()
            )),
            lines: invoice.lines(&products),
        };
        let order = PurchaseOrder::new(PurchaseOrderId::generate(), company_id, draft)?
            .from_invoice(invoice.access_key().clone());
        self.check_references(&order, None).await?;
        let audit = audit_entry(
            &order,
            actor,
            AuditAction::Created,
            purchase_order_creation_changes(&order),
        );
        let number = self.purchases().save(&order, &audit).await?;
        Ok(order.with_number(number))
    }

    /// Approves the order and receives all of it, once: from whatever state a failure left it in.
    async fn complete_invoice(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        invoice: &Invoice,
        entry: &InvoiceEntry,
        id: PurchaseOrderId,
    ) -> Result<ReceiptRecord, ApplicationError> {
        let mut order = self.get(company_id, id).await?;
        if order.status() == PurchaseStatus::Draft {
            match self.approve(company_id, actor, id).await {
                Ok(_)
                | Err(
                    ApplicationError::PurchaseChanged
                    | ApplicationError::Domain(DomainError::PurchaseIsLocked),
                ) => {}
                Err(error) => return Err(error),
            }
            order = self.get(company_id, id).await?;
        }
        if let Some(made) = self.receipts(company_id, id).await?.into_iter().next() {
            // Received already: only what it lacks is finished.
            return self
                .finish_receipt(company_id, actor, id, made.receipt.id())
                .await;
        }
        if order.status() != PurchaseStatus::Approved {
            return Err(ApplicationError::PurchaseChanged);
        }
        let received_on = entry
            .received_on
            .clone()
            .unwrap_or_else(|| invoice.issued_on().to_string());
        let items = (1..=order.lines().len())
            .zip(order.lines())
            .map(|(number, line)| ReceiptItemDraft {
                line: u16::try_from(number).unwrap_or(u16::MAX),
                quantity: line.quantity().to_string(),
            })
            .collect();
        let schedule: Vec<ScheduleDraft> = invoice
            .installments()
            .iter()
            .map(|(due, amount)| ScheduleDraft {
                due_date: due.to_string(),
                amount: amount.normalize().to_string(),
            })
            .collect();
        match self
            .receive_scheduled(
                company_id,
                actor,
                id,
                PurchaseReceiptDraft {
                    received_on,
                    notes: Some(format!("NF-e {}", invoice.number())),
                    items,
                },
                &schedule,
            )
            .await
        {
            // Received by someone entering the same invoice at the same moment: that is the receipt, and what it
            // lacks is finished.
            Err(
                ApplicationError::PurchaseChanged
                | ApplicationError::Domain(
                    DomainError::PurchaseIsNotReceivable | DomainError::ReceiptExceedsOrdered,
                ),
            ) => {
                let made = self
                    .receipts(company_id, id)
                    .await?
                    .into_iter()
                    .next()
                    .ok_or(ApplicationError::PurchaseChanged)?;
                self.finish_receipt(company_id, actor, id, made.receipt.id())
                    .await
            }
            other => other,
        }
    }
}
