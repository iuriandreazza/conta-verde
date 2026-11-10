use conta_verde_application::{ChargeRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    Actor, Amount, AuditEntity, AuditEntry, AuditRecord, CalendarDate, Charge, ChargeDraft,
    ChargeId, ChargeMethod, ChargePayment, ChargeReceipt, ChargeStatus, CompanyId, EntryId, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::postgres::storage_error;
use crate::postgres_audit;

const ONE_OPEN_PER_INSTALLMENT: &str = "charges_one_open_per_installment";

const SELECT_CHARGES: &str = "SELECT id, company_id, entry_id, installment, method, status, \
     amount::text AS amount, to_char(due_on, 'YYYY-MM-DD') AS due_on, provider, external_id, \
     digitable_line, pix_payload, payment_url, issued_by_user_id, issued_by_name, \
     to_char(paid_on, 'YYYY-MM-DD') AS paid_on, paid_amount::text AS paid_amount, fee::text AS fee, \
     note FROM charges";

pub struct PostgresChargeRepository {
    pool: PgPool,
}

impl PostgresChargeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn charge_from_row(row: &PgRow) -> Result<Charge, RepositoryError> {
    let id = ChargeId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let invalid = |what: &str| RepositoryError::Storage(format!("stored charge {id:?}: {what}"));
    let method: String = row.try_get("method").map_err(storage_error)?;
    let status: String = row.try_get("status").map_err(storage_error)?;
    let amount: String = row.try_get("amount").map_err(storage_error)?;
    let due_on: String = row.try_get("due_on").map_err(storage_error)?;
    let installment: i16 = row.try_get("installment").map_err(storage_error)?;
    let issued_by = Actor::User {
        id: UserId::from_uuid(row.try_get("issued_by_user_id").map_err(storage_error)?),
        name: row.try_get("issued_by_name").map_err(storage_error)?,
    };
    let draft = ChargeDraft {
        entry_id: EntryId::from_uuid(row.try_get("entry_id").map_err(storage_error)?),
        installment: u16::try_from(installment).map_err(|_| invalid("installment"))?,
        method: ChargeMethod::parse(&method).ok_or_else(|| invalid("method"))?,
        // Postgres hands back `10.50` for what was stored as `10.5`; `Amount` writes it the same way.
        amount: Amount::in_cents(&amount).ok_or_else(|| invalid("amount"))?,
        due_on: CalendarDate::parse(&due_on).ok_or_else(|| invalid("due date"))?,
        provider: row.try_get("provider").map_err(storage_error)?,
        external_id: row.try_get("external_id").map_err(storage_error)?,
        payment: ChargePayment {
            digitable_line: row.try_get("digitable_line").map_err(storage_error)?,
            pix_payload: row.try_get("pix_payload").map_err(storage_error)?,
            payment_url: row.try_get("payment_url").map_err(storage_error)?,
        },
    };
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let status = ChargeStatus::parse(&status).ok_or_else(|| invalid("status"))?;
    let paid_on: Option<String> = row.try_get("paid_on").map_err(storage_error)?;
    let paid_amount: Option<String> = row.try_get("paid_amount").map_err(storage_error)?;
    let fee: Option<String> = row.try_get("fee").map_err(storage_error)?;
    let receipt = match (paid_on, paid_amount, fee) {
        (Some(paid_on), Some(paid_amount), Some(fee)) => Some(ChargeReceipt {
            paid_on: CalendarDate::parse(&paid_on).ok_or_else(|| invalid("paid day"))?,
            paid_amount: Amount::in_cents(&paid_amount).ok_or_else(|| invalid("paid amount"))?,
            fee: Amount::in_cents(&fee).ok_or_else(|| invalid("fee"))?,
            note: row.try_get("note").map_err(storage_error)?,
        }),
        _ => None,
    };
    Charge::new(id, company_id, draft, issued_by)
        .map(|charge| charge.with_status(status).with_receipt(receipt))
        .map_err(|error| invalid(&format!("it is invalid: {error}")))
}

fn issued_by(charge: &Charge) -> (Uuid, &str) {
    // The column holds a person's id and name; no key reaches it ([ADR 0087]).
    let by = charge.issued_by();
    (by.id(), by.name())
}

impl ChargeRepository for PostgresChargeRepository {
    async fn save(&self, charge: &Charge, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, charge)?;
        let (user_id, user_name) = issued_by(charge);
        let payment = charge.payment();
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO charges \
             (id, company_id, entry_id, installment, method, status, amount, due_on, provider, \
              external_id, digitable_line, pix_payload, payment_url, issued_by_user_id, \
              issued_by_name) \
             VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8::date, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(charge.id().as_uuid())
        .bind(charge.company_id().as_uuid())
        .bind(charge.entry_id().as_uuid())
        .bind(i16::try_from(charge.installment()).map_err(|_| {
            RepositoryError::Storage("installment number out of range".to_owned())
        })?)
        .bind(charge.method().as_str())
        .bind(charge.status().as_str())
        .bind(charge.amount().to_string())
        .bind(charge.due_on().to_string())
        .bind(charge.provider())
        .bind(charge.external_id())
        .bind(payment.digitable_line.as_deref())
        .bind(payment.pix_payload.as_deref())
        .bind(payment.payment_url.as_deref())
        .bind(user_id)
        .bind(user_name)
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(ONE_OPEN_PER_INSTALLMENT) => {
                RepositoryError::ChargeAlreadyOpen
            }
            _ => storage_error(error),
        })?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn cancel(&self, cancelled: &Charge, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, cancelled)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE charges SET status = 'cancelled' \
             WHERE id = $1 AND company_id = $2 AND entry_id = $3 AND status = 'open'",
        )
        .bind(cancelled.id().as_uuid())
        .bind(cancelled.company_id().as_uuid())
        .bind(cancelled.entry_id().as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn mark_paid(&self, paid: &Charge, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, paid)?;
        let receipt = paid.receipt().ok_or_else(|| {
            RepositoryError::Storage("a charge is marked paid with its receipt".to_owned())
        })?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE charges SET status = 'paid', paid_on = CAST($4 AS DATE), \
             paid_amount = CAST($5 AS NUMERIC), fee = CAST($6 AS NUMERIC), note = $7 \
             WHERE id = $1 AND company_id = $2 AND entry_id = $3 AND status <> 'paid'",
        )
        .bind(paid.id().as_uuid())
        .bind(paid.company_id().as_uuid())
        .bind(paid.entry_id().as_uuid())
        .bind(receipt.paid_on.to_string())
        .bind(receipt.paid_amount.to_string())
        .bind(receipt.fee.to_string())
        .bind(receipt.note.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn expire(&self, expired: &Charge, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, expired)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE charges SET status = 'expired' \
             WHERE id = $1 AND company_id = $2 AND entry_id = $3 AND status = 'open'",
        )
        .bind(expired.id().as_uuid())
        .bind(expired.company_id().as_uuid())
        .bind(expired.entry_id().as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find_by_id(
        &self,
        company_id: CompanyId,
        id: ChargeId,
    ) -> Result<Option<Charge>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_CHARGES} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(charge_from_row).transpose()
    }

    async fn find_by_external(
        &self,
        company_id: CompanyId,
        provider: &str,
        external_id: &str,
    ) -> Result<Option<Charge>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_CHARGES} WHERE company_id = $1 AND provider = $2 AND external_id = $3"
        ))
        .bind(company_id.as_uuid())
        .bind(provider)
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(charge_from_row).transpose()
    }

    async fn find(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: ChargeId,
    ) -> Result<Option<Charge>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_CHARGES} WHERE company_id = $1 AND entry_id = $2 AND id = $3"
        ))
        .bind(company_id.as_uuid())
        .bind(entry_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(charge_from_row).transpose()
    }

    async fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Charge>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_CHARGES} WHERE company_id = $1 AND entry_id = $2 ORDER BY seq"
        ))
        .bind(company_id.as_uuid())
        .bind(entry_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(charge_from_row).collect()
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ChargeId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Charge, id.as_uuid()).await
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{
        ChargeService, CollectionService, FinancialEntryService, PaymentLinkRepository,
        PaymentPageService, PaymentWebhookService,
    };

    use super::*;
    use crate::{
        PostgresCollectionSettingsRepository, PostgresPaymentEventRepository,
        PostgresPaymentLinkRepository,
    };

    type PgReferences = RegisterReferences<
        PostgresCategoryRepository,
        PostgresCostCenterRepository,
        PostgresPartyRepository,
        PostgresBankAccountRepository,
        PostgresPaymentMethodRepository,
        crate::PostgresPeriodLockRepository,
    >;
    use crate::contract;
    use crate::{
        FakePaymentProvider, PostgresBankAccountRepository, PostgresCategoryRepository,
        PostgresCostCenterRepository, PostgresFinancialEntryRepository, PostgresPartyRepository,
        PostgresPaymentMethodRepository, RegisterReferences,
    };

    fn references(pool: &PgPool) -> PgReferences {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
            crate::PostgresPeriodLockRepository::new(pool.clone()),
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

    fn charge_service(
        pool: &PgPool,
        provider: &FakePaymentProvider,
    ) -> ChargeService<
        PostgresChargeRepository,
        PostgresFinancialEntryRepository,
        PostgresPartyRepository,
        FakePaymentProvider,
        PostgresCollectionSettingsRepository,
    > {
        ChargeService::new(
            PostgresChargeRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            provider.clone(),
            PostgresCollectionSettingsRepository::new(pool.clone()),
        )
    }

    fn entry_service(
        pool: &PgPool,
    ) -> FinancialEntryService<PostgresFinancialEntryRepository, PgReferences> {
        FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(pool),
        )
    }

    fn settings_service(
        pool: &PgPool,
    ) -> CollectionService<PostgresCollectionSettingsRepository, PgReferences> {
        CollectionService::new(
            PostgresCollectionSettingsRepository::new(pool.clone()),
            references(pool),
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn issues_and_cancels_charges() {
        let pool = contract::test_pool().await;
        let (company, other_company, seed) = seeded(&pool).await;
        let provider = FakePaymentProvider::default();
        contract::issues_and_cancels_charges(
            &charge_service(&pool, &provider),
            &PostgresChargeRepository::new(pool.clone()),
            &entry_service(&pool),
            &settings_service(&pool),
            &provider,
            &PostgresPartyRepository::new(pool.clone()),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn configures_collection() {
        let pool = contract::test_pool().await;
        let (company, other_company, seed) = seeded(&pool).await;
        contract::configures_collection(&settings_service(&pool), company, other_company, &seed)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn serves_payment_pages() {
        let pool = contract::test_pool().await;
        let (company, other_company, seed) = seeded(&pool).await;
        let provider = FakePaymentProvider::default();
        let links = PostgresPaymentLinkRepository::new(pool.clone());
        let pages = PaymentPageService::new(
            PostgresPaymentLinkRepository::new(pool.clone()),
            PostgresChargeRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            crate::Argon2SecretService::new().unwrap(),
            "https://app.example/",
        );
        contract::serves_payment_pages(
            &pages,
            &links,
            &charge_service(&pool, &provider),
            &entry_service(&pool),
            &settings_service(&pool),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn settles_charges_from_webhooks() {
        let pool = contract::test_pool().await;
        let (company, other_company, seed) = seeded(&pool).await;
        let provider = FakePaymentProvider::default();
        let events = PostgresPaymentEventRepository::new(pool.clone());
        let webhooks = PaymentWebhookService::new(
            provider.clone(),
            PostgresChargeRepository::new(pool.clone()),
            PostgresPaymentEventRepository::new(pool.clone()),
            PostgresCollectionSettingsRepository::new(pool.clone()),
            entry_service(&pool),
        );
        contract::settles_charges_from_webhooks(
            &webhooks,
            &charge_service(&pool, &provider),
            &entry_service(&pool),
            &settings_service(&pool),
            &conta_verde_application::PeriodLockService::new(
                crate::PostgresPeriodLockRepository::new(pool.clone()),
            ),
            &events,
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_and_an_entry_of_another_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await;
        let entries = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
                crate::PostgresPeriodLockRepository::new(pool.clone()),
            ),
        );
        let entry = entries
            .register(
                company,
                &contract::actor(),
                contract::entry_draft(&seed, "Venda", "2026-11-10", &[("2026-12-10", "10")]),
            )
            .await
            .unwrap();
        let store = PostgresChargeRepository::new(pool.clone());
        let charge = |company: CompanyId, external: &str| {
            Charge::new(
                ChargeId::generate(),
                company,
                ChargeDraft {
                    entry_id: entry.id(),
                    installment: 1,
                    method: ChargeMethod::Pix,
                    amount: Amount::in_cents("10").unwrap(),
                    due_on: CalendarDate::parse("2026-12-10").unwrap(),
                    provider: "fake".into(),
                    external_id: external.into(),
                    payment: ChargePayment {
                        pix_payload: Some("0002".into()),
                        ..ChargePayment::default()
                    },
                },
                contract::actor(),
            )
            .unwrap()
        };
        let audit = |charge: &Charge| AuditEntry {
            company_id: charge.company_id(),
            actor: contract::actor(),
            entity: AuditEntity::Charge,
            entity_id: charge.id().as_uuid(),
            action: conta_verde_domain::AuditAction::Created,
            changes: Vec::new(),
        };

        // An entry of another company is refused by the composite key, and leaves no trail.
        let foreign = charge(other_company, "ch_foreign");
        assert!(store.save(&foreign, &audit(&foreign)).await.is_err());
        assert!(
            store
                .history(other_company, foreign.id())
                .await
                .unwrap()
                .is_empty()
        );

        // The provider's id is found again within the company: the same id twice is refused.
        let first = charge(company, "ch_same");
        store.save(&first, &audit(&first)).await.unwrap();
        let again = charge(company, "ch_same");
        assert!(store.save(&again, &audit(&again)).await.is_err());
        sqlx::query("UPDATE charges SET status = 'cancelled' WHERE id = $1")
            .bind(first.id().as_uuid())
            .execute(&pool)
            .await
            .unwrap();

        // What the domain would refuse, the table refuses too, whatever the adapter was told.
        for statement in [
            "UPDATE charges SET installment = 0",
            "UPDATE charges SET method = 'cash'",
            "UPDATE charges SET status = 'lost'",
            "UPDATE charges SET amount = 0",
            "UPDATE charges SET amount = 10.005",
            "UPDATE charges SET provider = 'Fake'",
            "UPDATE charges SET external_id = ' x'",
            "UPDATE charges SET external_id = 'a' || chr(10) || 'b'",
            "UPDATE charges SET pix_payload = NULL",
            "UPDATE charges SET pix_payload = 'a' || chr(10) || 'b'",
            "UPDATE charges SET digitable_line = '123'",
            "UPDATE charges SET method = 'card_link', pix_payload = NULL, payment_url = 'http://x.test/a'",
            "UPDATE charges SET method = 'card_link', pix_payload = NULL, payment_url = 'https://x.test/a b'",
        ] {
            let kept = charge(company, &format!("ch_{}", Uuid::new_v4().simple()));
            store.save(&kept, &audit(&kept)).await.unwrap();
            let refused = sqlx::query(&format!("{statement} WHERE id = $1"))
                .bind(kept.id().as_uuid())
                .execute(&pool)
                .await;
            assert!(refused.is_err(), "{statement}");
            // Out of the way of the next one, since an installment has one open charge at a time.
            sqlx::query("UPDATE charges SET status = 'cancelled' WHERE id = $1")
                .bind(kept.id().as_uuid())
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_receipts_events_and_settings_the_domain_would() {
        let pool = contract::test_pool().await;
        let (company, other_company, seed) = seeded(&pool).await;
        let entries = entry_service(&pool);
        let entry = entries
            .register(
                company,
                &contract::actor(),
                contract::entry_draft(&seed, "Venda", "2026-11-10", &[("2026-12-10", "10")]),
            )
            .await
            .unwrap();
        let store = PostgresChargeRepository::new(pool.clone());
        let charge = Charge::new(
            ChargeId::generate(),
            company,
            ChargeDraft {
                entry_id: entry.id(),
                installment: 1,
                method: ChargeMethod::Pix,
                amount: Amount::in_cents("10").unwrap(),
                due_on: CalendarDate::parse("2026-12-10").unwrap(),
                provider: "fake".into(),
                external_id: "ch_receipts".into(),
                payment: ChargePayment {
                    pix_payload: Some("0002".into()),
                    ..ChargePayment::default()
                },
            },
            contract::actor(),
        )
        .unwrap();
        store
            .save(
                &charge,
                &AuditEntry {
                    company_id: company,
                    actor: contract::actor(),
                    entity: AuditEntity::Charge,
                    entity_id: charge.id().as_uuid(),
                    action: conta_verde_domain::AuditAction::Created,
                    changes: Vec::new(),
                },
            )
            .await
            .unwrap();

        // A charge is paid exactly when it has its receipt, and the receipt is in cents.
        for statement in [
            "UPDATE charges SET status = 'paid'",
            "UPDATE charges SET paid_on = DATE '2026-12-09'",
            "UPDATE charges SET status = 'paid', paid_on = DATE '2026-12-09', paid_amount = 0, fee = 0",
            "UPDATE charges SET status = 'paid', paid_on = DATE '2026-12-09', paid_amount = 10, fee = -1",
            "UPDATE charges SET status = 'paid', paid_on = DATE '2026-12-09', paid_amount = 10.005, fee = 0",
            "UPDATE charges SET status = 'paid', paid_on = DATE '2026-12-09', paid_amount = 10, fee = 0, note = ' x'",
            "UPDATE charges SET status = 'paid', paid_on = DATE '2026-12-09', paid_amount = 10, fee = 0, note = 'a' || chr(10)",
            "UPDATE charges SET note = 'a note on a charge that is not paid'",
        ] {
            let refused = sqlx::query(&format!("{statement} WHERE id = $1"))
                .bind(charge.id().as_uuid())
                .execute(&pool)
                .await;
            assert!(refused.is_err(), "{statement}");
        }

        // The notices: one per provider's id, short, one line, and not for another company's charge.
        let charge_uuid = charge.id().as_uuid();
        let insert = |company: CompanyId, provider: &str, event: &str, raw: Vec<u8>| {
            let pool = pool.clone();
            let (provider, event) = (provider.to_owned(), event.to_owned());
            async move {
                sqlx::query(
                    "INSERT INTO payment_events (id, company_id, charge_id, provider, event_id, kind, raw) \
                     VALUES ($1, $2, $3, $4, $5, 'paid', $6)",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(charge_uuid)
                .bind(provider)
                .bind(event)
                .bind(raw)
                .execute(&pool)
                .await
            }
        };
        assert!(insert(company, "fake", "evt_ok", vec![1]).await.is_ok());
        assert!(insert(company, "fake", "evt_ok", vec![1]).await.is_err());
        assert!(insert(company, "Fake", "evt_a", vec![1]).await.is_err());
        assert!(insert(company, "fake", " evt_b", vec![1]).await.is_err());
        assert!(insert(company, "fake", "evt_\n", vec![1]).await.is_err());
        assert!(
            insert(company, "fake", "evt_big", vec![0; 65537])
                .await
                .is_err()
        );
        assert!(
            insert(other_company, "fake", "evt_other", vec![1])
                .await
                .is_err()
        );
        for statement in [
            "UPDATE payment_events SET outcome = 'lost'",
            "UPDATE payment_events SET kind = 'refunded'",
            "UPDATE payment_events SET note = ''",
        ] {
            assert!(
                sqlx::query(statement).execute(&pool).await.is_err(),
                "{statement}"
            );
        }

        // A link is to the company's own charge and holds a digest, never a short text.
        let links = PostgresPaymentLinkRepository::new(pool.clone());
        // A digest is one link's for good, so each run makes its own.
        let digest = Uuid::new_v4().simple().to_string();
        assert!(
            links
                .save(other_company, charge.id(), &digest)
                .await
                .is_err()
        );
        assert!(links.save(company, charge.id(), "short").await.is_err());
        links.save(company, charge.id(), &digest).await.unwrap();
        assert!(
            links.save(company, charge.id(), &digest).await.is_err(),
            "a digest is one link's"
        );

        // Settings point at the company's own account and category.
        assert!(
            sqlx::query(
                "INSERT INTO collection_settings (company_id, bank_account_id) VALUES ($1, $2)"
            )
            .bind(other_company.as_uuid())
            .bind(seed.bank_account.as_uuid())
            .execute(&pool)
            .await
            .is_err()
        );
        assert!(
            sqlx::query(
                "INSERT INTO collection_settings (company_id, bank_account_id, fee_category_id) \
                 VALUES ($1, $2, $3)",
            )
            .bind(other_company.as_uuid())
            .bind(Uuid::new_v4())
            .bind(seed.category.as_uuid())
            .execute(&pool)
            .await
            .is_err()
        );
    }
}
