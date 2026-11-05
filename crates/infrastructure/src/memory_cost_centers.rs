use std::sync::Mutex;

use conta_verde_application::{
    CostCenterQuery, CostCenterRepository, NameCursor, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, CostCenter, CostCenterId,
};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

/// Keeps cost centers and their audit trail in process memory. Used by tests and for running the
/// API without a database. Like the Postgres adapter, it records an audit entry only for a write
/// that happened: a refused or missed write leaves nothing in the trail.
#[derive(Default)]
pub struct InMemoryCostCenterRepository {
    state: Mutex<CostCenterState>,
}

#[derive(Default)]
struct CostCenterState {
    cost_centers: Vec<CostCenter>,
    audit: AuditLog,
}

impl CostCenterState {
    /// Whether another cost center of the company already has the name, ignoring case.
    fn name_taken(&self, cost_center: &CostCenter) -> bool {
        self.cost_centers.iter().any(|other| {
            other.company_id() == cost_center.company_id()
                && other.id() != cost_center.id()
                && other.name().to_lowercase() == cost_center.name().to_lowercase()
        })
    }
}

impl CostCenterRepository for InMemoryCostCenterRepository {
    async fn save(
        &self,
        cost_center: &CostCenter,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, cost_center)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state.name_taken(cost_center) {
            return Err(RepositoryError::DuplicateName);
        }
        state.cost_centers.push(cost_center.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> Result<Option<CostCenter>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .cost_centers
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        cost_center: &CostCenter,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, cost_center)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .cost_centers
            .iter()
            .position(|stored| {
                stored.company_id() == cost_center.company_id() && stored.id() == cost_center.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if state.name_taken(cost_center) {
            return Err(RepositoryError::DuplicateName);
        }
        state.cost_centers[position] = cost_center.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::CostCenter, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &CostCenterQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<CostCenter>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .cost_centers
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .cloned()
            .collect();
        Ok(keyset_page(
            matching,
            query.sort(),
            after,
            limit,
            |stored| name_key(stored.name(), stored.id().as_uuid()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::CostCenterService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn stores_cost_centers_with_their_trail() {
        let repository = InMemoryCostCenterRepository::default();
        contract::stores_cost_centers_with_their_trail(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn edits_cost_centers_and_records_only_changes() {
        let service = CostCenterService::new(InMemoryCostCenterRepository::default());
        contract::edits_cost_centers_and_records_only_changes(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn filters_and_sorts_cost_centers() {
        let service = CostCenterService::new(InMemoryCostCenterRepository::default());
        contract::filters_and_sorts_cost_centers(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }
}
