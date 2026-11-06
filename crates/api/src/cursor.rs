use conta_verde_application::{DateCursor, NameCursor, NumberCursor};
use conta_verde_domain::CalendarDate;
use uuid::Uuid;

use crate::error::ApiError;

// Clients treat the cursor as opaque. The id comes first because it has a fixed shape,
// so a name containing the separator cannot confuse parsing.
pub(crate) fn encode_cursor(cursor: &NameCursor) -> String {
    format!("{}|{}", cursor.id, cursor.name)
}

pub(crate) fn decode_cursor(raw: &str) -> Result<NameCursor, ApiError> {
    let (id, name) = raw.split_once('|').ok_or(ApiError::InvalidCursor)?;
    let id = Uuid::parse_str(id).map_err(|_| ApiError::InvalidCursor)?;
    // No stored name holds a NUL, so a cursor that does was made up, and Postgres would fail on it.
    if name.contains('\0') {
        return Err(ApiError::InvalidCursor);
    }
    Ok(NameCursor {
        name: name.to_owned(),
        id,
    })
}

// The same, for lists ordered by a day: entries by competence date, transfers by the day they occurred.
pub(crate) fn encode_date_cursor(cursor: &DateCursor) -> String {
    format!("{}|{}", cursor.id, cursor.date)
}

pub(crate) fn decode_date_cursor(raw: &str) -> Result<DateCursor, ApiError> {
    let (id, date) = raw.split_once('|').ok_or(ApiError::InvalidCursor)?;
    Ok(DateCursor {
        id: Uuid::parse_str(id).map_err(|_| ApiError::InvalidCursor)?,
        date: CalendarDate::parse(date).ok_or(ApiError::InvalidCursor)?,
    })
}

// The same, for lists of numbered documents.
pub(crate) fn encode_number_cursor(cursor: &NumberCursor) -> String {
    format!("{}|{}", cursor.id, cursor.number)
}

pub(crate) fn decode_number_cursor(raw: &str) -> Result<NumberCursor, ApiError> {
    let (id, number) = raw.split_once('|').ok_or(ApiError::InvalidCursor)?;
    Ok(NumberCursor {
        id: Uuid::parse_str(id).map_err(|_| ApiError::InvalidCursor)?,
        number: number.parse().map_err(|_| ApiError::InvalidCursor)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_survives_separators_in_names() {
        let original = NameCursor {
            name: "Ana | Filhos".into(),
            id: Uuid::new_v4(),
        };
        let decoded = decode_cursor(&encode_cursor(&original)).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn a_date_cursor_survives_a_round_trip_and_rejects_garbage() {
        let original = DateCursor {
            date: CalendarDate::parse("2026-12-10").unwrap(),
            id: Uuid::new_v4(),
        };
        assert_eq!(
            decode_date_cursor(&encode_date_cursor(&original)).unwrap(),
            original
        );
        assert!(decode_date_cursor("nonsense").is_err());
        assert!(decode_date_cursor(&format!("{}|2026-02-30", Uuid::new_v4())).is_err());
        assert!(decode_date_cursor("not-a-uuid|2026-12-10").is_err());
    }

    #[test]
    fn a_number_cursor_survives_a_round_trip_and_rejects_garbage() {
        let original = NumberCursor {
            number: 42,
            id: Uuid::new_v4(),
        };
        assert_eq!(
            decode_number_cursor(&encode_number_cursor(&original)).unwrap(),
            original
        );
        assert!(decode_number_cursor("nonsense").is_err());
        assert!(decode_number_cursor(&format!("{}|x", Uuid::new_v4())).is_err());
    }

    #[test]
    fn garbage_cursors_are_rejected() {
        assert!(decode_cursor("nonsense").is_err());
        assert!(decode_cursor("not-a-uuid|Ana").is_err());
        let id = Uuid::new_v4();
        assert!(decode_cursor(&format!("{id}|An\0a")).is_err());
    }
}
