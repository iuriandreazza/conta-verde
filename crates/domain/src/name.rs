use crate::DomainError;

// A btree entry holds about 2.7 KB, and the name is indexed (`lower(name)`) with the company and
// the id: a name past it fails in the database, so it is refused here, as a 422, instead.
const MAX_NAME_CHARS: usize = 200;

/// The text trimmed, or `None` when nothing is left: an optional field left blank is not set.
pub(crate) fn non_blank(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then_some(trimmed)
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
