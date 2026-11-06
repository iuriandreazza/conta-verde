use conta_verde_application::{ChargeRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    Actor, Amount, AuditEntity, AuditEntry, AuditRecord, CalendarDate, Charge, ChargeDraft,
    ChargeId, ChargeMethod, ChargePayment, ChargeStatus, CompanyId, EntryId, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::postgres::storage_error;
use crate::postgres_audit;

const ONE_OPEN_PER_INSTALLMENT: &str = "charges_one_open_per_installment";

const SELECT_CHARGES: &str = "SELECT id, company_id, entry_id, installment, method, status, \
     amount::text AS amount, to_char(due_on, 'YYYY-MM-DD') AS due_on, provider, external_id, \
     digitable_line, pix_payload, payment_url, issued_by_user_id, issued_by_name FROM charges";

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
    Charge::new(id, company_id, draft, issued_by)
        .map(|charge| charge.with_status(status))
        .map_err(|error| invalid(&format!("it is invalid: {error}")))
}

fn issued_by(charge: &Charge) -> (Uuid, &str) {
    let Actor::User { id, name } = charge.issued_by();
    (id.as_uuid(), name)
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
    use conta_verde_application::{ChargeService, FinancialEntryService};

    use super::*;
    use crate::contract;
    use crate::{
        FakePaymentProvider, PostgresBankAccountRepository, PostgresCategoryRepository,
        PostgresCostCenterRepository, PostgresFinancialEntryRepository, PostgresPartyRepository,
        PostgresPaymentMethodRepository, RegisterReferences,
    };

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn issues_and_cancels_charges() {
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
        let provider = FakePaymentProvider::default();
        let store = PostgresChargeRepository::new(pool.clone());
        let service = ChargeService::new(
            PostgresChargeRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            provider.clone(),
        );
        let entries = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            RegisterReferences::new(
                PostgresCategoryRepository::new(pool.clone()),
                PostgresCostCenterRepository::new(pool.clone()),
                PostgresPartyRepository::new(pool.clone()),
                PostgresBankAccountRepository::new(pool.clone()),
                PostgresPaymentMethodRepository::new(pool.clone()),
            ),
        );
        contract::issues_and_cancels_charges(
            &service,
            &store,
            &entries,
            &provider,
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
}
