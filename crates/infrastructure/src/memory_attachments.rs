use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{
    AttachmentRepository, BlobStore, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    Attachment, AttachmentId, AuditEntity, AuditEntry, AuditRecord, BlobId, CompanyId, EntryId,
};

use crate::memory_audit::AuditLog;

/// Keeps attachment records and their audit trail in process memory, for tests and for running the API
/// without a database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryAttachmentRepository {
    state: Arc<Mutex<AttachmentState>>,
}

#[derive(Default)]
struct AttachmentState {
    /// In the order they were attached.
    attachments: Vec<Attachment>,
    audit: AuditLog,
}

impl AttachmentRepository for InMemoryAttachmentRepository {
    async fn save(
        &self,
        attachment: &Attachment,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, attachment)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.attachments.push(attachment.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn remove(
        &self,
        removed: &Attachment,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, removed)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .attachments
            .iter_mut()
            .find(|stored| {
                stored.company_id() == removed.company_id()
                    && stored.entry_id() == removed.entry_id()
                    && stored.id() == removed.id()
                    && !stored.is_removed()
            })
            .ok_or(RepositoryError::NotFound)?;
        *stored = stored.clone().with_removed(true);
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: AttachmentId,
    ) -> Result<Option<Attachment>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .attachments
            .iter()
            .find(|stored| {
                stored.company_id() == company_id
                    && stored.entry_id() == entry_id
                    && stored.id() == id
            })
            .cloned())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: AttachmentId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Attachment, id.as_uuid()))
    }

    async fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Attachment>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .attachments
            .iter()
            .filter(|stored| {
                stored.company_id() == company_id
                    && stored.entry_id() == entry_id
                    && !stored.is_removed()
            })
            .cloned()
            .collect())
    }

    async fn list_by_entries(
        &self,
        company_id: CompanyId,
        entry_ids: &[EntryId],
    ) -> Result<Vec<Attachment>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<(usize, Attachment)> = state
            .attachments
            .iter()
            .enumerate()
            .filter(|(_, stored)| {
                stored.company_id() == company_id
                    && entry_ids.contains(&stored.entry_id())
                    && !stored.is_removed()
            })
            .map(|(attached, stored)| (attached, stored.clone()))
            .collect();
        // The order they were attached is the order they were stored in.
        found.sort_by_key(|(attached, stored)| (stored.entry_id().as_uuid(), *attached));
        Ok(found.into_iter().map(|(_, stored)| stored).collect())
    }
}

/// Keeps the bytes of attachments in process memory, for tests and for running the API without a disk.
/// Clones share the same files.
#[derive(Default, Clone)]
pub struct InMemoryBlobStore {
    files: Arc<Mutex<HashMap<BlobKey, Vec<u8>>>>,
}

type BlobKey = (CompanyId, BlobId);

impl BlobStore for InMemoryBlobStore {
    async fn put(
        &self,
        company_id: CompanyId,
        id: BlobId,
        bytes: &[u8],
    ) -> Result<(), RepositoryError> {
        self.files
            .lock()
            .expect("blob store lock poisoned")
            .insert((company_id, id), bytes.to_vec());
        Ok(())
    }

    async fn get(
        &self,
        company_id: CompanyId,
        id: BlobId,
    ) -> Result<Option<Vec<u8>>, RepositoryError> {
        Ok(self
            .files
            .lock()
            .expect("blob store lock poisoned")
            .get(&(company_id, id))
            .cloned())
    }

    async fn delete(&self, company_id: CompanyId, id: BlobId) -> Result<(), RepositoryError> {
        self.files
            .lock()
            .expect("blob store lock poisoned")
            .remove(&(company_id, id));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{AttachmentService, FinancialEntryService};

    use super::*;
    use crate::InMemoryFinancialEntryRepository;
    use crate::contract;
    use crate::memory_entries::tests::Registers;

    #[tokio::test]
    async fn keeps_blobs() {
        contract::keeps_blobs(
            &InMemoryBlobStore::default(),
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn attaches_files() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let store = InMemoryAttachmentRepository::default();
        let blobs = InMemoryBlobStore::default();
        let service = AttachmentService::new(store.clone(), blobs.clone(), entry_store.clone());
        let entries = FinancialEntryService::new(entry_store, registers.references());
        contract::attaches_files(
            &service,
            &store,
            &blobs,
            &entries,
            company,
            other_company,
            &seed,
        )
        .await;
    }
}
