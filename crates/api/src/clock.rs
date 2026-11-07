use std::time::{SystemTime, UNIX_EPOCH};

use conta_verde_domain::CalendarDate;

/// Today by the server's clock, in UTC. It is read only to generate recurring entries two months ahead
/// ([ADR 0033]), where a zone's few hours never matter; everywhere else the client says which day it is
/// ([ADR 0030]).
pub fn today() -> CalendarDate {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    CalendarDate::from_unix_days(i64::try_from(seconds / 86_400).unwrap_or(0))
        .unwrap_or(CalendarDate::EARLIEST)
}

/// The day a customer's answer is judged against, and a contract's cycles are billed up to: yesterday by the
/// server's UTC clock. UTC changes day at 21:00
/// in Brazil, and a customer approving on the last day of a quote's validity in the evening must still be
/// in time; the cost is that one east of Greenwich may answer a few hours late ([ADR 0038]).
pub fn customer_today() -> CalendarDate {
    let day = today();
    day.pred().unwrap_or(day)
}

/// The moment now, in Unix milliseconds. It is read to judge the limits on e-mailed documents ([ADR 0064]), which are
/// counted over minutes and a day: a caller passes it in, so a test moves it.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}
