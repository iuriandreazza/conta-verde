use rust_decimal::Decimal;

use crate::{
    Amount, BankAccountId, CalendarDate, ChargeId, CompanyId, DomainError, EntryId,
    PaymentMethodId, SettlementId,
};

/// A settlement pays an installment of the entry it names; there is at most 120 of them.
const MAX_INSTALLMENT: u16 = 120;

/// Unvalidated input for settling an installment: the amounts as decimal text, the three that
/// adjust the payment left out when there is none.
#[derive(Debug, Clone)]
pub struct SettlementDraft {
    pub installment: u16,
    pub settled_on: String,
    pub applied: String,
    pub interest: Option<String>,
    pub fine: Option<String>,
    pub discount: Option<String>,
    pub bank_account_id: BankAccountId,
    pub payment_method_id: Option<PaymentMethodId>,
}

/// A payment of an installment of an entry, in part or in full ([ADR 0030]): how much of the
/// installment it settles, what was added (interest, fine) or taken off (discount) when the money
/// moved, the day, and the bank account. It is never edited: it is reversed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    id: SettlementId,
    company_id: CompanyId,
    entry_id: EntryId,
    installment: u16,
    settled_on: CalendarDate,
    applied: Amount,
    interest: Amount,
    fine: Amount,
    discount: Amount,
    bank_account_id: BankAccountId,
    payment_method_id: Option<PaymentMethodId>,
    /// The charge whose payment this settles, when a webhook made it: a charge settles at most once.
    charge_id: Option<ChargeId>,
    reversed: bool,
}

impl Settlement {
    /// A new settlement, not reversed. Whether the entry has that installment, whether it has the
    /// balance, and whether the account and method are usable are the use case's to check.
    pub fn new(
        id: SettlementId,
        company_id: CompanyId,
        entry_id: EntryId,
        draft: SettlementDraft,
    ) -> Result<Self, DomainError> {
        if draft.installment == 0 || draft.installment > MAX_INSTALLMENT {
            return Err(DomainError::InvalidInstallmentNumber);
        }
        let settled_on =
            CalendarDate::parse(&draft.settled_on).ok_or(DomainError::InvalidDate("settled_on"))?;
        let applied = Amount::in_cents(draft.applied.trim())
            .filter(|amount| amount.as_decimal() > Decimal::ZERO)
            .ok_or(DomainError::InvalidSettlementAmount("applied"))?;
        let adjustment = |raw: &Option<String>, field: &'static str| match raw.as_deref() {
            None => Ok(Amount::ZERO),
            Some(text) => {
                Amount::in_cents(text.trim()).ok_or(DomainError::InvalidSettlementAmount(field))
            }
        };
        let settlement = Self {
            id,
            company_id,
            entry_id,
            installment: draft.installment,
            settled_on,
            applied,
            interest: adjustment(&draft.interest, "interest")?,
            fine: adjustment(&draft.fine, "fine")?,
            discount: adjustment(&draft.discount, "discount")?,
            bank_account_id: draft.bank_account_id,
            payment_method_id: draft.payment_method_id,
            charge_id: None,
            reversed: false,
        };
        if settlement.cash() <= Decimal::ZERO {
            return Err(DomainError::SettlementMovesNothing);
        }
        Ok(settlement)
    }

    /// The same settlement, said to be the payment of a charge ([ADR 0046]).
    pub fn with_charge(mut self, charge_id: Option<ChargeId>) -> Self {
        self.charge_id = charge_id;
        self
    }

    pub fn charge_id(&self) -> Option<ChargeId> {
        self.charge_id
    }

    /// The same settlement, reversed or not: what storage hands back, and what a reversal makes.
    pub fn with_reversed(mut self, reversed: bool) -> Self {
        self.reversed = reversed;
        self
    }

    pub fn id(&self) -> SettlementId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn entry_id(&self) -> EntryId {
        self.entry_id
    }

    pub fn installment(&self) -> u16 {
        self.installment
    }

    pub fn settled_on(&self) -> CalendarDate {
        self.settled_on
    }

    pub fn applied(&self) -> Amount {
        self.applied
    }

    pub fn interest(&self) -> Amount {
        self.interest
    }

    pub fn fine(&self) -> Amount {
        self.fine
    }

    pub fn discount(&self) -> Amount {
        self.discount
    }

    pub fn bank_account_id(&self) -> BankAccountId {
        self.bank_account_id
    }

    pub fn payment_method_id(&self) -> Option<PaymentMethodId> {
        self.payment_method_id
    }

    pub fn is_reversed(&self) -> bool {
        self.reversed
    }

    /// What moved in the bank account: what it settles, with interest and fine, less the discount.
    /// More than 0, exact, written without trailing zeros. Not stored: it is added up.
    pub fn cash(&self) -> Decimal {
        (self.applied.as_decimal() + self.interest.as_decimal() + self.fine.as_decimal()
            - self.discount.as_decimal())
        .normalize()
    }
}

/// How much of installment `number` the settlements that are not reversed have settled.
pub fn settled_of(settlements: &[Settlement], number: u16) -> Decimal {
    settlements
        .iter()
        .filter(|settlement| settlement.installment == number && !settlement.reversed)
        .map(|settlement| settlement.applied.as_decimal())
        .sum::<Decimal>()
        .normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(applied: &str) -> SettlementDraft {
        SettlementDraft {
            installment: 1,
            settled_on: "2026-12-10".into(),
            applied: applied.into(),
            interest: None,
            fine: None,
            discount: None,
            bank_account_id: BankAccountId::generate(),
            payment_method_id: None,
        }
    }

    fn build(draft: SettlementDraft) -> Result<Settlement, DomainError> {
        Settlement::new(
            SettlementId::generate(),
            CompanyId::generate(),
            EntryId::generate(),
            draft,
        )
    }

    #[test]
    fn the_cash_is_what_is_applied_with_interest_and_fine_less_the_discount() {
        let plain = build(draft(" 100 ")).unwrap();
        assert_eq!(plain.cash().to_string(), "100");
        let late = build(SettlementDraft {
            interest: Some("5".into()),
            fine: Some("2.5".into()),
            ..draft("100")
        })
        .unwrap();
        assert_eq!(late.cash().to_string(), "107.5");
        assert_eq!(
            late.applied().to_string(),
            "100",
            "they do not change what is settled"
        );
        let early = build(SettlementDraft {
            discount: Some("10.05".into()),
            ..draft("100")
        })
        .unwrap();
        assert_eq!(early.cash().to_string(), "89.95");
    }

    #[test]
    fn what_is_applied_is_more_than_zero_and_in_cents() {
        for applied in ["0", "0.00", "-1", "1.005", "abc", "", "1,5"] {
            assert_eq!(
                build(draft(applied)).unwrap_err(),
                DomainError::InvalidSettlementAmount("applied"),
                "{applied:?}"
            );
        }
        assert!(build(draft("0.01")).is_ok());
    }

    #[test]
    fn the_adjustments_may_be_zero_but_not_negative_or_finer_than_cents() {
        for (field, bad) in [("interest", "-1"), ("fine", "0.001"), ("discount", "x")] {
            let mut input = draft("100");
            match field {
                "interest" => input.interest = Some(bad.into()),
                "fine" => input.fine = Some(bad.into()),
                _ => input.discount = Some(bad.into()),
            }
            assert_eq!(
                build(input).unwrap_err(),
                DomainError::InvalidSettlementAmount(field),
                "{field}"
            );
        }
        let zeros = SettlementDraft {
            interest: Some("0".into()),
            fine: Some("0.00".into()),
            discount: Some("0".into()),
            ..draft("100")
        };
        assert_eq!(build(zeros).unwrap().cash().to_string(), "100");
    }

    #[test]
    fn a_payment_that_moves_nothing_is_not_one() {
        let free = SettlementDraft {
            discount: Some("100".into()),
            ..draft("100")
        };
        assert_eq!(
            build(free).unwrap_err(),
            DomainError::SettlementMovesNothing
        );
        let more = SettlementDraft {
            discount: Some("100.01".into()),
            ..draft("100")
        };
        assert_eq!(
            build(more).unwrap_err(),
            DomainError::SettlementMovesNothing
        );
        let cent = SettlementDraft {
            discount: Some("99.99".into()),
            ..draft("100")
        };
        assert_eq!(build(cent).unwrap().cash().to_string(), "0.01");
    }

    #[test]
    fn names_the_date_and_the_installment_that_are_wrong() {
        let mut bad_date = draft("1");
        bad_date.settled_on = "10/12/2026".into();
        assert_eq!(
            build(bad_date).unwrap_err(),
            DomainError::InvalidDate("settled_on")
        );
        for number in [0, 121] {
            let mut bad = draft("1");
            bad.installment = number;
            assert_eq!(
                build(bad).unwrap_err(),
                DomainError::InvalidInstallmentNumber
            );
        }
    }

    #[test]
    fn reversed_settlements_do_not_count_toward_what_is_settled() {
        let first = build(draft("40")).unwrap();
        let second = build(draft("25.5")).unwrap();
        let other_installment = build(SettlementDraft {
            installment: 2,
            ..draft("10")
        })
        .unwrap();
        let undone = build(draft("30")).unwrap().with_reversed(true);
        let all = [first, second, other_installment, undone];
        assert_eq!(settled_of(&all, 1).to_string(), "65.5");
        assert_eq!(settled_of(&all, 2).to_string(), "10");
        assert_eq!(settled_of(&all, 3).to_string(), "0");
    }
}
