use crate::DomainError;

// A btree entry holds about 2.7 KB, and the name is indexed (`lower(name)`) with the company and
// the id: a name past it fails in the database, so it is refused here, as a 422, instead.
const MAX_NAME_CHARS: usize = 200;

/// The text trimmed, or `None` when nothing is left: an optional field left blank is not set.
pub(crate) fn non_blank(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// Trimmed, not empty, at most `max` characters and without a NUL byte, which Postgres refuses to
/// store. Nothing stricter: street and person names have digits, hyphens, apostrophes and accents.
/// `error` is what a refusal says, so each caller names its own field.
pub(crate) fn required_text(
    raw: &str,
    max: usize,
    error: DomainError,
) -> Result<String, DomainError> {
    let text = raw.trim();
    if text.is_empty() || text.contains('\0') || text.chars().count() > max {
        return Err(error);
    }
    Ok(text.to_owned())
}

/// Like [`optional_text`], for a note that is one line: a control character, which a line break is, is refused
/// too, because the database refuses it.
pub(crate) fn optional_line(
    raw: Option<&str>,
    max: usize,
    error: DomainError,
) -> Result<Option<String>, DomainError> {
    let text = optional_text(raw, max, error.clone())?;
    if text
        .as_deref()
        .is_some_and(|text| text.chars().any(char::is_control))
    {
        return Err(error);
    }
    Ok(text)
}

/// Like [`required_text`], but a missing or blank text is not set instead of refused.
pub(crate) fn optional_text(
    raw: Option<&str>,
    max: usize,
    error: DomainError,
) -> Result<Option<String>, DomainError> {
    raw.and_then(non_blank)
        .map(|text| required_text(text, max, error))
        .transpose()
}

/// The name of a register entry: trimmed, not empty, at most 200 characters, and without a NUL
/// byte, which Postgres refuses to store. Nothing stricter: a name already stored must stay readable.
pub(crate) fn clean_name(raw: &str) -> Result<String, DomainError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(DomainError::EmptyName);
    }
    if name.contains('\0') || name.chars().count() > MAX_NAME_CHARS {
        return Err(DomainError::InvalidName);
    }
    Ok(name.to_owned())
}
