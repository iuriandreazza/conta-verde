use conta_verde_domain::{AuditEntry, Audited};

use crate::RepositoryError;

/// Adapters call this before a write: an entry that is not about the record being written (another
/// company, another id, another kind) would put a row into the wrong trail, which is a caller's
/// bug, so it is refused instead of stored.
pub fn ensure_audit_describes(
    audit: &AuditEntry,
    record: &impl Audited,
) -> Result<(), RepositoryError> {
    if audit.describes(record) {
        Ok(())
    } else {
        Err(RepositoryError::Storage(
            "audit entry does not describe the record being written".to_owned(),
        ))
    }
}
