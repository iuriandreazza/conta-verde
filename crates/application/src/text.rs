use std::time::Duration;

use conta_verde_domain::CalendarDate;

/// A name typed by a person, made safe for one line of a subject or a log: a line break in it would add headers to a
/// message or forge lines in a log.
pub(crate) fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// `09/12/2026`, as people read a day in Brazil.
pub(crate) fn br_date(day: CalendarDate) -> String {
    format!("{:02}/{:02}/{}", day.day(), day.month(), day.year())
}

/// `1 dia`, `7 dias`: how long a link lasts, said in whole days.
pub(crate) fn in_days(lifetime: Duration) -> String {
    let days = lifetime.as_secs() / 86_400;
    format!("{days} {}", if days == 1 { "dia" } else { "dias" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_is_written_the_brazilian_way() {
        assert_eq!(
            br_date(CalendarDate::parse("2026-12-09").unwrap()),
            "09/12/2026"
        );
    }

    #[test]
    fn a_lifetime_is_said_in_days_with_the_right_plural() {
        assert_eq!(in_days(Duration::from_secs(86_400)), "1 dia");
        assert_eq!(in_days(Duration::from_secs(7 * 86_400)), "7 dias");
    }
}
