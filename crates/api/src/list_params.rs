//! The parameters every list endpoint parses the same way (ADR 0018).

use conta_verde_application::{MAX_SEARCH_CHARS, NameSort};

use crate::error::ApiError;

/// The search text, if it is not longer than a person would type.
pub(crate) fn search(text: Option<&str>) -> Result<Option<&str>, ApiError> {
    match text {
        Some(text) if text.chars().count() > MAX_SEARCH_CHARS => {
            Err(ApiError::InvalidQuery("invalid q: at most 100 characters"))
        }
        other => Ok(other),
    }
}

/// `name`, the default, or `-name` for descending.
pub(crate) fn sort(value: Option<&str>) -> Result<NameSort, ApiError> {
    match value {
        None | Some("name") => Ok(NameSort::NameAsc),
        Some("-name") => Ok(NameSort::NameDesc),
        Some(_) => Err(ApiError::InvalidQuery(
            "invalid sort: expected name or -name",
        )),
    }
}

/// `true` or `false`; `None` when the parameter is absent.
pub(crate) fn active(value: Option<&str>) -> Result<Option<bool>, ApiError> {
    match value {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(_) => Err(ApiError::InvalidQuery(
            "invalid active: expected true or false",
        )),
    }
}
