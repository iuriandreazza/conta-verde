use rust_decimal::Decimal;

use crate::financial_entry::clean_description;
use crate::line::{lines_total, validated_lines};
use crate::name::optional_text;
use crate::{
    Amount, CalendarDate, CategoryId, CompanyId, CostCenterId, DomainError, EntryInstallmentDraft,
    EntryKind, FinancialEntryDraft, Line, LineDraft, PartyId, PaymentTerm, PaymentTermId,
    PurchaseOrderId, PurchaseReceiptId, round_to_cents,
};

const MAX_NOTES_CHARS: usize = 2000;

/// Where a purchase order is in its life ([ADR 0053]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseStatus {
    /// Being written: the only state in which it can change.
    Draft,
    /// Agreed with the supplier; its items are fixed and goods can be received.
    Approved,
    /// Some of what was ordered has arrived and some has not.
    PartiallyReceived,
    /// Everything ordered arrived, or what is left was given up.
    Received,
    Cancelled,
}

impl PurchaseStatus {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::PartiallyReceived => "partially_received",
            Self::Received => "received",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "draft" => Some(Self::Draft),
            "approved" => Some(Self::Approved),
            "partially_received" => Some(Self::PartiallyReceived),
            "received" => Some(Self::Received),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Unvalidated input for writing or changing a purchase order. A line's `unit_price` is the unit cost.
#[derive(Debug, Clone)]
pub struct PurchaseOrderDraft {
    pub supplier_id: PartyId,
    pub ordered_on: String,
    pub expected_on: Option<String>,
    pub payment_term_id: Option<PaymentTermId>,
    /// The category of the payables the receipts make.
    pub category_id: CategoryId,
    pub cost_center_id: Option<CostCenterId>,
    pub notes: Option<String>,
    pub lines: Vec<LineDraft>,
}

/// Goods ordered from a supplier, to arrive on some day and be paid by a payment term ([ADR 0053]). Its
/// total is the sum of the lines and is never stored; each receipt makes its own payable.
///
/// Whether what it points at is usable is the use case's to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseOrder {
    id: PurchaseOrderId,
    company_id: CompanyId,
    /// The company's own sequence, given by the store when the order is first written; 0 before.
    number: i64,
    supplier_id: PartyId,
    ordered_on: CalendarDate,
    expected_on: Option<CalendarDate>,
    payment_term_id: Option<PaymentTermId>,
    category_id: CategoryId,
    cost_center_id: Option<CostCenterId>,
    notes: Option<String>,
    lines: Vec<Line>,
    status: PurchaseStatus,
}

impl PurchaseOrder {
    pub fn new(
        id: PurchaseOrderId,
        company_id: CompanyId,
        draft: PurchaseOrderDraft,
    ) -> Result<Self, DomainError> {
        let ordered_on =
            CalendarDate::parse(&draft.ordered_on).ok_or(DomainError::InvalidDate("ordered_on"))?;
        let expected_on = draft
            .expected_on
            .as_deref()
            .map(|raw| CalendarDate::parse(raw).ok_or(DomainError::InvalidDate("expected_on")))
            .transpose()?;
        if expected_on.is_some_and(|expected| expected < ordered_on) {
            return Err(DomainError::ExpectedBeforeOrdered);
        }
        let notes = optional_text(
            draft.notes.as_deref(),
            MAX_NOTES_CHARS,
            DomainError::InvalidQuoteNotes,
        )?;
        Ok(Self {
            id,
            company_id,
            number: 0,
            supplier_id: draft.supplier_id,
            ordered_on,
            expected_on,
            payment_term_id: draft.payment_term_id,
            category_id: draft.category_id,
            cost_center_id: draft.cost_center_id,
            notes,
            lines: validated_lines(&draft.lines)?,
            status: PurchaseStatus::Draft,
        })
    }

    /// The order as it was stored: its number and its status.
    pub fn restored(mut self, number: i64, status: PurchaseStatus) -> Self {
        self.number = number;
        self.status = status;
        self
    }

    pub fn with_number(mut self, number: i64) -> Self {
        self.number = number;
        self
    }

    /// The order with its content replaced by the draft's; only a draft can change. A draft that changes
    /// nothing is the order itself.
    pub fn replaced_with(&self, draft: PurchaseOrderDraft) -> Result<Self, DomainError> {
        if self.status != PurchaseStatus::Draft {
            return Err(DomainError::PurchaseIsLocked);
        }
        Ok(Self::new(self.id, self.company_id, draft)?.restored(self.number, self.status))
    }

    /// The order with only its status changed: what a store does when it records a transition.
    pub fn with_status(&self, status: PurchaseStatus) -> Self {
        Self {
            status,
            ..self.clone()
        }
    }

    /// The draft agreed: it must be worth something, since a payable has installments above 0.
    pub fn approved(&self) -> Result<Self, DomainError> {
        if self.status != PurchaseStatus::Draft {
            return Err(DomainError::PurchaseIsLocked);
        }
        if self.total() <= Decimal::ZERO {
            return Err(DomainError::PurchaseIsEmpty);
        }
        Ok(self.with_status(PurchaseStatus::Approved))
    }

    /// The order cancelled: only before anything arrived. Once cancelled it is not reopened.
    pub fn cancelled(&self) -> Result<Self, DomainError> {
        match self.status {
            PurchaseStatus::Draft | PurchaseStatus::Approved => {
                Ok(self.with_status(PurchaseStatus::Cancelled))
            }
            _ => Err(DomainError::PurchaseIsLocked),
        }
    }

    /// A partly received order closed: what has not arrived is given up.
    pub fn closed(&self) -> Result<Self, DomainError> {
        if self.status != PurchaseStatus::PartiallyReceived {
            return Err(DomainError::PurchaseIsLocked);
        }
        Ok(self.with_status(PurchaseStatus::Received))
    }

    /// The status the order has once `receipt` is in, given what each line had received before it: received
    /// when every line has arrived whole, partly received otherwise.
    pub fn status_after(
        &self,
        receipt: &PurchaseReceipt,
        received_before: &[(u16, Decimal)],
    ) -> PurchaseStatus {
        let whole = self.lines.iter().enumerate().all(|(index, line)| {
            let number = u16::try_from(index + 1).unwrap_or(u16::MAX);
            let before: Decimal = received_before
                .iter()
                .filter(|(seen, _)| *seen == number)
                .map(|(_, quantity)| *quantity)
                .sum();
            let now: Decimal = receipt
                .items
                .iter()
                .filter(|item| item.line == number)
                .map(|item| item.quantity.as_decimal())
                .sum();
            before + now >= line.quantity().as_decimal()
        });
        if whole {
            PurchaseStatus::Received
        } else {
            PurchaseStatus::PartiallyReceived
        }
    }

    /// The payable of one receipt: one entry whose installments come from the payment term, counted from the
    /// day of the receipt, or one that falls due that day when there is no term.
    pub fn payable_of(
        &self,
        receipt: &PurchaseReceipt,
        term: Option<&PaymentTerm>,
    ) -> Result<FinancialEntryDraft, DomainError> {
        let total = receipt.total();
        let installments = match term {
            Some(term) => term.split(total, receipt.received_on)?,
            None => vec![EntryInstallmentDraft {
                due_date: receipt.received_on.to_string(),
                amount: total.to_string(),
            }],
        };
        Ok(FinancialEntryDraft {
            kind: EntryKind::Payable,
            description: clean_description(&format!("Compra nº {}", self.number))?,
            party_id: Some(self.supplier_id),
            category_id: self.category_id,
            cost_center_id: self.cost_center_id,
            competence_date: receipt.received_on.to_string(),
            installments,
        })
    }

    pub fn id(&self) -> PurchaseOrderId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn number(&self) -> i64 {
        self.number
    }

    pub fn supplier_id(&self) -> PartyId {
        self.supplier_id
    }

    pub fn ordered_on(&self) -> CalendarDate {
        self.ordered_on
    }

    pub fn expected_on(&self) -> Option<CalendarDate> {
        self.expected_on
    }

    pub fn payment_term_id(&self) -> Option<PaymentTermId> {
        self.payment_term_id
    }

    pub fn category_id(&self) -> CategoryId {
        self.category_id
    }

    pub fn cost_center_id(&self) -> Option<CostCenterId> {
        self.cost_center_id
    }

    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn status(&self) -> PurchaseStatus {
        self.status
    }

    /// The sum of the lines, exact.
    pub fn total(&self) -> Decimal {
        lines_total(&self.lines)
    }
}

/// One line of the order that arrived, and how much of it.
#[derive(Debug, Clone)]
pub struct ReceiptItemDraft {
    /// The line's place in the order, from 1.
    pub line: u16,
    pub quantity: String,
}

/// Unvalidated input for recording a receipt.
#[derive(Debug, Clone)]
pub struct PurchaseReceiptDraft {
    pub received_on: String,
    pub notes: Option<String>,
    pub items: Vec<ReceiptItemDraft>,
}

/// What arrived of one line, and what it cost: a fact of the day of the receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptItem {
    pub line: u16,
    pub quantity: Amount,
    /// The line's net total for this quantity, in cents, taken cumulatively so the receipts of a line add up to
    /// the line exactly. It is the cost of what entered stock ([ADR 0052]).
    pub value: Amount,
}

/// What arrived of an approved order: some quantity of some of its lines ([ADR 0053]). It is never edited
/// or deleted, and it makes one payable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseReceipt {
    id: PurchaseReceiptId,
    company_id: CompanyId,
    purchase_id: PurchaseOrderId,
    received_on: CalendarDate,
    notes: Option<String>,
    items: Vec<ReceiptItem>,
}

impl PurchaseReceipt {
    /// A receipt of `order`, given what was received of each of its lines before (`(line, quantity)`). The value
    /// of a line is its total times the received part of its quantity, taken **cumulatively**: what has been
    /// received so far rounded to the cent, less what was valued before.
    pub fn new(
        id: PurchaseReceiptId,
        order: &PurchaseOrder,
        draft: PurchaseReceiptDraft,
        received_before: &[(u16, Decimal)],
    ) -> Result<Self, DomainError> {
        if !matches!(
            order.status(),
            PurchaseStatus::Approved | PurchaseStatus::PartiallyReceived
        ) {
            return Err(DomainError::PurchaseIsNotReceivable);
        }
        let received_on = CalendarDate::parse(&draft.received_on)
            .ok_or(DomainError::InvalidDate("received_on"))?;
        let notes = optional_text(draft.notes.as_deref(), 200, DomainError::InvalidNote)?;
        if draft.items.is_empty() {
            return Err(DomainError::NoReceiptItems);
        }
        let mut items: Vec<ReceiptItem> = Vec::new();
        for item in &draft.items {
            let line = usize::from(item.line)
                .checked_sub(1)
                .and_then(|index| order.lines().get(index))
                .filter(|_| items.iter().all(|seen| seen.line != item.line))
                .ok_or(DomainError::ReceiptLineUnknown)?;
            let quantity = Amount::parse(item.quantity.trim())
                .filter(|q| q.as_decimal() > Decimal::ZERO && q.as_decimal().scale() <= 3)
                .ok_or(DomainError::InvalidReceiptQuantity)?;
            let before: Decimal = received_before
                .iter()
                .filter(|(number, _)| *number == item.line)
                .map(|(_, quantity)| *quantity)
                .sum();
            let after = before + quantity.as_decimal();
            let ordered = line.quantity().as_decimal();
            if after > ordered {
                return Err(DomainError::ReceiptExceedsOrdered);
            }
            let up_to = |received: Decimal| round_to_cents(line.total() * received / ordered);
            let value = Amount::in_cents(&(up_to(after) - up_to(before)).normalize().to_string())
                .ok_or(DomainError::DocumentTooLarge)?;
            items.push(ReceiptItem {
                line: item.line,
                quantity,
                value,
            });
        }
        Ok(Self {
            id,
            company_id: order.company_id(),
            purchase_id: order.id(),
            received_on,
            notes,
            items,
        })
    }

    /// A receipt as it was stored: what it was worth is a fact of the day it was made, not worked out again.
    pub fn restored(
        id: PurchaseReceiptId,
        company_id: CompanyId,
        purchase_id: PurchaseOrderId,
        received_on: CalendarDate,
        notes: Option<String>,
        items: Vec<ReceiptItem>,
    ) -> Self {
        Self {
            id,
            company_id,
            purchase_id,
            received_on,
            notes,
            items,
        }
    }

    pub fn id(&self) -> PurchaseReceiptId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn purchase_id(&self) -> PurchaseOrderId {
        self.purchase_id
    }

    pub fn received_on(&self) -> CalendarDate {
        self.received_on
    }

    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }

    /// In the order they were given.
    pub fn items(&self) -> &[ReceiptItem] {
        &self.items
    }

    /// What the receipt is worth: the sum of its items, which is what its payable pays.
    pub fn total(&self) -> Decimal {
        self.items
            .iter()
            .map(|item| item.value.as_decimal())
            .sum::<Decimal>()
            .normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InstallmentDraft, PaymentTermDraft, ProductId};

    fn line(quantity: &str, price: &str, discount: &str) -> LineDraft {
        LineDraft {
            product_id: ProductId::generate(),
            description: "Item".into(),
            quantity: quantity.into(),
            unit_price: price.into(),
            discount: discount.into(),
        }
    }

    fn draft(lines: Vec<LineDraft>) -> PurchaseOrderDraft {
        PurchaseOrderDraft {
            supplier_id: PartyId::generate(),
            ordered_on: "2026-11-06".into(),
            expected_on: Some("2026-11-20".into()),
            payment_term_id: None,
            category_id: CategoryId::generate(),
            cost_center_id: None,
            notes: None,
            lines,
        }
    }

    fn order(lines: Vec<LineDraft>) -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::generate(),
            CompanyId::generate(),
            draft(lines),
        )
        .unwrap()
        .with_number(7)
    }

    fn receipt_draft(items: &[(u16, &str)]) -> PurchaseReceiptDraft {
        PurchaseReceiptDraft {
            received_on: "2026-11-10".into(),
            notes: None,
            items: items
                .iter()
                .map(|(line, quantity)| ReceiptItemDraft {
                    line: *line,
                    quantity: (*quantity).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_draft_is_approved_once_and_cancelled_only_before_anything_arrives() {
        let draft_order = order(vec![line("2", "10", "0")]);
        assert_eq!(draft_order.status(), PurchaseStatus::Draft);
        let approved = draft_order.approved().unwrap();
        assert_eq!(approved.status(), PurchaseStatus::Approved);
        assert_eq!(
            approved.approved().unwrap_err(),
            DomainError::PurchaseIsLocked
        );
        assert_eq!(
            approved
                .replaced_with(draft(vec![line("1", "1", "0")]))
                .unwrap_err(),
            DomainError::PurchaseIsLocked
        );
        assert_eq!(
            draft_order.cancelled().unwrap().status(),
            PurchaseStatus::Cancelled
        );
        assert_eq!(
            approved.cancelled().unwrap().status(),
            PurchaseStatus::Cancelled
        );
        let partial = approved.with_status(PurchaseStatus::PartiallyReceived);
        assert_eq!(
            partial.cancelled().unwrap_err(),
            DomainError::PurchaseIsLocked
        );
        assert_eq!(partial.closed().unwrap().status(), PurchaseStatus::Received);
        assert_eq!(
            approved.closed().unwrap_err(),
            DomainError::PurchaseIsLocked
        );
        // A purchase worth nothing has no payable to make.
        let free = order(vec![line("1", "10", "10")]);
        assert_eq!(free.approved().unwrap_err(), DomainError::PurchaseIsEmpty);
    }

    #[test]
    fn the_expected_day_is_not_before_the_day_of_the_order() {
        let mut early = draft(vec![line("1", "1", "0")]);
        early.expected_on = Some("2026-11-05".into());
        assert_eq!(
            PurchaseOrder::new(PurchaseOrderId::generate(), CompanyId::generate(), early)
                .unwrap_err(),
            DomainError::ExpectedBeforeOrdered
        );
        let mut none = draft(vec![line("1", "1", "0")]);
        none.expected_on = None;
        assert!(
            PurchaseOrder::new(PurchaseOrderId::generate(), CompanyId::generate(), none).is_ok()
        );
    }

    #[test]
    fn the_value_of_a_line_is_taken_cumulatively_so_receipts_add_up_to_the_line() {
        // 3 x 10 = 30, less 0.01 = 29.99, received in three parts of one.
        let order = order(vec![line("3", "10", "0.01")]).approved().unwrap();
        let mut before: Vec<(u16, Decimal)> = Vec::new();
        let mut values = Vec::new();
        for _ in 0..3 {
            let receipt = PurchaseReceipt::new(
                PurchaseReceiptId::generate(),
                &order,
                receipt_draft(&[(1, "1")]),
                &before,
            )
            .unwrap();
            values.push(receipt.items()[0].value.to_string());
            before.push((1, Decimal::ONE));
        }
        // 9.99667 -> 10, 19.99333 -> 19.99 (so 9.99 more), 29.99 (so 10 more).
        assert_eq!(values, ["10", "9.99", "10"]);
    }

    #[test]
    fn a_receipt_is_of_the_lines_the_order_has_and_no_more_than_is_left() {
        let order = order(vec![line("2", "10", "0"), line("1", "5", "0")])
            .approved()
            .unwrap();
        let attempt = |items: &[(u16, &str)], before: &[(u16, Decimal)]| {
            PurchaseReceipt::new(
                PurchaseReceiptId::generate(),
                &order,
                receipt_draft(items),
                before,
            )
        };
        for (items, error) in [
            (&[][..], DomainError::NoReceiptItems),
            (&[(3, "1")][..], DomainError::ReceiptLineUnknown),
            (&[(0, "1")][..], DomainError::ReceiptLineUnknown),
            (&[(1, "1"), (1, "1")][..], DomainError::ReceiptLineUnknown),
            (&[(1, "0")][..], DomainError::InvalidReceiptQuantity),
            (&[(1, "1.0001")][..], DomainError::InvalidReceiptQuantity),
            (&[(1, "2.001")][..], DomainError::ReceiptExceedsOrdered),
        ] {
            assert_eq!(attempt(items, &[]).unwrap_err(), error, "{items:?}");
        }
        assert_eq!(
            attempt(&[(1, "1")], &[(1, Decimal::new(15, 1))]).unwrap_err(),
            DomainError::ReceiptExceedsOrdered
        );
        let draft_order = self::order(vec![line("1", "1", "0")]);
        assert_eq!(
            PurchaseReceipt::new(
                PurchaseReceiptId::generate(),
                &draft_order,
                receipt_draft(&[(1, "1")]),
                &[]
            )
            .unwrap_err(),
            DomainError::PurchaseIsNotReceivable
        );
    }

    #[test]
    fn the_order_is_received_once_every_line_has_arrived_whole() {
        let order = order(vec![line("2", "10", "0"), line("1", "5", "0")])
            .approved()
            .unwrap();
        let first = PurchaseReceipt::new(
            PurchaseReceiptId::generate(),
            &order,
            receipt_draft(&[(1, "2")]),
            &[],
        )
        .unwrap();
        assert_eq!(first.total().to_string(), "20");
        assert_eq!(
            order.status_after(&first, &[]),
            PurchaseStatus::PartiallyReceived
        );
        let second = PurchaseReceipt::new(
            PurchaseReceiptId::generate(),
            &order.with_status(PurchaseStatus::PartiallyReceived),
            receipt_draft(&[(2, "1")]),
            &[(1, Decimal::from(2))],
        )
        .unwrap();
        assert_eq!(
            order.status_after(&second, &[(1, Decimal::from(2))]),
            PurchaseStatus::Received
        );
    }

    #[test]
    fn the_payable_of_a_receipt_is_split_by_the_term_from_the_day_it_arrived() {
        let order = order(vec![line("3", "33.33", "0")]).approved().unwrap();
        let receipt = PurchaseReceipt::new(
            PurchaseReceiptId::generate(),
            &order,
            receipt_draft(&[(1, "3")]),
            &[],
        )
        .unwrap();
        let single = order.payable_of(&receipt, None).unwrap();
        assert_eq!(single.description, "Compra nº 7");
        assert_eq!(single.kind, EntryKind::Payable);
        assert_eq!(single.party_id, Some(order.supplier_id()));
        assert_eq!(single.installments[0].due_date, "2026-11-10");
        assert_eq!(single.installments[0].amount, "99.99");
        let term = PaymentTerm::new(
            PaymentTermId::generate(),
            order.company_id(),
            PaymentTermDraft {
                name: "0/30".into(),
                installments: vec![
                    InstallmentDraft {
                        days: 0,
                        percent: "50".into(),
                    },
                    InstallmentDraft {
                        days: 30,
                        percent: "50".into(),
                    },
                ],
                active: true,
            },
        )
        .unwrap();
        let split = order.payable_of(&receipt, Some(&term)).unwrap();
        let amounts: Vec<_> = split
            .installments
            .iter()
            .map(|i| i.amount.as_str())
            .collect();
        assert_eq!(amounts, ["49.99", "50"]);
        assert_eq!(split.installments[1].due_date, "2026-12-10");
    }
}
