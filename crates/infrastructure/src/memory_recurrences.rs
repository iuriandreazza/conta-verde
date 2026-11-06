use std::sync::{Arc, Mutex};

use conta_verde_application::{
    NameCursor, RecurrenceQuery, RecurrenceRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CalendarDate, CompanyId, Recurrence, RecurrenceId,
};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

/// Keeps recurrences and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened. Clones share the same recurrences.
#[derive(Default, Clone)]
pub struct InMemoryRecurrenceRepository {
    state: Arc<Mutex<RecurrenceState>>,
}

#[derive(Default)]
struct RecurrenceState {
    recurrences: Vec<Recurrence>,
    audit: AuditLog,
}

impl RecurrenceRepository for InMemoryRecurrenceRepository {
    async fn save(
        &self,
        recurrence: &Recurrence,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, recurrence)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.recurrences.push(recurrence.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> Result<Option<Recurrence>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .recurrences
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        recurrence: &Recurrence,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, recurrence)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .recurrences
            .iter_mut()
            .find(|stored| {
                stored.company_id() == recurrence.company_id() && stored.id() == recurrence.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        *stored = recurrence.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn mark_generated(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
        through: CalendarDate,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        if let Some(stored) = state
            .recurrences
            .iter_mut()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .filter(|stored| stored.generated_through().is_none_or(|done| done < through))
        {
            *stored = stored.clone().with_generated_through(Some(through));
        }
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Recurrence, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &RecurrenceQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<Recurrence>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .recurrences
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .cloned()
            .collect();
        Ok(keyset_page(
            matching,
            query.sort(),
            after,
            limit,
            |stored| name_key(stored.description(), stored.id().as_uuid()),
        ))
    }

    async fn list_to_generate(
        &self,
        until: CalendarDate,
        limit: usize,
    ) -> Result<Vec<Recurrence>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut due: Vec<Recurrence> = state
            .recurrences
            .iter()
            .filter(|stored| {
                let done = stored.generated_through();
                stored.is_active()
                    && stored.starts_on() <= until
                    && done.is_none_or(|day| day < until)
                    && stored
                        .ends_on()
                        .is_none_or(|end| done.is_none_or(|day| day < end))
            })
            .cloned()
            .collect();
        due.sort_by_key(|stored| (stored.generated_through(), stored.id().as_uuid()));
        due.truncate(limit);
        Ok(due)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{FinancialEntryService, RecurrenceService};

    use super::*;
    use crate::InMemoryFinancialEntryRepository;
    use crate::contract;
    use crate::memory_entries::tests::Registers;

    #[tokio::test]
    async fn generates_recurring_entries_ahead() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let store = InMemoryRecurrenceRepository::default();
        let entry_store = InMemoryFinancialEntryRepository::default();
        let service = RecurrenceService::new(
            store.clone(),
            FinancialEntryService::new(entry_store.clone(), registers.references()),
            registers.references(),
        );
        contract::generates_recurring_entries_ahead(
            &service,
            &store,
            &entry_store,
            &registers.categories,
            company,
            other_company,
            &seed,
        )
        .await;
    }
}
