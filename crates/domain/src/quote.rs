use rust_decimal::Decimal;

use crate::line::{lines_total, validated_lines};
use crate::name::optional_text;
use crate::{
    Actor, CalendarDate, CompanyId, DomainError, Line, LineDraft, PartyId, PaymentTermId, QuoteId,
};

const MAX_NOTES_CHARS: usize = 2000;

/// Where a quote is in its life ([ADR 0038]). `Converted` is set when it becomes a sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStatus {
    /// Being written: nobody outside has seen it.
    Draft,
    /// The customer was sent a link to look at it and answer.
    Sent,
    Approved,
    Declined,
    Converted,
}

impl QuoteStatus {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Sent => "sent",
            Self::Approved => "approved",
            Self::Declined => "declined",
            Self::Converted => "converted",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "draft" => Some(Self::Draft),
            "sent" => Some(Self::Sent),
            "approved" => Some(Self::Approved),
            "declined" => Some(Self::Declined),
            "converted" => Some(Self::Converted),
            _ => None,
        }
    }
}

/// Unvalidated input for writing or changing a quote.
#[derive(Debug, Clone)]
pub struct QuoteDraft {
    pub customer_id: PartyId,
    pub issued_on: String,
    pub valid_until: String,
    pub payment_term_id: Option<PaymentTermId>,
    pub notes: Option<String>,
    pub lines: Vec<LineDraft>,
}

/// What a company offers a customer: lines with discounts, valid until a day, answered by the customer
/// through a link ([ADR 0038]). Its total is the sum of the lines and is never stored.
///
/// Whether the customer, the term and the products are usable is the use case's to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    id: QuoteId,
    company_id: CompanyId,
    /// The company's own sequence, given by the store when the quote is first written; 0 before.
    number: i64,
    customer_id: PartyId,
    issued_on: CalendarDate,
    valid_until: CalendarDate,
    payment_term_id: Option<PaymentTermId>,
    notes: Option<String>,
    lines: Vec<Line>,
    status: QuoteStatus,
    /// Whoever sent it last: the customer's answer is recorded in their name, since the customer is
    /// no user of the system.
    sent_by: Option<Actor>,
}

impl Quote {
    pub fn new(id: QuoteId, company_id: CompanyId, draft: QuoteDraft) -> Result<Self, DomainError> {
        let issued_on =
            CalendarDate::parse(&draft.issued_on).ok_or(DomainError::InvalidDate("issued_on"))?;
        let valid_until = CalendarDate::parse(&draft.valid_until)
            .ok_or(DomainError::InvalidDate("valid_until"))?;
        if valid_until < issued_on {
            return Err(DomainError::QuoteValidityBeforeIssue);
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
            customer_id: draft.customer_id,
            issued_on,
            valid_until,
            payment_term_id: draft.payment_term_id,
            notes,
            lines: validated_lines(&draft.lines)?,
            status: QuoteStatus::Draft,
            sent_by: None,
        })
    }

    /// The quote as it was stored: its number, its status and who sent it.
    pub fn restored(mut self, number: i64, status: QuoteStatus, sent_by: Option<Actor>) -> Self {
        self.number = number;
        self.status = status;
        self.sent_by = sent_by;
        self
    }

    pub fn with_number(mut self, number: i64) -> Self {
        self.number = number;
        self
    }

    /// The quote with its content replaced by the draft's. It is only open to change while it is a
    /// draft or sent; a sent quote that changes is a draft again, because the customer was shown
    /// something else (its link stops working). A draft that changes nothing is the quote itself.
    pub fn replaced_with(&self, draft: QuoteDraft) -> Result<Self, DomainError> {
        if !matches!(self.status, QuoteStatus::Draft | QuoteStatus::Sent) {
            return Err(DomainError::QuoteIsLocked);
        }
        let replaced =
            Self::new(self.id, self.company_id, draft)?.restored(self.number, self.status, None);
        if replaced.same_content(self) {
            return Ok(self.clone());
        }
        Ok(Self {
            status: QuoteStatus::Draft,
            sent_by: self.sent_by.clone(),
            ..replaced
        })
    }

    fn same_content(&self, other: &Self) -> bool {
        self.customer_id == other.customer_id
            && self.issued_on == other.issued_on
            && self.valid_until == other.valid_until
            && self.payment_term_id == other.payment_term_id
            && self.notes == other.notes
            && self.lines == other.lines
    }

    /// The quote sent by `by`. A quote that was sent can be sent again, with a new link.
    pub fn sent(&self, by: Actor) -> Result<Self, DomainError> {
        if !matches!(self.status, QuoteStatus::Draft | QuoteStatus::Sent) {
            return Err(DomainError::QuoteIsLocked);
        }
        Ok(Self {
            status: QuoteStatus::Sent,
            sent_by: Some(by),
            ..self.clone()
        })
    }

    /// The quote that became a sale. A quote that was declined, or already converted, cannot.
    pub fn converted(&self) -> Result<Self, DomainError> {
        if matches!(self.status, QuoteStatus::Declined | QuoteStatus::Converted) {
            return Err(DomainError::QuoteIsLocked);
        }
        Ok(Self {
            status: QuoteStatus::Converted,
            ..self.clone()
        })
    }

    /// The customer's answer to a quote that was sent.
    pub fn answered(&self, approved: bool) -> Result<Self, DomainError> {
        if self.status != QuoteStatus::Sent {
            return Err(DomainError::QuoteIsNotSent);
        }
        Ok(Self {
            status: if approved {
                QuoteStatus::Approved
            } else {
                QuoteStatus::Declined
            },
            ..self.clone()
        })
    }

    pub fn id(&self) -> QuoteId {
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

    pub fn issued_on(&self) -> CalendarDate {
        self.issued_on
    }

    pub fn valid_until(&self) -> CalendarDate {
        self.valid_until
    }

    pub fn payment_term_id(&self) -> Option<PaymentTermId> {
        self.payment_term_id
    }

    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn status(&self) -> QuoteStatus {
        self.status
    }

    pub fn sent_by(&self) -> Option<&Actor> {
        self.sent_by.as_ref()
    }

    /// The sum of the lines, exact.
    pub fn total(&self) -> Decimal {
        lines_total(&self.lines)
    }

    /// Whether the quote's validity has run out on `day`: its last day is still good.
    pub fn is_expired_on(&self, day: CalendarDate) -> bool {
        self.valid_until < day
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProductId, UserId};

    fn actor() -> Actor {
        Actor::User {
            id: UserId::generate(),
            name: "Ana".into(),
        }
    }

    fn draft() -> QuoteDraft {
        QuoteDraft {
            customer_id: PartyId::generate(),
            issued_on: "2026-11-06".into(),
            valid_until: "2026-11-20".into(),
            payment_term_id: None,
            notes: Some("  Entrega em 5 dias  ".into()),
            lines: vec![LineDraft {
                product_id: ProductId::generate(),
                description: "Consultoria".into(),
                quantity: "2".into(),
                unit_price: "100".into(),
                discount: "10".into(),
            }],
        }
    }

    fn quote() -> Quote {
        Quote::new(QuoteId::generate(), CompanyId::generate(), draft())
            .unwrap()
            .with_number(7)
    }

    #[test]
    fn a_new_quote_is_a_draft_worth_its_lines() {
        let quote = quote();
        assert_eq!(quote.status(), QuoteStatus::Draft);
        assert_eq!(quote.notes(), Some("Entrega em 5 dias"));
        assert_eq!(quote.total().to_string(), "190");
        assert!(!quote.is_expired_on(CalendarDate::parse("2026-11-20").unwrap()));
        assert!(quote.is_expired_on(CalendarDate::parse("2026-11-21").unwrap()));
    }

    #[test]
    fn the_validity_cannot_end_before_the_quote_is_issued() {
        let mut bad = draft();
        bad.valid_until = "2026-11-05".into();
        assert_eq!(
            Quote::new(QuoteId::generate(), CompanyId::generate(), bad).unwrap_err(),
            DomainError::QuoteValidityBeforeIssue
        );
        let mut bad = draft();
        bad.issued_on = "2026-02-30".into();
        assert_eq!(
            Quote::new(QuoteId::generate(), CompanyId::generate(), bad).unwrap_err(),
            DomainError::InvalidDate("issued_on")
        );
    }

    #[test]
    fn a_sent_quote_is_answered_once_and_a_changed_one_is_a_draft_again() {
        let sent = quote().sent(actor()).unwrap();
        assert_eq!(sent.status(), QuoteStatus::Sent);

        // The same content changes nothing; another one puts it back to a draft.
        let mut same = draft();
        same.customer_id = sent.customer_id();
        same.lines[0].product_id = sent.lines()[0].product_id();
        assert_eq!(sent.replaced_with(same.clone()).unwrap(), sent);
        same.valid_until = "2026-12-01".into();
        let changed = sent.replaced_with(same).unwrap();
        assert_eq!(changed.status(), QuoteStatus::Draft);
        assert_eq!(changed.number(), 7);

        let approved = sent.answered(true).unwrap();
        assert_eq!(approved.status(), QuoteStatus::Approved);
        assert_eq!(
            approved.answered(false).unwrap_err(),
            DomainError::QuoteIsNotSent
        );
        assert_eq!(
            quote().answered(true).unwrap_err(),
            DomainError::QuoteIsNotSent
        );
        assert_eq!(
            approved.replaced_with(draft()).unwrap_err(),
            DomainError::QuoteIsLocked
        );
        assert_eq!(
            approved.sent(actor()).unwrap_err(),
            DomainError::QuoteIsLocked
        );
        assert_eq!(
            sent.answered(false).unwrap().status(),
            QuoteStatus::Declined
        );
    }
}
