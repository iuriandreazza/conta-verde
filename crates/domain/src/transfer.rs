use rust_decimal::Decimal;

use crate::name::optional_text;
use crate::{
    Amount, BankAccountId, CalendarDate, CompanyId, DomainError, SignedAmount, TransferId,
};

const MAX_NOTE_CHARS: usize = 200;

/// Unvalidated input for moving money between two accounts.
#[derive(Debug, Clone)]
pub struct TransferDraft {
    pub from: BankAccountId,
    pub to: BankAccountId,
    pub amount: String,
    pub occurred_on: String,
    pub note: Option<String>,
}

/// Money moved from one bank account of the company to another, on a day ([ADR 0031]). It is not an
/// entry: no category, no party, nothing in the income statement. Never edited: reversed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    id: TransferId,
    company_id: CompanyId,
    from: BankAccountId,
    to: BankAccountId,
    amount: Amount,
    occurred_on: CalendarDate,
    note: Option<String>,
    reversed: bool,
}

impl Transfer {
    /// A new transfer, not reversed. Whether the accounts are of the company and active is the use
    /// case's to check.
    pub fn new(
        id: TransferId,
        company_id: CompanyId,
        draft: TransferDraft,
    ) -> Result<Self, DomainError> {
        if draft.from == draft.to {
            return Err(DomainError::TransferToTheSameAccount);
        }
        let amount = Amount::in_cents(draft.amount.trim())
            .filter(|amount| amount.as_decimal() > Decimal::ZERO)
            .ok_or(DomainError::InvalidTransferAmount)?;
        let occurred_on = CalendarDate::parse(&draft.occurred_on)
            .ok_or(DomainError::InvalidDate("occurred_on"))?;
        // One line, like an entry's description: it is read in lists.
        let note = optional_text(
            draft.note.as_deref(),
            MAX_NOTE_CHARS,
            DomainError::InvalidNote,
        )?;
        if note.as_deref().is_some_and(|text| {
            text.chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        }) {
            return Err(DomainError::InvalidNote);
        }
        Ok(Self {
            id,
            company_id,
            from: draft.from,
            to: draft.to,
            amount,
            occurred_on,
            note,
            reversed: false,
        })
    }

    /// The same transfer, reversed or not: what storage hands back, and what a reversal makes.
    pub fn with_reversed(mut self, reversed: bool) -> Self {
        self.reversed = reversed;
        self
    }

    pub fn id(&self) -> TransferId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn from(&self) -> BankAccountId {
        self.from
    }

    pub fn to(&self) -> BankAccountId {
        self.to
    }

    pub fn amount(&self) -> Amount {
        self.amount
    }

    pub fn occurred_on(&self) -> CalendarDate {
        self.occurred_on
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn is_reversed(&self) -> bool {
        self.reversed
    }
}

/// What one account held before the product was used: its balance at the start of a day ([ADR 0031]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountOpening {
    account_id: BankAccountId,
    company_id: CompanyId,
    balance: SignedAmount,
    on: CalendarDate,
}

impl AccountOpening {
    pub fn new(
        company_id: CompanyId,
        account_id: BankAccountId,
        balance: &str,
        on: &str,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            account_id,
            company_id,
            balance: SignedAmount::in_cents(balance.trim())
                .ok_or(DomainError::InvalidOpeningBalance)?,
            on: CalendarDate::parse(on).ok_or(DomainError::InvalidDate("on"))?,
        })
    }

    pub fn account_id(&self) -> BankAccountId {
        self.account_id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn balance(&self) -> SignedAmount {
        self.balance
    }

    /// The day the balance is of, at its start: what happened from this day on counts after it.
    pub fn on(&self) -> CalendarDate {
        self.on
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(amount: &str) -> TransferDraft {
        TransferDraft {
            from: BankAccountId::generate(),
            to: BankAccountId::generate(),
            amount: amount.into(),
            occurred_on: "2026-12-10".into(),
            note: None,
        }
    }

    fn build(draft: TransferDraft) -> Result<Transfer, DomainError> {
        Transfer::new(TransferId::generate(), CompanyId::generate(), draft)
    }

    #[test]
    fn moves_an_amount_above_zero_in_cents_between_two_different_accounts() {
        let transfer = build(draft(" 150.50 ")).unwrap();
        assert_eq!(transfer.amount().to_string(), "150.5");
        assert_eq!(transfer.occurred_on().to_string(), "2026-12-10");
        assert!(!transfer.is_reversed());
        for amount in ["0", "0.00", "-1", "1.005", "abc", "", "1,5"] {
            assert_eq!(
                build(draft(amount)).unwrap_err(),
                DomainError::InvalidTransferAmount,
                "{amount:?}"
            );
        }
        let account = BankAccountId::generate();
        assert_eq!(
            build(TransferDraft {
                from: account,
                to: account,
                ..draft("1")
            })
            .unwrap_err(),
            DomainError::TransferToTheSameAccount
        );
    }

    #[test]
    fn names_the_date_that_is_wrong() {
        let bad = TransferDraft {
            occurred_on: "10/12/2026".into(),
            ..draft("1")
        };
        assert_eq!(
            build(bad).unwrap_err(),
            DomainError::InvalidDate("occurred_on")
        );
    }

    #[test]
    fn the_note_is_optional_and_one_short_line() {
        let blank = TransferDraft {
            note: Some("   ".into()),
            ..draft("1")
        };
        assert_eq!(build(blank).unwrap().note(), None);
        let good = TransferDraft {
            note: Some("  Fatura do cartão  ".into()),
            ..draft("1")
        };
        assert_eq!(build(good).unwrap().note(), Some("Fatura do cartão"));
        for note in ["a\nb", "a\tb", "a\0b", "a\u{2028}b", &"x".repeat(201)] {
            let bad = TransferDraft {
                note: Some(note.into()),
                ..draft("1")
            };
            assert_eq!(
                build(bad).unwrap_err(),
                DomainError::InvalidNote,
                "{note:?}"
            );
        }
    }

    #[test]
    fn an_opening_balance_may_be_negative_and_is_in_cents() {
        let company = CompanyId::generate();
        let account = BankAccountId::generate();
        let opening = |balance: &str| AccountOpening::new(company, account, balance, "2026-01-01");
        assert_eq!(
            opening(" 1500.50 ").unwrap().balance().to_string(),
            "1500.5"
        );
        assert_eq!(opening("-320.10").unwrap().balance().to_string(), "-320.1");
        assert_eq!(opening("0").unwrap().balance().to_string(), "0");
        assert_eq!(opening("-0").unwrap().balance().to_string(), "0");
        for bad in [
            "",
            "abc",
            "1.005",
            "--1",
            "+1",
            "- 1",
            "10000000000000",
            "1,5",
        ] {
            assert_eq!(
                opening(bad).unwrap_err(),
                DomainError::InvalidOpeningBalance,
                "{bad:?}"
            );
        }
        assert_eq!(
            AccountOpening::new(company, account, "1", "2026-13-01").unwrap_err(),
            DomainError::InvalidDate("on")
        );
    }
}
