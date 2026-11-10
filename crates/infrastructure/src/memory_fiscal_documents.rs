use std::sync::{Arc, Mutex};

use conta_verde_application::{FiscalDocumentRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, FiscalDocument, FiscalDocumentId,
    FiscalDocumentKind, FiscalDocumentStatus, FiscalFile, FiscalFileKind, FiscalOrigin,
};

use crate::memory_audit::AuditLog;

/// Keeps fiscal documents, their files' records and their audit trail in process memory, for tests and for running
/// the API without a database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryFiscalDocumentRepository {
    state: Arc<Mutex<DocumentState>>,
}

#[derive(Default)]
struct DocumentState {
    documents: Vec<FiscalDocument>,
    files: Vec<FiscalFile>,
    audit: AuditLog,
}

impl FiscalDocumentRepository for InMemoryFiscalDocumentRepository {
    async fn create(
        &self,
        document: &FiscalDocument,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, document)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let taken = document.status().is_live()
            && state.documents.iter().any(|stored| {
                stored.company_id() == document.company_id()
                    && stored.origin() == document.origin()
                    && stored.kind() == document.kind()
                    && stored.status().is_live()
            });
        if taken {
            return Err(RepositoryError::FiscalDocumentAlreadyLive);
        }
        state.documents.push(document.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn replace(
        &self,
        document: &FiscalDocument,
        expected: FiscalDocumentStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, document)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let others_live = document.status().is_live()
            && state.documents.iter().any(|stored| {
                stored.company_id() == document.company_id()
                    && stored.id() != document.id()
                    && stored.origin() == document.origin()
                    && stored.kind() == document.kind()
                    && stored.status().is_live()
            });
        let stored = state
            .documents
            .iter_mut()
            .find(|stored| {
                stored.company_id() == document.company_id() && stored.id() == document.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != expected {
            return Err(RepositoryError::FiscalDocumentChanged);
        }
        let frozen = matches!(
            stored.status(),
            FiscalDocumentStatus::Authorized | FiscalDocumentStatus::Cancelled
        );
        if frozen && stored.content_differs_from(document) {
            return Err(RepositoryError::Storage(
                "the content of an authorized fiscal document never changes".to_owned(),
            ));
        }
        if others_live {
            return Err(RepositoryError::FiscalDocumentAlreadyLive);
        }
        *stored = document.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Option<FiscalDocument>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .documents
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn find_live(
        &self,
        company_id: CompanyId,
        origin: FiscalOrigin,
        kind: FiscalDocumentKind,
    ) -> Result<Option<FiscalDocument>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .documents
            .iter()
            .find(|stored| {
                stored.company_id() == company_id
                    && stored.origin() == origin
                    && stored.kind() == kind
                    && stored.status().is_live()
            })
            .cloned())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::FiscalDocument, id.as_uuid()))
    }

    async fn save_file(&self, file: &FiscalFile) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        if !state.documents.iter().any(|stored| {
            stored.company_id() == file.company_id() && stored.id() == file.document_id()
        }) {
            return Err(RepositoryError::NotFound);
        }
        let taken = state.files.iter().any(|stored| {
            stored.company_id() == file.company_id()
                && stored.document_id() == file.document_id()
                && stored.kind() == file.kind()
        });
        if taken {
            return Err(RepositoryError::DuplicateFiscalFile);
        }
        state.files.push(file.clone());
        Ok(())
    }

    async fn files_of(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<FiscalFile>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut files: Vec<FiscalFile> = state
            .files
            .iter()
            .filter(|stored| stored.company_id() == company_id && stored.document_id() == id)
            .cloned()
            .collect();
        files.sort_by_key(|file| file.kind().key());
        Ok(files)
    }

    async fn find_file(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
        kind: FiscalFileKind,
    ) -> Result<Option<FiscalFile>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .files
            .iter()
            .find(|stored| {
                stored.company_id() == company_id
                    && stored.document_id() == id
                    && stored.kind() == kind
            })
            .cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;
    use conta_verde_domain::SaleId;

    #[tokio::test]
    async fn keeps_fiscal_documents() {
        contract::fiscal_documents_are_kept(
            &InMemoryFiscalDocumentRepository::default(),
            CompanyId::generate(),
            CompanyId::generate(),
            SaleId::generate(),
            SaleId::generate(),
        )
        .await;
    }
}
