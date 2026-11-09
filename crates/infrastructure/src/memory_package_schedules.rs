use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use conta_verde_application::{PackageScheduleRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CalendarDate, CompanyId, PackageSchedule,
};

use crate::memory_audit::AuditLog;

/// Keeps the schedules of the monthly package, the months already announced and the schedules' audit trail in process
/// memory, for tests and for running the API without a database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryPackageScheduleRepository {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    schedules: HashMap<CompanyId, PackageSchedule>,
    claims: HashSet<(CompanyId, CalendarDate)>,
    audit: AuditLog,
}

impl PackageScheduleRepository for InMemoryPackageScheduleRepository {
    async fn find(
        &self,
        company_id: CompanyId,
    ) -> Result<Option<PackageSchedule>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.schedules.get(&company_id).copied())
    }

    async fn save(
        &self,
        schedule: &PackageSchedule,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, schedule)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.schedules.insert(schedule.company_id(), *schedule);
        state.audit.record(audit);
        Ok(())
    }

    async fn history(&self, company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.audit.history(
            company_id,
            AuditEntity::PackageSchedule,
            company_id.as_uuid(),
        ))
    }

    async fn due(&self, day_of_month: u8) -> Result<Vec<CompanyId>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut due: Vec<CompanyId> = state
            .schedules
            .values()
            .filter(|schedule| schedule.is_enabled() && schedule.day() <= day_of_month)
            .map(PackageSchedule::company_id)
            .collect();
        due.sort_by_key(|company| company.as_uuid());
        Ok(due)
    }

    async fn claim(
        &self,
        company_id: CompanyId,
        month: CalendarDate,
        _at_millis: i64,
    ) -> Result<bool, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        Ok(state.claims.insert((company_id, month)))
    }

    async fn release(
        &self,
        company_id: CompanyId,
        month: CalendarDate,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.claims.remove(&(company_id, month));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::PackageScheduleService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::{InMemoryEmailSender, InMemoryIdentityRepository, contract};

    #[tokio::test]
    async fn stores_the_package_schedule() {
        contract::stores_the_package_schedule(
            &InMemoryPackageScheduleRepository::default(),
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn announces_the_monthly_package() {
        let (store, identities, mailbox) = (
            InMemoryPackageScheduleRepository::default(),
            InMemoryIdentityRepository::default(),
            InMemoryEmailSender::default(),
        );
        let service = PackageScheduleService::new(
            store,
            identities.clone(),
            mailbox.clone(),
            "https://contas.example.com/",
        );
        contract::announces_the_monthly_package(&service, &identities, &mailbox).await;
    }

    #[tokio::test]
    async fn gives_the_month_back_when_nothing_could_be_queued() {
        let (store, identities) = (
            InMemoryPackageScheduleRepository::default(),
            InMemoryIdentityRepository::default(),
        );
        let service = PackageScheduleService::new(
            store.clone(),
            identities.clone(),
            InMemoryEmailSender::broken(),
            "https://contas.example.com",
        );
        contract::gives_the_month_back_when_nothing_could_be_queued(&service, &store, &identities)
            .await;
    }
}
