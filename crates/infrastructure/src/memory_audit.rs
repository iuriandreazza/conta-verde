use std::time::{SystemTime, UNIX_EPOCH};

use conta_verde_domain::{AuditEntity, AuditEntry, AuditRecord, CompanyId};
use uuid::Uuid;

/// The audit trail of one in-memory repository. It lives in the same state, behind the same lock,
/// as the rows it describes, so recording last is what makes a write and its entry atomic.
#[derive(Default)]
pub(crate) struct AuditLog {
    records: Vec<AuditRecord>,
    next_sequence: i64,
}

impl AuditLog {
    pub(crate) fn record(&mut self, audit: &AuditEntry) {
        self.next_sequence += 1;
        let at_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
            });
        self.records.push(AuditRecord {
            sequence: self.next_sequence,
            at_millis,
            entry: audit.clone(),
        });
    }

    /// The trail of one record, oldest first.
    pub(crate) fn history(
        &self,
        company_id: CompanyId,
        entity: AuditEntity,
        entity_id: Uuid,
    ) -> Vec<AuditRecord> {
        self.records
            .iter()
            .filter(|record| {
                record.entry.company_id == company_id
                    && record.entry.entity == entity
                    && record.entry.entity_id == entity_id
            })
            .cloned()
            .collect()
    }
}
