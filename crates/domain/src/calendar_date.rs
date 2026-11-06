use std::fmt;

/// The first and last year a date may have: a bound on what is entered by hand, not a business rule.
const FIRST_YEAR: u16 = 1900;
const LAST_YEAR: u16 = 2200;

/// A day on the calendar: a year, a month and a day, with no time and no zone. A due date does not
/// move when the server's zone does, so it is not a timestamp. Written `YYYY-MM-DD`.
///
/// The domain keeps its own small type for it so that it needs no date library; the order of the
/// fields is the order of the days, so the derived ordering is the chronological one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDate {
    year: u16,
    month: u8,
    day: u8,
}

impl CalendarDate {
    /// Strict `YYYY-MM-DD`: four, two and two digits and two hyphens, a year from 1900 to 2200 and a
    /// day that exists in its month. Nothing is trimmed or guessed: `2026-2-1` is refused.
    pub fn parse(raw: &str) -> Option<Self> {
        let bytes = raw.as_bytes();
        let shaped = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, byte)| i == 4 || i == 7 || byte.is_ascii_digit());
        if !shaped {
            return None;
        }
        let year: u16 = raw[0..4].parse().ok()?;
        let month: u8 = raw[5..7].parse().ok()?;
        let day: u8 = raw[8..10].parse().ok()?;
        let valid = (FIRST_YEAR..=LAST_YEAR).contains(&year)
            && (1..=12).contains(&month)
            && (1..=days_in_month(year, month)).contains(&day);
        valid.then_some(Self { year, month, day })
    }

    pub fn year(&self) -> u16 {
        self.year
    }

    pub fn month(&self) -> u8 {
        self.month
    }

    pub fn day(&self) -> u8 {
        self.day
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_the_same_text() {
        let date = CalendarDate::parse("2026-11-05").unwrap();
        assert_eq!((date.year(), date.month(), date.day()), (2026, 11, 5));
        assert_eq!(date.to_string(), "2026-11-05");
    }

    #[test]
    fn a_day_must_exist_in_its_month() {
        assert!(CalendarDate::parse("2026-02-28").is_some());
        assert!(CalendarDate::parse("2026-02-29").is_none());
        assert!(CalendarDate::parse("2028-02-29").is_some());
        assert!(
            CalendarDate::parse("1900-02-29").is_none(),
            "1900 was not a leap year"
        );
        assert!(CalendarDate::parse("2000-02-29").is_some());
        assert!(CalendarDate::parse("2026-04-31").is_none());
        assert!(CalendarDate::parse("2026-12-31").is_some());
        assert!(CalendarDate::parse("2026-13-01").is_none());
        assert!(CalendarDate::parse("2026-00-10").is_none());
        assert!(CalendarDate::parse("2026-05-00").is_none());
    }

    #[test]
    fn refuses_anything_that_is_not_exactly_the_iso_form() {
        for raw in [
            "",
            "2026-2-1",
            "26-02-01",
            "2026/02/01",
            " 2026-02-01",
            "2026-02-01 ",
            "2026-02-011",
            "2026-02-0a",
            "+026-02-01",
            "２０２６-02-01",
            "2026-02-01T00:00",
        ] {
            assert!(CalendarDate::parse(raw).is_none(), "{raw:?}");
        }
    }

    #[test]
    fn years_are_bounded() {
        assert!(CalendarDate::parse("1899-12-31").is_none());
        assert!(CalendarDate::parse("1900-01-01").is_some());
        assert!(CalendarDate::parse("2200-12-31").is_some());
        assert!(CalendarDate::parse("2201-01-01").is_none());
    }

    #[test]
    fn dates_order_chronologically() {
        let day = |raw| CalendarDate::parse(raw).unwrap();
        assert!(day("2026-11-30") < day("2026-12-01"));
        assert!(day("2026-12-01") < day("2027-01-01"));
        assert!(day("2026-02-10") > day("2026-02-09"));
    }
}
