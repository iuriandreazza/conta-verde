use crate::{CalendarDate, CompanyId, DomainError};

/// The longest reason a reopening may give.
const MAX_REASON: usize = 200;

/// A company's closed period ([ADR 0067], [ADR 0069]): everything dated on or before `locked_through` is closed, so a
/// write that makes, changes or undoes what the books hold on such a day is refused. The date is the last day of a
/// month, because the lock closes whole months. There is one per company, replaced as a whole; a company that never
/// closed anything has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeriodLock {
    company_id: CompanyId,
    locked_through: Option<CalendarDate>,
}

impl PeriodLock {
    /// What a company has before it closes a month: nothing is locked.
    pub fn open(company_id: CompanyId) -> Self {
        Self {
            company_id,
            locked_through: None,
        }
    }

    /// A lock that closes through `through`, which must be the last day of a month; `None` is no lock.
    pub fn through(
        company_id: CompanyId,
        through: Option<CalendarDate>,
    ) -> Result<Self, DomainError> {
        if through.is_some_and(|day| day != day.last_of_month()) {
            return Err(DomainError::InvalidPeriodLock);
        }
        Ok(Self {
            company_id,
            locked_through: through,
        })
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn locked_through(&self) -> Option<CalendarDate> {
        self.locked_through
    }

    /// Whether the day is closed.
    pub fn locks(&self, day: CalendarDate) -> bool {
        self.locked_through.is_some_and(|through| day <= through)
    }

    /// Whether replacing this lock with `after` opens days that were closed.
    pub fn is_reopened_by(&self, after: &Self) -> bool {
        match (self.locked_through, after.locked_through) {
            (Some(_), None) => true,
            (Some(before), Some(after)) => after < before,
            (None, _) => false,
        }
    }
}

/// The reason a month is reopened, as it is kept: one line of 1 to 200 characters, trimmed.
pub fn reopen_reason(raw: &str) -> Result<String, DomainError> {
    let reason = raw.trim();
    let length = reason.chars().count();
    if length == 0 || length > MAX_REASON || reason.chars().any(char::is_control) {
        return Err(DomainError::InvalidReopenReason);
    }
    Ok(reason.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(raw: &str) -> CalendarDate {
        CalendarDate::parse(raw).unwrap()
    }

    #[test]
    fn closes_whole_months_only() {
        let company = CompanyId::generate();
        assert!(PeriodLock::through(company, Some(day("2026-10-31"))).is_ok());
        assert!(PeriodLock::through(company, Some(day("2028-02-29"))).is_ok());
        assert_eq!(
            PeriodLock::through(company, Some(day("2026-10-30"))),
            Err(DomainError::InvalidPeriodLock)
        );
        assert_eq!(
            PeriodLock::through(company, None).unwrap(),
            PeriodLock::open(company)
        );
    }

    #[test]
    fn a_day_is_closed_on_or_before_the_lock() {
        let lock = PeriodLock::through(CompanyId::generate(), Some(day("2026-10-31"))).unwrap();
        assert!(lock.locks(day("2026-10-31")));
        assert!(lock.locks(day("2026-01-01")));
        assert!(!lock.locks(day("2026-11-01")));
        assert!(!PeriodLock::open(CompanyId::generate()).locks(day("1900-01-01")));
    }

    #[test]
    fn moving_the_date_back_or_removing_it_is_a_reopening() {
        let company = CompanyId::generate();
        let october = PeriodLock::through(company, Some(day("2026-10-31"))).unwrap();
        let september = PeriodLock::through(company, Some(day("2026-09-30"))).unwrap();
        let november = PeriodLock::through(company, Some(day("2026-11-30"))).unwrap();
        assert!(october.is_reopened_by(&september));
        assert!(october.is_reopened_by(&PeriodLock::open(company)));
        assert!(!october.is_reopened_by(&november));
        assert!(!october.is_reopened_by(&october));
        assert!(!PeriodLock::open(company).is_reopened_by(&september));
    }

    #[test]
    fn a_reason_is_one_trimmed_line() {
        assert_eq!(reopen_reason("  late invoice ").unwrap(), "late invoice");
        assert!(reopen_reason("   ").is_err());
        assert!(reopen_reason("two\nlines").is_err());
        assert!(reopen_reason(&"x".repeat(201)).is_err());
        assert!(reopen_reason(&"x".repeat(200)).is_ok());
    }
}
