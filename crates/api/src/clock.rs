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
