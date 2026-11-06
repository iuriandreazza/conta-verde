use conta_verde_application::{
    Claim, DunningPlanRepository, NotificationRepository, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, DunningChannel, DunningPlan, DunningRule,
    EntryId, MAX_NOTIFICATION_ATTEMPTS, Notification, NotificationId, NotificationStatus,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

pub struct PostgresDunningPlanRepository {
    pool: PgPool,
}

impl PostgresDunningPlanRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl DunningPlanRepository for PostgresDunningPlanRepository {
    async fn find(&self, company_id: CompanyId) -> Result<Option<DunningPlan>, RepositoryError> {
        let enabled: Option<bool> =
            sqlx::query_scalar("SELECT enabled FROM dunning_plans WHERE company_id = $1")
                .bind(company_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage_error)?;
        let Some(enabled) = enabled else {
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT days, channel FROM dunning_rules WHERE company_id = $1 ORDER BY position",
        )
        .bind(company_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let rules = rows
            .iter()
            .map(|row| {
                let channel: String = row.try_get("channel").map_err(storage_error)?;
                Ok(DunningRule {
                    days: row.try_get("days").map_err(storage_error)?,
                    channel: DunningChannel::parse(&channel).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored rule with channel {channel:?}"))
                    })?,
                })
            })
            .collect::<Result<Vec<_>, RepositoryError>>()?;
        DunningPlan::new(company_id, enabled, rules)
            .map(Some)
            .map_err(|error| RepositoryError::Storage(format!("stored plan is invalid: {error}")))
    }

    async fn save(&self, plan: &DunningPlan, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, plan)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO dunning_plans (company_id, enabled) VALUES ($1, $2) \
             ON CONFLICT (company_id) DO UPDATE SET enabled = EXCLUDED.enabled",
        )
        .bind(plan.company_id().as_uuid())
        .bind(plan.is_enabled())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        // The rules are replaced with the plan, in one transaction.
        sqlx::query("DELETE FROM dunning_rules WHERE company_id = $1")
            .bind(plan.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        for (position, rule) in plan.rules().iter().enumerate() {
            sqlx::query(
                "INSERT INTO dunning_rules (company_id, position, days, channel) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(plan.company_id().as_uuid())
            .bind(i16::try_from(position + 1).unwrap_or(i16::MAX))
            .bind(rule.days)
            .bind(rule.channel.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(&self, company_id: CompanyId) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::DunningPlan,
            company_id.as_uuid(),
        )
        .await
    }

    async fn enabled_companies(&self) -> Result<Vec<CompanyId>, RepositoryError> {
        let ids: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT company_id FROM dunning_plans WHERE enabled ORDER BY company_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(ids.into_iter().map(CompanyId::from_uuid).collect())
    }
}

pub struct PostgresNotificationRepository {
    pool: PgPool,
}

impl PostgresNotificationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn notification_from_row(row: &PgRow) -> Result<Notification, RepositoryError> {
    let id = NotificationId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let invalid =
        |what: &str| RepositoryError::Storage(format!("stored notification {id:?}: {what}"));
    let channel: String = row.try_get("channel").map_err(storage_error)?;
    let status: String = row.try_get("status").map_err(storage_error)?;
    let installment: i16 = row.try_get("installment").map_err(storage_error)?;
    let attempts: i16 = row.try_get("attempts").map_err(storage_error)?;
    let note: Option<String> = row.try_get("note").map_err(storage_error)?;
    let recipient: String = row.try_get("recipient").map_err(storage_error)?;
    Ok(Notification::new(
        id,
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        EntryId::from_uuid(row.try_get("entry_id").map_err(storage_error)?),
        u16::try_from(installment).map_err(|_| invalid("installment"))?,
        row.try_get("days").map_err(storage_error)?,
        DunningChannel::parse(&channel).ok_or_else(|| invalid("channel"))?,
        &recipient,
    )
    .with_state(
        NotificationStatus::parse(&status).ok_or_else(|| invalid("status"))?,
        u8::try_from(attempts).map_err(|_| invalid("attempts"))?,
        note.as_deref(),
    ))
}

impl NotificationRepository for PostgresNotificationRepository {
    async fn claim(&self, notification: &Notification) -> Result<Claim, RepositoryError> {
        // One statement decides: a new row, or a failed one with attempts left made pending again, or
        // nothing (then it is somebody's already). `xmax = 0` tells an insert from an update.
        let row = sqlx::query(
            "INSERT INTO notifications \
             (id, company_id, entry_id, installment, days, channel, recipient, status, attempts) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', 1) \
             ON CONFLICT (company_id, entry_id, installment, days, channel) DO UPDATE \
             SET status = 'pending', attempts = notifications.attempts + 1, note = NULL, \
                 recipient = EXCLUDED.recipient \
             WHERE notifications.status = 'failed' AND notifications.attempts < $8 \
             RETURNING id, company_id, entry_id, installment, days, channel, recipient, status, \
                       attempts, note, (xmax = 0) AS inserted",
        )
        .bind(notification.id().as_uuid())
        .bind(notification.company_id().as_uuid())
        .bind(notification.entry_id().as_uuid())
        .bind(i16::try_from(notification.installment()).unwrap_or(i16::MAX))
        .bind(notification.days())
        .bind(notification.channel().as_str())
        .bind(notification.recipient())
        .bind(i16::from(MAX_NOTIFICATION_ATTEMPTS))
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else {
            return Ok(Claim::Done);
        };
        let inserted: bool = row.try_get("inserted").map_err(storage_error)?;
        let stored = notification_from_row(&row)?;
        Ok(if inserted {
            Claim::New(stored)
        } else {
            Claim::Retry(stored)
        })
    }

    async fn finish(
        &self,
        company_id: CompanyId,
        id: NotificationId,
        status: NotificationStatus,
        note: Option<&str>,
    ) -> Result<(), RepositoryError> {
        let shaped = Notification::new(
            id,
            company_id,
            EntryId::generate(),
            1,
            0,
            DunningChannel::Email,
            "x",
        )
        .with_state(status, 1, note);
        let result = sqlx::query(
            "UPDATE notifications SET status = $3, note = $4 WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .bind(status.as_str())
        .bind(shaped.note())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Notification>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, company_id, entry_id, installment, days, channel, recipient, status, attempts, \
             note FROM notifications WHERE company_id = $1 AND entry_id = $2 ORDER BY seq",
        )
        .bind(company_id.as_uuid())
        .bind(entry_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(notification_from_row).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;
    use crate::{
        Argon2SecretService, FakePaymentProvider, InMemoryEmailSender, InMemoryMessageSender,
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresChargeRepository,
        PostgresCollectionSettingsRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresIdentityRepository, PostgresPartyRepository,
        PostgresPaymentLinkRepository, PostgresPaymentMethodRepository, RegisterReferences,
    };
    use conta_verde_application::{
        ChargeService, CollectionService, DunningService, EmailSender, FinancialEntryService,
        MessageSender, PaymentPageService,
    };
    use conta_verde_domain::AuditAction;

    type PgReferences = RegisterReferences<
        PostgresCategoryRepository,
        PostgresCostCenterRepository,
        PostgresPartyRepository,
        PostgresBankAccountRepository,
        PostgresPaymentMethodRepository,
    >;

    fn references(pool: &PgPool) -> PgReferences {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
        )
    }

    async fn seeded(pool: &PgPool) -> (CompanyId, CompanyId, contract::EntrySeed) {
        let company = contract::create_company(pool).await;
        let other_company = contract::create_company(pool).await;
        let seed = contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await;
        (company, other_company, seed)
    }

    type PgService<M, W> = DunningService<
        PostgresDunningPlanRepository,
        PostgresFinancialEntryRepository,
        PostgresPartyRepository,
        PaymentPageService<
            PostgresPaymentLinkRepository,
            PostgresChargeRepository,
            PostgresFinancialEntryRepository,
            Argon2SecretService,
        >,
        PostgresNotificationRepository,
        PostgresIdentityRepository,
        M,
        W,
    >;

    fn service<M: EmailSender, W: MessageSender>(
        pool: &PgPool,
        emails: M,
        messages: W,
    ) -> PgService<M, W> {
        DunningService::new(
            PostgresDunningPlanRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PaymentPageService::new(
                PostgresPaymentLinkRepository::new(pool.clone()),
                PostgresChargeRepository::new(pool.clone()),
                PostgresFinancialEntryRepository::new(pool.clone()),
                Argon2SecretService::new().unwrap(),
                "https://app.example",
            ),
            PostgresNotificationRepository::new(pool.clone()),
            PostgresIdentityRepository::new(pool.clone()),
            emails,
            messages,
        )
    }

    fn entries(
        pool: &PgPool,
    ) -> FinancialEntryService<PostgresFinancialEntryRepository, PgReferences> {
        FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(pool),
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn sends_reminders_on_schedule() {
        let pool = contract::test_pool().await;
        let (company, other_company, seed) = seeded(&pool).await;
        let emails = InMemoryEmailSender::default();
        let messages = InMemoryMessageSender::default();
        let charges = ChargeService::new(
            PostgresChargeRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            FakePaymentProvider::default(),
            PostgresCollectionSettingsRepository::new(pool.clone()),
        );
        let settings = CollectionService::new(
            PostgresCollectionSettingsRepository::new(pool.clone()),
            references(&pool),
        );
        contract::sends_reminders_on_schedule(
            &service(&pool, emails.clone(), messages.clone()),
            &entries(&pool),
            &PostgresPartyRepository::new(pool.clone()),
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
    #[ignore = "requires DATABASE_URL"]
    async fn retries_a_failed_reminder_a_few_times() {
        let pool = contract::test_pool().await;
        let (company, _, seed) = seeded(&pool).await;
        contract::retries_a_failed_reminder_a_few_times(
            &service(
                &pool,
                InMemoryEmailSender::broken(),
                InMemoryMessageSender::default(),
            ),
            &entries(&pool),
            &PostgresPartyRepository::new(pool.clone()),
            company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn two_passes_at_once_claim_a_reminder_once_and_the_database_refuses_what_the_domain_would()
     {
        let pool = contract::test_pool().await;
        let (company, other_company, seed) = seeded(&pool).await;
        let sale = entries(&pool)
            .register(
                company,
                &contract::actor(),
                contract::entry_draft(&seed, "Venda", "2026-11-10", &[("2026-12-10", "10")]),
            )
            .await
            .unwrap();
        let store = std::sync::Arc::new(PostgresNotificationRepository::new(pool.clone()));
        let candidate = |company: CompanyId, entry: EntryId| {
            Notification::new(
                NotificationId::generate(),
                company,
                entry,
                1,
                0,
                DunningChannel::Email,
                "a@example.com",
            )
        };

        // Ten passes at the same moment: one gets the reminder.
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let store = store.clone();
            let notification = candidate(company, sale.id());
            tasks.push(tokio::spawn(async move {
                store.claim(&notification).await.unwrap()
            }));
        }
        let mut news = 0;
        for task in tasks {
            if matches!(task.await.unwrap(), Claim::New(_)) {
                news += 1;
            }
        }
        assert_eq!(news, 1);

        // An entry of another company is refused by the composite key.
        assert!(
            store
                .claim(&candidate(other_company, sale.id()))
                .await
                .is_err()
        );

        // What the domain would refuse, the table refuses too.
        for statement in [
            "UPDATE notifications SET installment = 0",
            "UPDATE notifications SET days = 181",
            "UPDATE notifications SET channel = 'sms'",
            "UPDATE notifications SET status = 'lost'",
            "UPDATE notifications SET attempts = 4",
            "UPDATE notifications SET recipient = ''",
            "UPDATE notifications SET note = ''",
        ] {
            assert!(
                sqlx::query(statement).execute(&pool).await.is_err(),
                "{statement}"
            );
        }

        // The plan: rules once each, in range, of a known channel, and only a plan's own.
        let plans = PostgresDunningPlanRepository::new(pool.clone());
        let plan = DunningPlan::new(company, true, Vec::new()).unwrap();
        plans
            .save(
                &plan,
                &conta_verde_domain::AuditEntry {
                    company_id: company,
                    actor: contract::actor(),
                    entity: AuditEntity::DunningPlan,
                    entity_id: company.as_uuid(),
                    action: AuditAction::Created,
                    changes: Vec::new(),
                },
            )
            .await
            .unwrap();
        let rule = |company: CompanyId, position: i16, days: i16, channel: &str| {
            let pool = pool.clone();
            let channel = channel.to_owned();
            async move {
                sqlx::query(
                    "INSERT INTO dunning_rules (company_id, position, days, channel) VALUES ($1, $2, $3, $4)",
                )
                .bind(company.as_uuid())
                .bind(position)
                .bind(days)
                .bind(channel)
                .execute(&pool)
                .await
            }
        };
        assert!(rule(company, 1, -3, "email").await.is_ok());
        assert!(rule(company, 2, -3, "email").await.is_err(), "a rule once");
        assert!(rule(company, 3, 181, "email").await.is_err());
        assert!(rule(company, 4, -61, "email").await.is_err());
        assert!(rule(company, 5, 0, "sms").await.is_err());
        assert!(rule(company, 21, 0, "email").await.is_err());
        assert!(
            rule(other_company, 1, 0, "email").await.is_err(),
            "no plan, no rules"
        );
    }
}
