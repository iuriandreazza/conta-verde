use conta_verde_domain::{Actor, AuditAction, AuditEntry, Audited, FieldChange};

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

/// The audit entry for a write to `record`, made by `actor`.
pub(crate) fn audit_entry(
    record: &impl Audited,
    actor: &Actor,
    action: AuditAction,
    changes: Vec<FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: record.audit_company(),
        actor: actor.clone(),
        entity: record.audit_entity(),
        entity_id: record.audit_id(),
        action,
        changes,
    }
}
