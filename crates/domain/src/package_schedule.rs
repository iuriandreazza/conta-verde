use crate::{CalendarDate, CompanyId, DomainError};

/// The first day of the month the package may go out on, and the last: the day is one that every month has.
const FIRST_DAY: u8 = 1;
const LAST_DAY: u8 = 28;
/// What a company has before it chooses.
const DEFAULT_DAY: u8 = 5;

/// When a company's monthly package is announced to its accountants ([ADR 0067], [ADR 0072]): a switch and a day of
/// the month, 1 to 28. One per company, replaced as a whole; a company that never chose has it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageSchedule {
    company_id: CompanyId,
    enabled: bool,
    day: u8,
}

impl PackageSchedule {
    /// What a company has before it chooses: off, on the 5th.
    pub fn off(company_id: CompanyId) -> Self {
        Self {
            company_id,
            enabled: false,
            day: DEFAULT_DAY,
        }
    }

    /// A schedule as people write one: the day must be one every month has.
    pub fn new(company_id: CompanyId, enabled: bool, day: u8) -> Result<Self, DomainError> {
        if !(FIRST_DAY..=LAST_DAY).contains(&day) {
            return Err(DomainError::InvalidPackageDay);
        }
        Ok(Self {
            company_id,
            enabled,
            day,
        })
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn day(&self) -> u8 {
        self.day
    }

    /// Whether the package of the previous month is due on `today`: the switch is on and the day has come. It stays
    /// due for the rest of the month, so a server that was down on the day still sends it when it is back.
    pub fn is_due_on(&self, today: CalendarDate) -> bool {
        self.enabled && today.day() >= self.day
    }
}

/// The month a package announced on `today` is for: the previous one, as its first day.
pub fn month_to_package(today: CalendarDate) -> CalendarDate {
    // The day before the first of this month is in the previous one; it exists for every day the calendar has but the
    // very first, which no company is asked about.
    today
        .first_of_month()
        .pred()
        .map_or_else(|| today.first_of_month(), |last| last.first_of_month())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(raw: &str) -> CalendarDate {
        CalendarDate::parse(raw).unwrap()
    }

    #[test]
    fn a_company_that_never_chose_has_it_off_on_the_fifth() {
        let schedule = PackageSchedule::off(CompanyId::generate());
        assert!(!schedule.is_enabled());
        assert_eq!(schedule.day(), 5);
        assert!(!schedule.is_due_on(day("2026-11-30")));
    }

    #[test]
    fn the_day_is_one_that_every_month_has() {
        let company = CompanyId::generate();
        for ok in [1, 15, 28] {
            assert_eq!(PackageSchedule::new(company, true, ok).unwrap().day(), ok);
        }
        for bad in [0, 29, 31, 255] {
            assert_eq!(
                PackageSchedule::new(company, true, bad),
                Err(DomainError::InvalidPackageDay)
            );
        }
    }

    #[test]
    fn it_is_due_from_its_day_to_the_end_of_the_month_only_while_on() {
        let on = PackageSchedule::new(CompanyId::generate(), true, 10).unwrap();
        assert!(!on.is_due_on(day("2026-11-09")));
        assert!(on.is_due_on(day("2026-11-10")));
        assert!(on.is_due_on(day("2026-11-30")));
        let off = PackageSchedule::new(CompanyId::generate(), false, 10).unwrap();
        assert!(!off.is_due_on(day("2026-11-20")));
    }

    #[test]
    fn the_package_is_for_the_previous_month_across_the_turn_of_the_year() {
        assert_eq!(month_to_package(day("2026-11-10")), day("2026-10-01"));
        assert_eq!(month_to_package(day("2026-03-31")), day("2026-02-01"));
        assert_eq!(month_to_package(day("2027-01-05")), day("2026-12-01"));
    }
}
