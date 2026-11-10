use std::sync::{Arc, Mutex};

use conta_verde_application::{FiscalDocumentRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CalendarDate, CompanyId, FiscalDocument,
    FiscalDocumentId, FiscalDocumentKind, FiscalDocumentStatus, FiscalEnvironment, FiscalEvent,
    FiscalEventId, FiscalEventStatus, FiscalEventSubject, FiscalFile, FiscalFileKind, FiscalOrigin,
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
    events: Vec<FiscalEvent>,
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
        if state
            .documents
            .iter()
            .any(|stored| stored.id() == document.id())
        {
            return Err(RepositoryError::Storage(
                "the fiscal document already exists".to_owned(),
            ));
        }
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
        // The index of the migration: an access key names one document of a company.
        let key_taken = document.authorization().is_some_and(|authorization| {
            state.documents.iter().any(|stored| {
                stored.company_id() == document.company_id()
                    && stored.id() != document.id()
                    && stored.environment() == document.environment()
                    && stored
                        .authorization()
                        .is_some_and(|other| other.access_key() == authorization.access_key())
            })
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
        // What the migration's trigger holds: a cancelled or denied document never moves again.
        if matches!(
            stored.status(),
            FiscalDocumentStatus::Cancelled | FiscalDocumentStatus::Denied
        ) && stored.status() != document.status()
        {
            return Err(RepositoryError::Storage(
                "a cancelled or denied fiscal document never moves again".to_owned(),
            ));
        }
        if others_live {
            return Err(RepositoryError::FiscalDocumentAlreadyLive);
        }
        if key_taken {
            return Err(RepositoryError::Storage(
                "an access key names one fiscal document".to_owned(),
            ));
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

    async fn pending(
        &self,
        since_unix: i64,
        limit: usize,
    ) -> Result<Vec<FiscalDocument>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut waiting: Vec<FiscalDocument> = state
            .documents
            .iter()
            .filter(|stored| stored.status().is_pending() && stored.issued_at_unix() >= since_unix)
            .cloned()
            .collect();
        waiting.sort_by_key(|stored| (stored.issued_at_unix(), stored.id().as_uuid()));
        waiting.truncate(limit);
        Ok(waiting)
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

    async fn issued_between(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
        limit: usize,
    ) -> Result<Vec<FiscalDocument>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<FiscalDocument> = state
            .documents
            .iter()
            .filter(|stored| {
                stored.company_id() == company_id
                    && stored.environment() == FiscalEnvironment::Production
                    && matches!(
                        stored.status(),
                        FiscalDocumentStatus::Authorized | FiscalDocumentStatus::Cancelled
                    )
                    && (from..=to).contains(&stored.issued_on())
            })
            .cloned()
            .collect();
        found.sort_by_key(|stored| (stored.issued_at_unix(), stored.id().as_uuid()));
        found.truncate(limit);
        Ok(found)
    }

    async fn record_event(
        &self,
        event: &FiscalEvent,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, event)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if event.status() != FiscalEventStatus::Sent {
            return Err(RepositoryError::Storage(
                "an event is recorded before it is answered".to_owned(),
            ));
        }
        if state.events.iter().any(|stored| stored.id() == event.id()) {
            return Err(RepositoryError::Storage(
                "the fiscal event already exists".to_owned(),
            ));
        }
        if let FiscalEventSubject::Correction {
            document, sequence, ..
        } = event.subject()
        {
            if !state
                .documents
                .iter()
                .any(|stored| stored.company_id() == event.company_id() && stored.id() == *document)
            {
                return Err(RepositoryError::NotFound);
            }
            let taken = state.events.iter().any(|stored| {
                stored.company_id() == event.company_id()
                    && matches!(stored.subject(), FiscalEventSubject::Correction { document: other, sequence: place, .. } if other == document && place == sequence)
            });
            if taken {
                return Err(RepositoryError::FiscalDocumentChanged);
            }
        }
        state.events.push(event.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn settle_event(
        &self,
        event: &FiscalEvent,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, event)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .events
            .iter_mut()
            .find(|stored| stored.company_id() == event.company_id() && stored.id() == event.id())
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != FiscalEventStatus::Sent {
            return Err(RepositoryError::FiscalDocumentChanged);
        }
        // Only the answer is written: what was asked is the record and is never written again.
        *stored = stored.clone().restored(
            event.status(),
            event.protocol().map(str::to_owned),
            event.refusal().cloned(),
            event.xml(),
        );
        state.audit.record(audit);
        Ok(())
    }

    async fn event_history(
        &self,
        company_id: CompanyId,
        id: FiscalEventId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::FiscalEvent, id.as_uuid()))
    }

    async fn find_event(
        &self,
        company_id: CompanyId,
        id: FiscalEventId,
    ) -> Result<Option<FiscalEvent>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .events
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn corrections_of(
        &self,
        company_id: CompanyId,
        id: FiscalDocumentId,
    ) -> Result<Vec<FiscalEvent>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut letters: Vec<FiscalEvent> = state
            .events
            .iter()
            .filter(|stored| {
                stored.company_id() == company_id
                    && matches!(stored.subject(), FiscalEventSubject::Correction { document, .. } if *document == id)
            })
            .cloned()
            .collect();
        letters.sort_by_key(|letter| match letter.subject() {
            FiscalEventSubject::Correction { sequence, .. } => *sequence,
            FiscalEventSubject::Invalidation { .. } => 0,
        });
        Ok(letters)
    }

    async fn invalidations(
        &self,
        company_id: CompanyId,
        limit: usize,
    ) -> Result<Vec<FiscalEvent>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<FiscalEvent> = state
            .events
            .iter()
            .filter(|stored| {
                stored.company_id() == company_id
                    && matches!(stored.subject(), FiscalEventSubject::Invalidation { .. })
            })
            .cloned()
            .collect();
        found.reverse();
        found.truncate(limit);
        Ok(found)
    }

    async fn number_is_used(
        &self,
        company_id: CompanyId,
        environment: FiscalEnvironment,
        series: u16,
        first: u32,
        last: u32,
    ) -> Result<bool, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.documents.iter().any(|stored| {
            stored.company_id() == company_id
                && stored.environment() == environment
                && stored.authorization().is_some_and(|authorization| {
                    authorization.series() == series
                        && (first..=last).contains(&authorization.number())
                })
        }))
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

    #[tokio::test]
    async fn issues_an_nfe_from_a_sale() {
        use crate::memory_entries::tests::Registers;
        use crate::{
            FakeFiscalGateway, InMemoryBlobStore, InMemoryFiscalSettingsRepository,
            InMemoryPaymentTermRepository, InMemoryProductRepository, InMemorySaleRepository,
        };
        use conta_verde_application::FiscalDocumentService;

        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let entry_seed = registers.seed(company).await;
        let products = InMemoryProductRepository::default();
        let seed = contract::seed_sales_records(
            &registers.parties,
            &products,
            &InMemoryPaymentTermRepository::default(),
            company,
        )
        .await;
        let fiscal = contract::seed_fiscal_customer(&registers.parties, company).await;
        let gateway = FakeFiscalGateway::authorizing_after(1);
        let settings = InMemoryFiscalSettingsRepository::default();
        let sales = InMemorySaleRepository::default();
        let service = FiscalDocumentService::new(
            InMemoryFiscalDocumentRepository::default(),
            sales.clone(),
            registers.parties.clone(),
            products,
            settings.clone(),
            gateway.clone(),
            InMemoryBlobStore::default(),
            registers.references(),
        );
        contract::issues_an_nfe_from_a_sale(
            &service,
            &gateway,
            &settings,
            &sales,
            company,
            other_company,
            &entry_seed,
            &seed,
            &fiscal,
        )
        .await;
    }

    #[tokio::test]
    async fn keeps_fiscal_events() {
        contract::fiscal_events_are_kept(
            &InMemoryFiscalDocumentRepository::default(),
            CompanyId::generate(),
            CompanyId::generate(),
            SaleId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn an_access_key_names_one_document() {
        let repository = InMemoryFiscalDocumentRepository::default();
        let company = CompanyId::generate();
        contract::fiscal_document_ready_to_authorize(&repository, company, SaleId::generate())
            .await;
        let error =
            contract::authorize_with_the_same_key(&repository, company, SaleId::generate()).await;
        assert!(matches!(error, RepositoryError::Storage(_)), "{error:?}");
    }
}
