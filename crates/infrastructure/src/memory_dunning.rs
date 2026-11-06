use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{
    Claim, DunningPlanRepository, NotificationRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, DunningPlan, EntryId,
    MAX_NOTIFICATION_ATTEMPTS, Notification, NotificationId, NotificationStatus,
};

use crate::memory_audit::AuditLog;

/// Keeps the reminder plans and their audit trail in process memory, for tests and for running the API
/// without a database. Clones share the same records.
#[derive(Default, Clone)]
pub struct InMemoryDunningPlanRepository {
    state: Arc<Mutex<PlanState>>,
}

#[derive(Default)]
struct PlanState {
    plans: HashMap<CompanyId, DunningPlan>,
    audit: AuditLog,
}

impl DunningPlanRepository for InMemoryDunningPlanRepository {
    async fn find(&self, company_id: CompanyId) -> Result<Option<DunningPlan>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.plans.get(&company_id).cloned())
    }

    async fn save(&self, plan: &DunningPlan, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, plan)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.plans.insert(plan.company_id(), plan.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn history(&self, company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::DunningPlan, company_id.as_uuid()))
    }

    async fn enabled_companies(&self) -> Result<Vec<CompanyId>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut ids: Vec<CompanyId> = state
            .plans
            .values()
            .filter(|plan| plan.is_enabled())
            .map(DunningPlan::company_id)
            .collect();
        ids.sort_by_key(|id| id.as_uuid());
        Ok(ids)
    }
}

/// Keeps the reminders in process memory, for tests and for running the API without a database. Clones
/// share the same records.
#[derive(Default, Clone)]
pub struct InMemoryNotificationRepository {
    notifications: Arc<Mutex<Vec<Notification>>>,
}

impl NotificationRepository for InMemoryNotificationRepository {
    async fn claim(&self, notification: &Notification) -> Result<Claim, RepositoryError> {
        let mut stored = self.notifications.lock().expect("repository lock poisoned");
        let same = |other: &Notification| {
            other.company_id() == notification.company_id()
                && other.entry_id() == notification.entry_id()
                && other.installment() == notification.installment()
                && other.days() == notification.days()
                && other.channel() == notification.channel()
        };
        match stored.iter_mut().find(|other| same(other)) {
            None => {
                stored.push(notification.clone());
                Ok(Claim::New(notification.clone()))
            }
            Some(existing)
                if existing.status() == NotificationStatus::Failed
                    && existing.attempts() < MAX_NOTIFICATION_ATTEMPTS =>
            {
                let again = Notification::new(
                    existing.id(),
                    existing.company_id(),
                    existing.entry_id(),
                    existing.installment(),
                    existing.days(),
                    existing.channel(),
                    notification.recipient(),
                )
                .with_state(
                    NotificationStatus::Pending,
                    existing.attempts() + 1,
                    None,
                );
                *existing = again.clone();
                Ok(Claim::Retry(again))
            }
            Some(_) => Ok(Claim::Done),
        }
    }

    async fn finish(
        &self,
        company_id: CompanyId,
        id: NotificationId,
        status: NotificationStatus,
        note: Option<&str>,
    ) -> Result<(), RepositoryError> {
        let mut stored = self.notifications.lock().expect("repository lock poisoned");
        let existing = stored
            .iter_mut()
            .find(|other| other.company_id() == company_id && other.id() == id)
            .ok_or(RepositoryError::NotFound)?;
        *existing = existing
            .clone()
            .with_state(status, existing.attempts(), note);
        Ok(())
    }

    async fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Notification>, RepositoryError> {
        let stored = self.notifications.lock().expect("repository lock poisoned");
        Ok(stored
            .iter()
            .filter(|other| other.company_id() == company_id && other.entry_id() == entry_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{
        ChargeService, CollectionService, DunningService, FinancialEntryService, PaymentPageService,
    };

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        Argon2SecretService, FakePaymentProvider, InMemoryChargeRepository,
        InMemoryCollectionSettingsRepository, InMemoryEmailSender,
        InMemoryFinancialEntryRepository, InMemoryIdentityRepository, InMemoryMessageSender,
        InMemoryPaymentLinkRepository, contract,
    };

    #[tokio::test]
    async fn sends_reminders_on_schedule() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let charge_store = InMemoryChargeRepository::default();
        let settings_store = InMemoryCollectionSettingsRepository::default();
        let emails = InMemoryEmailSender::default();
        let messages = InMemoryMessageSender::default();
        let service = DunningService::new(
            InMemoryDunningPlanRepository::default(),
            entry_store.clone(),
            registers.parties.clone(),
            PaymentPageService::new(
                InMemoryPaymentLinkRepository::default(),
                charge_store.clone(),
                entry_store.clone(),
                Argon2SecretService::new().unwrap(),
                "https://app.example",
            ),
            InMemoryNotificationRepository::default(),
            InMemoryIdentityRepository::default(),
            emails.clone(),
            messages.clone(),
        );
        let charges = ChargeService::new(
            charge_store,
            entry_store.clone(),
            registers.parties.clone(),
            FakePaymentProvider::default(),
            settings_store.clone(),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        let settings = CollectionService::new(settings_store, registers.references());
        contract::sends_reminders_on_schedule(
            &service,
            &entries,
            &registers.parties,
            &charges,
            &settings,
            &emails,
            &messages,
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn retries_a_failed_reminder_a_few_times() {
        let registers = Registers::new();
        let company = CompanyId::generate();
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let service = DunningService::new(
            InMemoryDunningPlanRepository::default(),
            entry_store.clone(),
            registers.parties.clone(),
            PaymentPageService::new(
                InMemoryPaymentLinkRepository::default(),
                InMemoryChargeRepository::default(),
                entry_store.clone(),
                Argon2SecretService::new().unwrap(),
                "https://app.example",
            ),
            InMemoryNotificationRepository::default(),
            InMemoryIdentityRepository::default(),
            InMemoryEmailSender::broken(),
            InMemoryMessageSender::default(),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        contract::retries_a_failed_reminder_a_few_times(
            &service,
            &entries,
            &registers.parties,
            company,
            &seed,
        )
        .await;
    }
}
