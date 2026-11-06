use rust_decimal::Decimal;

use crate::financial_entry::clean_description;
use crate::line::{lines_total, validated_lines};
use crate::name::optional_text;
use crate::{
    Amount, CalendarDate, CategoryId, CompanyId, ContractCycle, CostCenterId, DomainError,
    EntryInstallmentDraft, EntryKind, FinancialEntryDraft, Line, LineDraft, PartyId, PaymentTerm,
    PaymentTermId, QuoteId, SaleId, SaleReturnId, round_to_cents,
};

const MAX_NOTES_CHARS: usize = 2000;

/// Where a sale is in its life ([ADR 0039]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaleStatus {
    /// An order: agreed, not billed yet, still open to change.
    Open,
    /// Billed: its receivable exists and its items are fixed.
    Completed,
    Cancelled,
}

impl SaleStatus {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "open" => Some(Self::Open),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Unvalidated input for writing or changing a sale.
#[derive(Debug, Clone)]
pub struct SaleDraft {
    pub customer_id: PartyId,
    pub seller_id: Option<PartyId>,
    pub sold_on: String,
    pub payment_term_id: Option<PaymentTermId>,
    /// The category of the receivable the sale makes.
    pub category_id: CategoryId,
    pub cost_center_id: Option<CostCenterId>,
    pub notes: Option<String>,
    pub lines: Vec<LineDraft>,
}

/// An order that becomes a sale: items sold to a customer on a day, billed by a payment term into one
/// receivable ([ADR 0039]). Its total is the sum of the lines and is never stored.
///
/// Whether what it points at is usable is the use case's to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sale {
    id: SaleId,
    company_id: CompanyId,
    /// The company's own sequence, given by the store when the sale is first written; 0 before.
    number: i64,
    customer_id: PartyId,
    seller_id: Option<PartyId>,
    sold_on: CalendarDate,
    payment_term_id: Option<PaymentTermId>,
    category_id: CategoryId,
    cost_center_id: Option<CostCenterId>,
    notes: Option<String>,
    lines: Vec<Line>,
    status: SaleStatus,
    /// The quote it was made from, which never changes.
    quote_id: Option<QuoteId>,
    /// The cycle of a contract it bills, which never changes.
    cycle: Option<ContractCycle>,
}

impl Sale {
    pub fn new(id: SaleId, company_id: CompanyId, draft: SaleDraft) -> Result<Self, DomainError> {
        let sold_on =
            CalendarDate::parse(&draft.sold_on).ok_or(DomainError::InvalidDate("sold_on"))?;
        let notes = optional_text(
            draft.notes.as_deref(),
            MAX_NOTES_CHARS,
            DomainError::InvalidQuoteNotes,
        )?;
        Ok(Self {
            id,
            company_id,
            number: 0,
            customer_id: draft.customer_id,
            seller_id: draft.seller_id,
            sold_on,
            payment_term_id: draft.payment_term_id,
            category_id: draft.category_id,
            cost_center_id: draft.cost_center_id,
            notes,
            lines: validated_lines(&draft.lines)?,
            status: SaleStatus::Open,
            quote_id: None,
            cycle: None,
        })
    }

    /// The sale as it was stored: its number, its status and the quote it came from.
    pub fn restored(mut self, number: i64, status: SaleStatus, quote_id: Option<QuoteId>) -> Self {
        self.number = number;
        self.status = status;
        self.quote_id = quote_id;
        self
    }

    pub fn with_number(mut self, number: i64) -> Self {
        self.number = number;
        self
    }

    /// The same sale, said to bill a cycle of a contract.
    pub fn billing(mut self, cycle: ContractCycle) -> Self {
        self.cycle = Some(cycle);
        self
    }

    /// The sale as it was stored, with the cycle it bills.
    pub fn with_cycle(mut self, cycle: Option<ContractCycle>) -> Self {
        self.cycle = cycle;
        self
    }

    /// The cycle of a contract the sale bills, if it does.
    pub fn cycle(&self) -> Option<ContractCycle> {
        self.cycle
    }

    /// The same sale, said to have been made from a quote.
    pub fn from_quote(mut self, quote_id: QuoteId) -> Self {
        self.quote_id = Some(quote_id);
        self
    }

    /// The sale with its content replaced by the draft's; only an order that is still open can change.
    /// A draft that changes nothing is the sale itself.
    pub fn replaced_with(&self, draft: SaleDraft) -> Result<Self, DomainError> {
        if self.status != SaleStatus::Open {
            return Err(DomainError::SaleIsLocked);
        }
        let replaced = Self::new(self.id, self.company_id, draft)?
            .restored(self.number, self.status, self.quote_id)
            .with_cycle(self.cycle);
        Ok(replaced)
    }

    /// The sale with only its status changed: what a store does when it records a transition.
    pub fn with_status(&self, status: SaleStatus) -> Self {
        Self {
            status,
            ..self.clone()
        }
    }

    /// The sale billed: it must be open and worth something, since a receivable has installments above 0.
    pub fn completed(&self) -> Result<Self, DomainError> {
        if self.status != SaleStatus::Open {
            return Err(DomainError::SaleIsLocked);
        }
        if self.total() <= Decimal::ZERO {
            return Err(DomainError::SaleIsEmpty);
        }
        Ok(Self {
            status: SaleStatus::Completed,
            ..self.clone()
        })
    }

    /// The sale cancelled, from an open order or from a completed sale (whose receivable the use case
    /// voids). Once cancelled it is not reopened.
    pub fn cancelled(&self) -> Result<Self, DomainError> {
        if self.status == SaleStatus::Cancelled {
            return Err(DomainError::SaleIsLocked);
        }
        Ok(Self {
            status: SaleStatus::Cancelled,
            ..self.clone()
        })
    }

    /// The receivable of the sale: one entry whose installments come from the payment term, or one that falls
    /// due on the day of the sale when it has none.
    pub fn receivable(
        &self,
        term: Option<&PaymentTerm>,
    ) -> Result<FinancialEntryDraft, DomainError> {
        let total = self.total();
        let installments = match term {
            Some(term) => term.split(total, self.sold_on)?,
            None => vec![EntryInstallmentDraft {
                due_date: self.sold_on.to_string(),
                amount: total.to_string(),
            }],
        };
        Ok(FinancialEntryDraft {
            kind: EntryKind::Receivable,
            description: clean_description(&format!("Venda nº {}", self.number))?,
            party_id: Some(self.customer_id),
            category_id: self.category_id,
            cost_center_id: self.cost_center_id,
            competence_date: self.sold_on.to_string(),
            installments,
        })
    }

    pub fn id(&self) -> SaleId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn number(&self) -> i64 {
        self.number
    }

    pub fn customer_id(&self) -> PartyId {
        self.customer_id
    }

    pub fn seller_id(&self) -> Option<PartyId> {
        self.seller_id
    }

    pub fn sold_on(&self) -> CalendarDate {
        self.sold_on
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

    pub fn status(&self) -> SaleStatus {
        self.status
    }

    pub fn quote_id(&self) -> Option<QuoteId> {
        self.quote_id
    }

    /// The sum of the lines, exact.
    pub fn total(&self) -> Decimal {
        lines_total(&self.lines)
    }
}

/// One line of the sale that comes back, and how much of it.
#[derive(Debug, Clone)]
pub struct ReturnItemDraft {
    /// The line's place in the sale, from 1.
    pub line: u16,
    pub quantity: String,
}

/// Unvalidated input for recording a return.
#[derive(Debug, Clone)]
pub struct SaleReturnDraft {
    pub returned_on: String,
    pub reason: Option<String>,
    pub items: Vec<ReturnItemDraft>,
}

/// What came back of a completed sale: some quantity of some of its lines, and what is refunded for it
/// ([ADR 0039]). It is never edited or deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleReturn {
    id: SaleReturnId,
    company_id: CompanyId,
    sale_id: SaleId,
    returned_on: CalendarDate,
    reason: Option<String>,
    items: Vec<(u16, Amount)>,
    refund: Amount,
}

impl SaleReturn {
    /// A return of `sale`, given what was returned of each of its lines before (`(line, quantity)`).
    /// The refund of a line is its total times the returned part of its quantity, taken **cumulatively**:
    /// what has been returned so far rounded to the cent, less what was refunded before, so that the parts
    /// of a line add up to the line exactly.
    pub fn new(
        id: SaleReturnId,
        sale: &Sale,
        draft: SaleReturnDraft,
        returned_before: &[(u16, Decimal)],
    ) -> Result<Self, DomainError> {
        if sale.status() != SaleStatus::Completed {
            return Err(DomainError::SaleIsNotCompleted);
        }
        let returned_on = CalendarDate::parse(&draft.returned_on)
            .ok_or(DomainError::InvalidDate("returned_on"))?;
        let reason = optional_text(draft.reason.as_deref(), 200, DomainError::InvalidNote)?;
        if draft.items.is_empty() {
            return Err(DomainError::NoReturnItems);
        }
        let mut items: Vec<(u16, Amount)> = Vec::new();
        let mut refund = Decimal::ZERO;
        for item in &draft.items {
            let line = usize::from(item.line)
                .checked_sub(1)
                .and_then(|index| sale.lines().get(index))
                .filter(|_| items.iter().all(|(seen, _)| *seen != item.line))
                .ok_or(DomainError::ReturnLineUnknown)?;
            let quantity = Amount::parse(item.quantity.trim())
                .filter(|q| q.as_decimal() > Decimal::ZERO && q.as_decimal().scale() <= 3)
                .ok_or(DomainError::InvalidReturnQuantity)?;
            let before: Decimal = returned_before
                .iter()
                .filter(|(number, _)| *number == item.line)
                .map(|(_, quantity)| *quantity)
                .sum();
            let after = before + quantity.as_decimal();
            let sold = line.quantity().as_decimal();
            if after > sold {
                return Err(DomainError::ReturnExceedsSold);
            }
            let up_to = |returned: Decimal| round_to_cents(line.total() * returned / sold);
            refund += up_to(after) - up_to(before);
            items.push((item.line, quantity));
        }
        let refund = Amount::in_cents(&refund.normalize().to_string())
            .filter(|refund| refund.as_decimal() > Decimal::ZERO)
            .ok_or(DomainError::ReturnRefundIsNothing)?;
        Ok(Self {
            id,
            company_id: sale.company_id(),
            sale_id: sale.id(),
            returned_on,
            reason,
            items,
            refund,
        })
    }

    /// A return as it was stored: what it refunded is a fact of the day it was made, not worked out again.
    pub fn restored(
        id: SaleReturnId,
        company_id: CompanyId,
        sale_id: SaleId,
        returned_on: CalendarDate,
        reason: Option<String>,
        items: Vec<(u16, Amount)>,
        refund: Amount,
    ) -> Self {
        Self {
            id,
            company_id,
            sale_id,
            returned_on,
            reason,
            items,
            refund,
        }
    }

    /// The payable that gives the money back: one installment of the refund due on `due_on`, to the customer.
    pub fn refund_entry(
        &self,
        sale: &Sale,
        category_id: CategoryId,
        due_on: &str,
    ) -> Result<FinancialEntryDraft, DomainError> {
        Ok(FinancialEntryDraft {
            kind: EntryKind::Payable,
            description: clean_description(&format!("Devolução da venda nº {}", sale.number()))?,
            party_id: Some(sale.customer_id()),
            category_id,
            cost_center_id: sale.cost_center_id(),
            competence_date: self.returned_on.to_string(),
            installments: vec![EntryInstallmentDraft {
                due_date: due_on.to_owned(),
                amount: self.refund.to_string(),
            }],
        })
    }

    pub fn id(&self) -> SaleReturnId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn sale_id(&self) -> SaleId {
        self.sale_id
    }

    pub fn returned_on(&self) -> CalendarDate {
        self.returned_on
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// `(line, quantity)`, in the order they were given.
    pub fn items(&self) -> &[(u16, Amount)] {
        &self.items
    }

    pub fn refund(&self) -> Amount {
        self.refund
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

    fn draft(lines: Vec<LineDraft>) -> SaleDraft {
        SaleDraft {
            customer_id: PartyId::generate(),
            seller_id: None,
            sold_on: "2026-11-06".into(),
            payment_term_id: None,
            category_id: CategoryId::generate(),
            cost_center_id: None,
            notes: None,
            lines,
        }
    }

    fn sale(lines: Vec<LineDraft>) -> Sale {
        Sale::new(SaleId::generate(), CompanyId::generate(), draft(lines))
            .unwrap()
            .with_number(12)
    }

    #[test]
    fn an_order_is_completed_once_and_cancelled_from_either_state() {
        let order = sale(vec![line("2", "10", "0")]);
        assert_eq!(order.status(), SaleStatus::Open);
        let done = order.completed().unwrap();
        assert_eq!(done.status(), SaleStatus::Completed);
        assert_eq!(done.completed().unwrap_err(), DomainError::SaleIsLocked);
        assert_eq!(
            done.replaced_with(draft(vec![line("1", "1", "0")]))
                .unwrap_err(),
            DomainError::SaleIsLocked
        );
        assert_eq!(order.cancelled().unwrap().status(), SaleStatus::Cancelled);
        let gone = done.cancelled().unwrap();
        assert_eq!(gone.status(), SaleStatus::Cancelled);
        assert_eq!(gone.cancelled().unwrap_err(), DomainError::SaleIsLocked);
        // A sale worth nothing has no receivable to make.
        let free = sale(vec![line("1", "10", "10")]);
        assert_eq!(free.completed().unwrap_err(), DomainError::SaleIsEmpty);
    }

    #[test]
    fn the_receivable_is_one_entry_split_by_the_term_or_due_on_the_day_of_the_sale() {
        let order = sale(vec![line("3", "33.33", "0.01")]);
        // 99.99 - 0.01 = 99.98.
        let single = order.receivable(None).unwrap();
        assert_eq!(single.description, "Venda nº 12");
        assert_eq!(single.installments.len(), 1);
        assert_eq!(single.installments[0].due_date, "2026-11-06");
        assert_eq!(single.installments[0].amount, "99.98");
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
        let split = order.receivable(Some(&term)).unwrap();
        assert_eq!(split.kind, EntryKind::Receivable);
        assert_eq!(split.party_id, Some(order.customer_id()));
        let parts: Vec<(&str, &str)> = split
            .installments
            .iter()
            .map(|p| (p.due_date.as_str(), p.amount.as_str()))
            .collect();
        assert_eq!(parts, [("2026-11-06", "49.99"), ("2026-12-06", "49.99")]);
    }

    fn return_of(
        sale: &Sale,
        items: &[(u16, &str)],
        before: &[(u16, &str)],
    ) -> Result<SaleReturn, DomainError> {
        SaleReturn::new(
            SaleReturnId::generate(),
            sale,
            SaleReturnDraft {
                returned_on: "2026-11-10".into(),
                reason: Some("  Estragou  ".into()),
                items: items
                    .iter()
                    .map(|(line, quantity)| ReturnItemDraft {
                        line: *line,
                        quantity: (*quantity).into(),
                    })
                    .collect(),
            },
            &before
                .iter()
                .map(|(line, q)| (*line, q.parse().unwrap()))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn a_return_refunds_the_returned_part_of_a_line_and_the_parts_add_up_to_the_line() {
        // 3 x 3.33 = 9.99 for the line.
        let done = sale(vec![line("3", "3.33", "0"), line("1", "5", "0")])
            .completed()
            .unwrap();
        let first = return_of(&done, &[(1, "1")], &[]).unwrap();
        assert_eq!(first.reason(), Some("Estragou"));
        assert_eq!(first.refund().to_string(), "3.33");
        // Each third of 10.00 rounds on its own, so the cumulative rule is what makes the thirds add up.
        let thirds = sale(vec![line("3", "3.3333", "0")]).completed().unwrap();
        let a = return_of(&thirds, &[(1, "1")], &[]).unwrap().refund();
        let b = return_of(&thirds, &[(1, "1")], &[(1, "1")])
            .unwrap()
            .refund();
        let c = return_of(&thirds, &[(1, "1")], &[(1, "2")])
            .unwrap()
            .refund();
        // 3 x 3.3333 = 9.9999 -> 10.00 for the line: 3.33 + 3.34 + 3.33.
        assert_eq!(
            [a.to_string(), b.to_string(), c.to_string()],
            ["3.33", "3.34", "3.33"]
        );
        // Nothing past what was sold, nothing of a line the sale does not have or twice in one return.
        assert_eq!(
            return_of(&done, &[(1, "2.001")], &[(1, "1")]).unwrap_err(),
            DomainError::ReturnExceedsSold
        );
        assert_eq!(
            return_of(&done, &[(3, "1")], &[]).unwrap_err(),
            DomainError::ReturnLineUnknown
        );
        assert_eq!(
            return_of(&done, &[(1, "1"), (1, "1")], &[]).unwrap_err(),
            DomainError::ReturnLineUnknown
        );
        assert_eq!(
            return_of(&done, &[(1, "0")], &[]).unwrap_err(),
            DomainError::InvalidReturnQuantity
        );
        assert_eq!(
            return_of(&done, &[], &[]).unwrap_err(),
            DomainError::NoReturnItems
        );
        // A sale that was not completed has no returns.
        let open = sale(vec![line("1", "5", "0")]);
        assert_eq!(
            return_of(&open, &[(1, "1")], &[]).unwrap_err(),
            DomainError::SaleIsNotCompleted
        );
        // The refund is a payable to the customer, due where the person said.
        let entry = first
            .refund_entry(&done, CategoryId::generate(), "2026-11-20")
            .unwrap();
        assert_eq!(entry.kind, EntryKind::Payable);
        assert_eq!(entry.description, "Devolução da venda nº 12");
        assert_eq!(entry.installments[0].amount, "3.33");
        assert_eq!(entry.competence_date, "2026-11-10");
    }
}
