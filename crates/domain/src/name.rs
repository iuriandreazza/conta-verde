use crate::DomainError;

/// The name of a register entry: trimmed, not empty, and free of control characters. A NUL byte in
/// particular is something Postgres refuses to store, so it must not get past the domain.
pub(crate) fn clean_name(raw: &str) -> Result<String, DomainError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(DomainError::EmptyName);
    }
    if name.chars().any(char::is_control) {
        return Err(DomainError::InvalidName);
    }
    Ok(name.to_owned())
}
