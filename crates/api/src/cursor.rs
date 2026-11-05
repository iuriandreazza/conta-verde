use conta_verde_application::NameCursor;
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
    fn garbage_cursors_are_rejected() {
        assert!(decode_cursor("nonsense").is_err());
        assert!(decode_cursor("not-a-uuid|Ana").is_err());
        let id = Uuid::new_v4();
        assert!(decode_cursor(&format!("{id}|An\0a")).is_err());
    }
}
