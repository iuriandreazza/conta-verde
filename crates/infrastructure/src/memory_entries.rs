use std::sync::Mutex;

use conta_verde_application::{
    EntryCursor, EntrySort, FinancialEntryQuery, FinancialEntryRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, EntryId, FinancialEntry,
};

use crate::memory_audit::AuditLog;

/// Keeps entries and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened: a refused or missed write leaves nothing in the trail, and the installments of an entry
/// it could not change stay as they were.
#[derive(Default)]
pub struct InMemoryFinancialEntryRepository {
    state: Mutex<EntryState>,
}

#[derive(Default)]
struct EntryState {
    entries: Vec<FinancialEntry>,
    audit: AuditLog,
}

impl FinancialEntryRepository for InMemoryFinancialEntryRepository {
    async fn save(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, entry)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.entries.push(entry.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<Option<FinancialEntry>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .entries
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, entry)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .entries
            .iter()
            .position(|stored| {
                stored.company_id() == entry.company_id() && stored.id() == entry.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        state.entries[position] = entry.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::FinancialEntry, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &FinancialEntryQuery,
        after: Option<&EntryCursor>,
        limit: usize,
    ) -> Result<Vec<FinancialEntry>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let descending = query.sort() == EntrySort::CompetenceDesc;
        let key = |entry: &FinancialEntry| (entry.competence_date(), entry.id().as_uuid());
        let mut rows: Vec<FinancialEntry> = state
            .entries
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .filter(|stored| {
                after.is_none_or(|cursor| {
                    let after_key = (cursor.competence_date, cursor.id);
                    if descending {
                        key(stored) < after_key
                    } else {
                        key(stored) > after_key
                    }
                })
            })
            .cloned()
            .collect();
        rows.sort_by_key(key);
        if descending {
            rows.reverse();
        }
        rows.truncate(limit);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::FinancialEntryService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract::{self, EntrySeed};
    use crate::{
        InMemoryCategoryRepository, InMemoryCostCenterRepository, InMemoryPartyRepository,
        RegisterReferences,
    };

    /// The registers an entry points at, shared by handle with the references that read them.
    struct Registers {
        categories: InMemoryCategoryRepository,
        cost_centers: InMemoryCostCenterRepository,
        parties: InMemoryPartyRepository,
    }

    impl Registers {
        fn new() -> Self {
            Self {
                categories: InMemoryCategoryRepository::default(),
                cost_centers: InMemoryCostCenterRepository::default(),
                parties: InMemoryPartyRepository::default(),
            }
        }

        async fn seed(&self, company: CompanyId) -> EntrySeed {
            contract::seed_entry_records(
                &self.categories,
                &self.cost_centers,
                &self.parties,
                company,
            )
            .await
        }

        fn references(
            &self,
        ) -> RegisterReferences<
            InMemoryCategoryRepository,
            InMemoryCostCenterRepository,
            InMemoryPartyRepository,
        > {
            RegisterReferences::new(
                self.categories.clone(),
                self.cost_centers.clone(),
                self.parties.clone(),
            )
        }
    }

    #[tokio::test]
    async fn the_registers_say_what_an_entry_may_point_at() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        contract::entry_references_say_what_is_usable(
            &registers.references(),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn stores_entries_with_their_trail() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        contract::stores_entries_with_their_trail(
            &InMemoryFinancialEntryRepository::default(),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn edits_entries_and_records_only_changes() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::edits_entries_and_records_only_changes(&service, company, other_company, &seed)
            .await;
    }

    #[tokio::test]
    async fn an_edit_may_keep_what_was_deactivated_since() {
        let registers = Registers::new();
        let company = CompanyId::generate();
        let seed = registers.seed(company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::an_edit_may_keep_what_was_deactivated_since(
            &service,
            &registers.categories,
            company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn filters_and_sorts_entries() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let other_seed = registers.seed(other_company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::filters_and_sorts_entries(&service, company, other_company, &other_seed, &seed)
            .await;
    }
}
