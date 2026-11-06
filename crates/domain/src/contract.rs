use rust_decimal::Decimal;

use crate::financial_entry::clean_description;
use crate::line::{lines_total, validated_lines};
use crate::name::optional_text;
use crate::{
    Actor, CalendarDate, CategoryId, CompanyId, ContractId, CostCenterId, DomainError, Line,
    LineDraft, PartyId, PaymentTermId, SaleDraft, SignedAmount, round_to_cents,
};

const MAX_NOTES_CHARS: usize = 2000;

/// How often a contract bills: every so many months, counted from the first day with the day clipped to the
/// end of a short month (the 31st gives the 28th and then the 31st again, never sliding for good).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingCycle {
    Monthly,
    Bimonthly,
    Quarterly,
    Semiannual,
    Yearly,
}

impl BillingCycle {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::Bimonthly => "bimonthly",
            Self::Quarterly => "quarterly",
            Self::Semiannual => "semiannual",
            Self::Yearly => "yearly",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "monthly" => Some(Self::Monthly),
            "bimonthly" => Some(Self::Bimonthly),
            "quarterly" => Some(Self::Quarterly),
            "semiannual" => Some(Self::Semiannual),
            "yearly" => Some(Self::Yearly),
            _ => None,
        }
    }

    pub fn months(self) -> u32 {
        match self {
            Self::Monthly => 1,
            Self::Bimonthly => 2,
            Self::Quarterly => 3,
            Self::Semiannual => 6,
            Self::Yearly => 12,
        }
    }
}

/// Where a contract is in its life ([ADR 0040]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractStatus {
    Active,
    /// Bills nothing until it is brought back, and what fell due meanwhile is not made up.
    Paused,
    /// Ended for good: it bills nothing more and does not change.
    Terminated,
}

impl ContractStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Terminated => "terminated",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "terminated" => Some(Self::Terminated),
            _ => None,
        }
    }
}

/// Unvalidated input for registering or changing a contract.
#[derive(Debug, Clone)]
pub struct ContractDraft {
    pub name: String,
    pub customer_id: PartyId,
    pub seller_id: Option<PartyId>,
    pub payment_term_id: Option<PaymentTermId>,
    pub category_id: CategoryId,
    pub cost_center_id: Option<CostCenterId>,
    pub notes: Option<String>,
    pub lines: Vec<LineDraft>,
    pub cycle: BillingCycle,
    pub starts_on: String,
    pub ends_on: Option<String>,
    /// `false` pauses it.
    pub active: bool,
}

/// Which cycle of which contract a sale bills ([ADR 0040]): a contract has at most one sale per cycle day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractCycle {
    pub contract_id: ContractId,
    pub cycle_on: CalendarDate,
}

/// An agreement to sell the same lines to a customer every cycle: each cycle becomes a completed sale and its
/// receivable ([ADR 0040]). The total of a cycle is the sum of the lines and is never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contract {
    id: ContractId,
    company_id: CompanyId,
    name: String,
    customer_id: PartyId,
    seller_id: Option<PartyId>,
    payment_term_id: Option<PaymentTermId>,
    category_id: CategoryId,
    cost_center_id: Option<CostCenterId>,
    notes: Option<String>,
    lines: Vec<Line>,
    cycle: BillingCycle,
    starts_on: CalendarDate,
    ends_on: Option<CalendarDate>,
    status: ContractStatus,
    /// The day through which the cycles were billed; bookkeeping, not an editable field.
    billed_through: Option<CalendarDate>,
    /// Whoever last saved it: the sales it makes are made in their name.
    saved_by: Actor,
}

impl Contract {
    pub fn new(
        id: ContractId,
        company_id: CompanyId,
        draft: ContractDraft,
        saved_by: Actor,
    ) -> Result<Self, DomainError> {
        let name = clean_description(&draft.name)?;
        let starts_on =
            CalendarDate::parse(&draft.starts_on).ok_or(DomainError::InvalidDate("starts_on"))?;
        let ends_on = draft
            .ends_on
            .as_deref()
            .map(|raw| CalendarDate::parse(raw).ok_or(DomainError::InvalidDate("ends_on")))
            .transpose()?;
        if ends_on.is_some_and(|end| end < starts_on) {
            return Err(DomainError::RecurrenceEndsBeforeItStarts);
        }
        let notes = optional_text(
            draft.notes.as_deref(),
            MAX_NOTES_CHARS,
            DomainError::InvalidQuoteNotes,
        )?;
        Ok(Self {
            id,
            company_id,
            name,
            customer_id: draft.customer_id,
            seller_id: draft.seller_id,
            payment_term_id: draft.payment_term_id,
            category_id: draft.category_id,
            cost_center_id: draft.cost_center_id,
            notes,
            lines: validated_lines(&draft.lines)?,
            cycle: draft.cycle,
            starts_on,
            ends_on,
            status: if draft.active {
                ContractStatus::Active
            } else {
                ContractStatus::Paused
            },
            billed_through: None,
            saved_by,
        })
    }

    /// A contract as it was stored: its status and the day it was billed through.
    pub fn restored(
        mut self,
        status: ContractStatus,
        billed_through: Option<CalendarDate>,
    ) -> Self {
        self.status = status;
        self.billed_through = billed_through;
        self
    }

    /// The same contract with every editable field replaced by the draft's; a terminated contract is fixed.
    /// It keeps the day it was billed through even when the schedule changes: a new schedule has other days,
    /// and starting over would bill the history again.
    pub fn replaced_with(
        &self,
        draft: ContractDraft,
        saved_by: Actor,
    ) -> Result<Self, DomainError> {
        if self.status == ContractStatus::Terminated {
            return Err(DomainError::ContractIsTerminated);
        }
        let mut replaced = Self::new(self.id, self.company_id, draft, saved_by)?;
        replaced.billed_through = self.billed_through;
        Ok(replaced)
    }

    /// The same contract that goes on from today after a pause: what fell due while it was paused is skipped.
    pub fn resumed_from(&self, yesterday: CalendarDate) -> Self {
        Self {
            billed_through: self
                .billed_through
                .filter(|done| *done >= yesterday)
                .or(Some(yesterday)),
            ..self.clone()
        }
    }

    /// The contract ended on `on`: nothing is billed after it, and it never changes again.
    pub fn terminated_on(&self, on: &str, saved_by: Actor) -> Result<Self, DomainError> {
        if self.status == ContractStatus::Terminated {
            return Err(DomainError::ContractIsTerminated);
        }
        let on = CalendarDate::parse(on).ok_or(DomainError::InvalidDate("on"))?;
        if on < self.starts_on {
            return Err(DomainError::RecurrenceEndsBeforeItStarts);
        }
        Ok(Self {
            status: ContractStatus::Terminated,
            ends_on: Some(self.ends_on.map_or(on, |end| end.min(on))),
            saved_by,
            ..self.clone()
        })
    }

    /// The prices of the lines raised (or lowered, below 0) by a percentage, each rounded to the cent
    /// ([ADR 0037]): from the next cycle that is billed on. Between -99.99 and 1000, at most two decimals.
    pub fn adjusted_by(&self, percent: &str, saved_by: Actor) -> Result<Self, DomainError> {
        if self.status == ContractStatus::Terminated {
            return Err(DomainError::ContractIsTerminated);
        }
        let percent = SignedAmount::in_cents(percent.trim())
            .map(|value| value.as_decimal())
            .filter(|value| *value > Decimal::from(-100) && *value <= Decimal::from(1000))
            .ok_or(DomainError::InvalidAdjustment)?;
        let factor = (Decimal::ONE_HUNDRED + percent) / Decimal::ONE_HUNDRED;
        let lines = self
            .lines
            .iter()
            .map(|line| LineDraft {
                unit_price: round_to_cents(line.unit_price().as_decimal() * factor).to_string(),
                ..line.to_draft()
            })
            .collect::<Vec<_>>();
        Ok(Self {
            lines: validated_lines(&lines)?,
            saved_by,
            ..self.clone()
        })
    }

    /// The days the contract bills that are after `through` (all of them when `None`), up to and including
    /// `until` and never past its end: at most `limit`, oldest first.
    pub fn cycles_after(
        &self,
        through: Option<CalendarDate>,
        until: CalendarDate,
        limit: usize,
    ) -> Vec<CalendarDate> {
        let last = self.ends_on.map_or(until, |end| end.min(until));
        let mut days = Vec::new();
        for n in 0u32.. {
            let Some(day) = n
                .checked_mul(self.cycle.months())
                .and_then(|months| self.starts_on.plus_months(months))
                .filter(|day| *day <= last)
            else {
                break;
            };
            if through.is_none_or(|done| day > done) {
                days.push(day);
                if days.len() == limit {
                    break;
                }
            }
        }
        days
    }

    /// The sale one cycle makes: the contract's lines, term and category, sold on the cycle day.
    pub fn sale_draft(&self, cycle_on: CalendarDate) -> SaleDraft {
        SaleDraft {
            customer_id: self.customer_id,
            seller_id: self.seller_id,
            sold_on: cycle_on.to_string(),
            payment_term_id: self.payment_term_id,
            category_id: self.category_id,
            cost_center_id: self.cost_center_id,
            notes: self.notes.clone(),
            lines: self.lines.iter().map(Line::to_draft).collect(),
        }
    }

    pub fn id(&self) -> ContractId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn customer_id(&self) -> PartyId {
        self.customer_id
    }

    pub fn seller_id(&self) -> Option<PartyId> {
        self.seller_id
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

    pub fn cycle(&self) -> BillingCycle {
        self.cycle
    }

    pub fn starts_on(&self) -> CalendarDate {
        self.starts_on
    }

    pub fn ends_on(&self) -> Option<CalendarDate> {
        self.ends_on
    }

    pub fn status(&self) -> ContractStatus {
        self.status
    }

    pub fn billed_through(&self) -> Option<CalendarDate> {
        self.billed_through
    }

    pub fn saved_by(&self) -> &Actor {
        &self.saved_by
    }

    /// What one cycle bills: the sum of the lines.
    pub fn total(&self) -> Decimal {
        lines_total(&self.lines)
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

    fn draft(cycle: BillingCycle, starts_on: &str, ends_on: Option<&str>) -> ContractDraft {
        ContractDraft {
            name: "  Manutenção mensal  ".into(),
            customer_id: PartyId::generate(),
            seller_id: None,
            payment_term_id: None,
            category_id: CategoryId::generate(),
            cost_center_id: None,
            notes: None,
            lines: vec![LineDraft {
                product_id: ProductId::generate(),
                description: "Suporte".into(),
                quantity: "2".into(),
                unit_price: "100.10".into(),
                discount: "0.20".into(),
            }],
            cycle,
            starts_on: starts_on.into(),
            ends_on: ends_on.map(Into::into),
            active: true,
        }
    }

    fn contract(cycle: BillingCycle, starts_on: &str, ends_on: Option<&str>) -> Contract {
        Contract::new(
            ContractId::generate(),
            CompanyId::generate(),
            draft(cycle, starts_on, ends_on),
            actor(),
        )
        .unwrap()
    }

    fn days(contract: &Contract, through: Option<&str>, until: &str, limit: usize) -> Vec<String> {
        contract
            .cycles_after(
                through.and_then(CalendarDate::parse),
                CalendarDate::parse(until).unwrap(),
                limit,
            )
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn a_contract_bills_what_its_lines_are_worth() {
        let contract = contract(BillingCycle::Monthly, "2026-11-05", None);
        assert_eq!(contract.name(), "Manutenção mensal");
        assert_eq!(contract.total().to_string(), "200");
        assert_eq!(contract.status(), ContractStatus::Active);
        let sale = contract.sale_draft(CalendarDate::parse("2026-12-05").unwrap());
        assert_eq!(sale.sold_on, "2026-12-05");
        assert_eq!(sale.lines.len(), 1);
    }

    #[test]
    fn the_cycles_are_counted_from_the_first_day_with_the_day_clipped() {
        let monthly = contract(BillingCycle::Monthly, "2026-01-31", None);
        assert_eq!(
            days(&monthly, None, "2026-04-30", 10),
            ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]
        );
        let quarterly = contract(BillingCycle::Quarterly, "2026-11-30", Some("2027-08-30"));
        // Never past its last day, and from where it was billed through.
        assert_eq!(
            days(&quarterly, None, "2030-01-01", 10),
            ["2026-11-30", "2027-02-28", "2027-05-30", "2027-08-30"]
        );
        assert_eq!(
            days(&quarterly, Some("2027-02-28"), "2030-01-01", 10),
            ["2027-05-30", "2027-08-30"]
        );
        assert_eq!(
            days(&quarterly, None, "2030-01-01", 2),
            ["2026-11-30", "2027-02-28"]
        );
        // Nothing before it starts.
        assert!(days(&quarterly, None, "2026-11-29", 10).is_empty());
        let yearly = contract(BillingCycle::Yearly, "2024-02-29", None);
        assert_eq!(
            days(&yearly, None, "2026-03-01", 10),
            ["2024-02-29", "2025-02-28", "2026-02-28"]
        );
    }

    #[test]
    fn a_price_adjustment_rounds_each_price_to_the_cent() {
        let contract = contract(BillingCycle::Monthly, "2026-11-05", None);
        let raised = contract.adjusted_by("5.5", actor()).unwrap();
        // 100.10 x 1.055 = 105.6055 -> 105.61, and the line is 2 x 105.61 less 0.20.
        assert_eq!(raised.lines()[0].unit_price().to_string(), "105.61");
        assert_eq!(raised.total().to_string(), "211.02");
        let lowered = contract.adjusted_by("-10", actor()).unwrap();
        assert_eq!(lowered.lines()[0].unit_price().to_string(), "90.09");
        for bad in ["-100", "1000.01", "abc", "", "1.234"] {
            assert_eq!(
                contract.adjusted_by(bad, actor()).unwrap_err(),
                DomainError::InvalidAdjustment,
                "{bad:?}"
            );
        }
        // A cut that leaves the discount bigger than the line is refused.
        let mut big_discount = draft(BillingCycle::Monthly, "2026-11-05", None);
        big_discount.lines[0].discount = "200".into();
        let heavy = Contract::new(
            ContractId::generate(),
            CompanyId::generate(),
            big_discount,
            actor(),
        )
        .unwrap();
        assert_eq!(
            heavy.adjusted_by("-50", actor()).unwrap_err(),
            DomainError::DiscountExceedsLine
        );
    }

    #[test]
    fn a_terminated_contract_is_fixed_and_a_pause_skips_what_fell_due() {
        let contract = contract(BillingCycle::Monthly, "2026-11-05", Some("2027-12-05"));
        let ended = contract.terminated_on("2027-03-01", actor()).unwrap();
        assert_eq!(ended.status(), ContractStatus::Terminated);
        assert_eq!(ended.ends_on().unwrap().to_string(), "2027-03-01");
        assert_eq!(
            ended.terminated_on("2027-02-01", actor()).unwrap_err(),
            DomainError::ContractIsTerminated
        );
        assert_eq!(
            ended
                .replaced_with(draft(BillingCycle::Monthly, "2026-11-05", None), actor())
                .unwrap_err(),
            DomainError::ContractIsTerminated
        );
        assert_eq!(
            ended.adjusted_by("5", actor()).unwrap_err(),
            DomainError::ContractIsTerminated
        );
        assert_eq!(
            contract.terminated_on("2026-11-04", actor()).unwrap_err(),
            DomainError::RecurrenceEndsBeforeItStarts
        );
        // Terminating after its natural end cannot lengthen it.
        let late = contract.terminated_on("2030-01-01", actor()).unwrap();
        assert_eq!(late.ends_on().unwrap().to_string(), "2027-12-05");

        let yesterday = CalendarDate::parse("2027-01-09").unwrap();
        assert_eq!(
            contract.resumed_from(yesterday).billed_through(),
            Some(yesterday)
        );
        let ahead = contract
            .clone()
            .restored(ContractStatus::Active, CalendarDate::parse("2027-02-01"));
        assert_eq!(
            ahead.resumed_from(yesterday).billed_through(),
            CalendarDate::parse("2027-02-01")
        );
    }
}
