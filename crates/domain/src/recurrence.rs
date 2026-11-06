use crate::financial_entry::clean_description;
use crate::{
    Actor, Amount, CalendarDate, CategoryId, CompanyId, CostCenterId, DomainError,
    EntryInstallmentDraft, EntryKind, FinancialEntryDraft, PartyId, RecurrenceId,
};
use rust_decimal::Decimal;

/// How often a recurrence comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    Weekly,
    Monthly,
    Yearly,
}

impl Frequency {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            "yearly" => Some(Self::Yearly),
            _ => None,
        }
    }
}

/// Unvalidated input for registering or changing a recurrence.
#[derive(Debug, Clone)]
pub struct RecurrenceDraft {
    pub kind: EntryKind,
    pub description: String,
    pub party_id: Option<PartyId>,
    pub category_id: CategoryId,
    pub cost_center_id: Option<CostCenterId>,
    pub amount: String,
    pub frequency: Frequency,
    pub starts_on: String,
    pub ends_on: Option<String>,
    pub active: bool,
}

/// An entry that comes back every week, month or year ([ADR 0033]): the occurrences are `starts_on` plus
/// 0, 1, 2… periods, and each one becomes an entry with one installment.
///
/// Whether the category, the cost center and the party are usable is the use case's to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurrence {
    id: RecurrenceId,
    company_id: CompanyId,
    kind: EntryKind,
    description: String,
    party_id: Option<PartyId>,
    category_id: CategoryId,
    cost_center_id: Option<CostCenterId>,
    amount: Amount,
    frequency: Frequency,
    starts_on: CalendarDate,
    ends_on: Option<CalendarDate>,
    active: bool,
    /// The day through which the occurrences were generated; not an editable field.
    generated_through: Option<CalendarDate>,
    /// Whoever last saved it: the entries it generates are made in their name.
    saved_by: Actor,
}

impl Recurrence {
    pub fn new(
        id: RecurrenceId,
        company_id: CompanyId,
        draft: RecurrenceDraft,
        saved_by: Actor,
    ) -> Result<Self, DomainError> {
        let description = clean_description(&draft.description)?;
        let amount = Amount::in_cents(draft.amount.trim())
            .filter(|amount| amount.as_decimal() > Decimal::ZERO)
            .ok_or(DomainError::InvalidRecurrenceAmount)?;
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
        Ok(Self {
            id,
            company_id,
            kind: draft.kind,
            description,
            party_id: draft.party_id,
            category_id: draft.category_id,
            cost_center_id: draft.cost_center_id,
            amount,
            frequency: draft.frequency,
            starts_on,
            ends_on,
            active: draft.active,
            generated_through: None,
            saved_by,
        })
    }

    /// The same recurrence with every editable field replaced by the draft's. It starts generating over
    /// when the first day or the frequency changes, and keeps where it was otherwise.
    pub fn replaced_with(
        &self,
        draft: RecurrenceDraft,
        saved_by: Actor,
    ) -> Result<Self, DomainError> {
        let mut replaced = Self::new(self.id, self.company_id, draft, saved_by)?;
        if replaced.frequency == self.frequency && replaced.starts_on == self.starts_on {
            replaced.generated_through = self.generated_through;
        }
        Ok(replaced)
    }

    /// A recurrence read back from storage with the day it was generated through.
    pub fn with_generated_through(mut self, day: Option<CalendarDate>) -> Self {
        self.generated_through = day;
        self
    }

    pub fn id(&self) -> RecurrenceId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn party_id(&self) -> Option<PartyId> {
        self.party_id
    }

    pub fn category_id(&self) -> CategoryId {
        self.category_id
    }

    pub fn cost_center_id(&self) -> Option<CostCenterId> {
        self.cost_center_id
    }

    pub fn amount(&self) -> Amount {
        self.amount
    }

    pub fn frequency(&self) -> Frequency {
        self.frequency
    }

    pub fn starts_on(&self) -> CalendarDate {
        self.starts_on
    }

    pub fn ends_on(&self) -> Option<CalendarDate> {
        self.ends_on
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn generated_through(&self) -> Option<CalendarDate> {
        self.generated_through
    }

    pub fn saved_by(&self) -> &Actor {
        &self.saved_by
    }

    /// The day of occurrence number `n` (from 0), or `None` past the last day a date may have.
    fn occurrence(&self, n: u32) -> Option<CalendarDate> {
        match self.frequency {
            Frequency::Weekly => {
                (0..n.checked_mul(7)?).try_fold(self.starts_on, |day, _| day.succ())
            }
            Frequency::Monthly => self.starts_on.plus_months(n),
            Frequency::Yearly => self.starts_on.plus_months(n.checked_mul(12)?),
        }
    }

    /// The occurrence days after `through` (all of them when `None`) up to and including `until`, and
    /// never past `ends_on`: at most `limit`, oldest first.
    pub fn occurrences_after(
        &self,
        through: Option<CalendarDate>,
        until: CalendarDate,
        limit: usize,
    ) -> Vec<CalendarDate> {
        let last = self.ends_on.map_or(until, |end| end.min(until));
        let mut days = Vec::new();
        for n in 0.. {
            let Some(day) = self.occurrence(n).filter(|day| *day <= last) else {
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

    /// The entry an occurrence becomes: one installment that falls due on the day.
    pub fn entry_draft(&self, on: CalendarDate) -> FinancialEntryDraft {
        FinancialEntryDraft {
            kind: self.kind,
            description: self.description.clone(),
            party_id: self.party_id,
            category_id: self.category_id,
            cost_center_id: self.cost_center_id,
            competence_date: on.to_string(),
            installments: vec![EntryInstallmentDraft {
                due_date: on.to_string(),
                amount: self.amount.to_string(),
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserId;

    fn actor() -> Actor {
        Actor::User {
            id: UserId::generate(),
            name: "Ana".into(),
        }
    }

    fn draft(frequency: Frequency, starts_on: &str, ends_on: Option<&str>) -> RecurrenceDraft {
        RecurrenceDraft {
            kind: EntryKind::Payable,
            description: "  Aluguel  ".into(),
            party_id: None,
            category_id: CategoryId::generate(),
            cost_center_id: None,
            amount: "2500.50".into(),
            frequency,
            starts_on: starts_on.into(),
            ends_on: ends_on.map(Into::into),
            active: true,
        }
    }

    fn recurrence(frequency: Frequency, starts_on: &str, ends_on: Option<&str>) -> Recurrence {
        Recurrence::new(
            RecurrenceId::generate(),
            CompanyId::generate(),
            draft(frequency, starts_on, ends_on),
            actor(),
        )
        .unwrap()
    }

    fn days(days: Vec<CalendarDate>) -> Vec<String> {
        days.iter().map(ToString::to_string).collect()
    }

    fn day(raw: &str) -> CalendarDate {
        CalendarDate::parse(raw).unwrap()
    }

    #[test]
    fn cleans_the_description_and_keeps_the_amount_in_cents() {
        let recurrence = recurrence(Frequency::Monthly, "2026-11-05", None);
        assert_eq!(recurrence.description(), "Aluguel");
        assert_eq!(recurrence.amount().to_string(), "2500.5");
        assert!(recurrence.is_active());
        assert_eq!(recurrence.generated_through(), None);
    }

    #[test]
    fn refuses_what_is_not_a_recurrence() {
        let new = |draft: RecurrenceDraft| {
            Recurrence::new(
                RecurrenceId::generate(),
                CompanyId::generate(),
                draft,
                actor(),
            )
            .unwrap_err()
        };
        let base = || draft(Frequency::Monthly, "2026-11-05", None);
        for amount in ["0", "-1", "1.005", "abc", ""] {
            assert_eq!(
                new(RecurrenceDraft {
                    amount: amount.into(),
                    ..base()
                }),
                DomainError::InvalidRecurrenceAmount,
                "{amount:?}"
            );
        }
        assert_eq!(
            new(RecurrenceDraft {
                description: " ".into(),
                ..base()
            }),
            DomainError::InvalidDescription
        );
        assert_eq!(
            new(RecurrenceDraft {
                starts_on: "2026-02-30".into(),
                ..base()
            }),
            DomainError::InvalidDate("starts_on")
        );
        assert_eq!(
            new(RecurrenceDraft {
                ends_on: Some("tomorrow".into()),
                ..base()
            }),
            DomainError::InvalidDate("ends_on")
        );
        assert_eq!(
            new(RecurrenceDraft {
                ends_on: Some("2026-11-04".into()),
                ..base()
            }),
            DomainError::RecurrenceEndsBeforeItStarts
        );
    }

    #[test]
    fn a_month_is_counted_from_the_first_day_and_the_day_is_clipped_not_slid() {
        let rent = recurrence(Frequency::Monthly, "2026-01-31", None);
        assert_eq!(
            days(rent.occurrences_after(None, day("2026-05-31"), 10)),
            [
                "2026-01-31",
                "2026-02-28",
                "2026-03-31",
                "2026-04-30",
                "2026-05-31"
            ]
        );
        let leap = recurrence(Frequency::Monthly, "2028-01-30", None);
        assert_eq!(
            days(leap.occurrences_after(None, day("2028-03-31"), 10)),
            ["2028-01-30", "2028-02-29", "2028-03-30"]
        );
    }

    #[test]
    fn a_week_is_seven_days_and_a_year_is_twelve_months() {
        let weekly = recurrence(Frequency::Weekly, "2026-12-28", None);
        assert_eq!(
            days(weekly.occurrences_after(None, day("2027-01-20"), 10)),
            ["2026-12-28", "2027-01-04", "2027-01-11", "2027-01-18"]
        );
        let yearly = recurrence(Frequency::Yearly, "2028-02-29", None);
        assert_eq!(
            days(yearly.occurrences_after(None, day("2031-12-31"), 10)),
            ["2028-02-29", "2029-02-28", "2030-02-28", "2031-02-28"]
        );
    }

    #[test]
    fn stops_at_the_end_day_the_horizon_and_the_limit_and_skips_what_was_generated() {
        let ends = recurrence(Frequency::Monthly, "2026-11-05", Some("2027-01-05"));
        assert_eq!(
            days(ends.occurrences_after(None, day("2030-01-01"), 10)),
            ["2026-11-05", "2026-12-05", "2027-01-05"]
        );
        let open = recurrence(Frequency::Monthly, "2026-11-05", None);
        assert_eq!(
            days(open.occurrences_after(None, day("2026-12-31"), 10)).len(),
            2
        );
        assert_eq!(
            days(open.occurrences_after(None, day("2030-01-01"), 3)).len(),
            3
        );
        assert_eq!(
            days(open.occurrences_after(Some(day("2026-12-05")), day("2027-02-28"), 10)),
            ["2027-01-05", "2027-02-05"]
        );
        assert!(
            open.occurrences_after(None, day("2026-11-04"), 10)
                .is_empty()
        );
    }

    #[test]
    fn has_no_occurrence_beyond_the_last_day_a_date_may_have() {
        let late = recurrence(Frequency::Yearly, "2199-06-01", None);
        assert_eq!(
            days(late.occurrences_after(None, day("2200-12-31"), 10)),
            ["2199-06-01", "2200-06-01"]
        );
    }

    #[test]
    fn an_occurrence_becomes_an_entry_with_one_installment_on_its_day() {
        let rent = recurrence(Frequency::Monthly, "2026-11-05", None);
        let draft = rent.entry_draft(day("2026-12-05"));
        assert_eq!(draft.kind, EntryKind::Payable);
        assert_eq!(draft.description, "Aluguel");
        assert_eq!(draft.competence_date, "2026-12-05");
        assert_eq!(draft.installments.len(), 1);
        assert_eq!(draft.installments[0].due_date, "2026-12-05");
        assert_eq!(draft.installments[0].amount, "2500.5");
    }

    #[test]
    fn starts_generating_over_only_when_the_first_day_or_the_frequency_changes() {
        let generated = recurrence(Frequency::Monthly, "2026-11-05", None)
            .with_generated_through(Some(day("2027-01-04")));
        let kept = generated
            .replaced_with(
                RecurrenceDraft {
                    amount: "3000".into(),
                    ..draft(Frequency::Monthly, "2026-11-05", None)
                },
                actor(),
            )
            .unwrap();
        assert_eq!(kept.generated_through(), Some(day("2027-01-04")));
        assert_eq!(kept.id(), generated.id());
        for changed in [
            draft(Frequency::Weekly, "2026-11-05", None),
            draft(Frequency::Monthly, "2026-11-06", None),
        ] {
            assert_eq!(
                generated
                    .replaced_with(changed, actor())
                    .unwrap()
                    .generated_through(),
                None
            );
        }
    }

    #[test]
    fn frequencies_read_and_write_the_same_key() {
        for frequency in [Frequency::Weekly, Frequency::Monthly, Frequency::Yearly] {
            assert_eq!(Frequency::parse(frequency.as_str()), Some(frequency));
        }
        assert_eq!(Frequency::parse("daily"), None);
    }
}
