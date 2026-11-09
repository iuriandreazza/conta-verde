use crate::CalendarDate;

/// The IBGE code of a state, which an access key begins with. `None` for what is not one of the 27 units.
pub fn state_code(state: &str) -> Option<u8> {
    Some(match state {
        "RO" => 11,
        "AC" => 12,
        "AM" => 13,
        "RR" => 14,
        "PA" => 15,
        "AP" => 16,
        "TO" => 17,
        "MA" => 21,
        "PI" => 22,
        "CE" => 23,
        "RN" => 24,
        "PB" => 25,
        "PE" => 26,
        "AL" => 27,
        "SE" => 28,
        "BA" => 29,
        "MG" => 31,
        "ES" => 32,
        "RJ" => 33,
        "SP" => 35,
        "PR" => 41,
        "SC" => 42,
        "RS" => 43,
        "MS" => 50,
        "MT" => 51,
        "GO" => 52,
        "DF" => 53,
        _ => return None,
    })
}

/// The standard-time offset from UTC of a state, in minutes: -03:00 for most, -04:00 for Amazonas, Rondônia, Roraima,
/// Mato Grosso and Mato Grosso do Sul, -05:00 for Acre. Daylight saving is not applied: Brazil has none since 2019,
/// and a document's time must follow the issuer's clock (verify with an accountant if it returns, [ADR 0075]).
/// Fernando de Noronha (-02:00) is a district of Pernambuco, not a state, and is not told apart.
pub fn state_utc_offset_minutes(state: &str) -> Option<i16> {
    state_code(state)?;
    Some(match state {
        "AC" => -300,
        "AM" | "RO" | "RR" | "MT" | "MS" => -240,
        _ => -180,
    })
}

/// A moment as the SEFAZ writes it: `2026-11-09T14:30:00-03:00`, the clock of the issuer's state. `None` for a
/// state that does not exist or a moment outside the years a date may have.
pub fn fiscal_timestamp(unix_seconds: i64, state: &str) -> Option<String> {
    let offset = state_utc_offset_minutes(state)?;
    let local = unix_seconds.checked_add(i64::from(offset) * 60)?;
    let day = CalendarDate::from_unix_days(local.div_euclid(86_400))?;
    let seconds = local.rem_euclid(86_400);
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.abs();
    Some(format!(
        "{day}T{:02}:{:02}:{:02}{sign}{:02}:{:02}",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60,
        minutes / 60,
        minutes % 60
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_a_code_and_an_offset() {
        for state in [
            "AC", "AL", "AM", "AP", "BA", "CE", "DF", "ES", "GO", "MA", "MG", "MS", "MT", "PA",
            "PB", "PE", "PI", "PR", "RJ", "RN", "RO", "RR", "RS", "SC", "SE", "SP", "TO",
        ] {
            assert!(state_code(state).is_some(), "{state}");
            assert!(state_utc_offset_minutes(state).is_some(), "{state}");
        }
        assert_eq!(state_code("SP"), Some(35));
        assert_eq!(state_code("XX"), None);
        assert_eq!(state_code("sp"), None);
    }

    #[test]
    fn the_offsets_are_the_four_the_country_has() {
        assert_eq!(state_utc_offset_minutes("SP"), Some(-180));
        assert_eq!(state_utc_offset_minutes("AM"), Some(-240));
        assert_eq!(state_utc_offset_minutes("MS"), Some(-240));
        assert_eq!(state_utc_offset_minutes("AC"), Some(-300));
        assert_eq!(state_utc_offset_minutes("XX"), None);
    }

    #[test]
    fn the_timestamp_is_the_clock_of_the_state() {
        // 2026-11-09 17:30:00 UTC.
        let moment = CalendarDate::parse("2026-11-09").unwrap().to_unix_days() * 86_400
            + 17 * 3600
            + 30 * 60;
        assert_eq!(
            fiscal_timestamp(moment, "SP").as_deref(),
            Some("2026-11-09T14:30:00-03:00")
        );
        assert_eq!(
            fiscal_timestamp(moment, "AM").as_deref(),
            Some("2026-11-09T13:30:00-04:00")
        );
        assert_eq!(
            fiscal_timestamp(moment, "AC").as_deref(),
            Some("2026-11-09T12:30:00-05:00")
        );
        assert_eq!(fiscal_timestamp(moment, "XX"), None);
    }

    #[test]
    fn the_day_turns_with_the_state_not_with_utc() {
        // 2026-11-10 01:30 UTC is still the 9th in São Paulo and in Rio Branco.
        let moment =
            CalendarDate::parse("2026-11-10").unwrap().to_unix_days() * 86_400 + 1800 + 3600;
        assert_eq!(
            fiscal_timestamp(moment, "SP").as_deref(),
            Some("2026-11-09T22:30:00-03:00")
        );
        assert_eq!(
            fiscal_timestamp(moment, "AC").as_deref(),
            Some("2026-11-09T20:30:00-05:00")
        );
    }
}
