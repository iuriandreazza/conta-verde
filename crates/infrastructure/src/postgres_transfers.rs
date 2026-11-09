use conta_verde_application::{
    AccountMovement, DateCursor, DayMovement, OpeningRepository, RepositoryError, TransferQuery,
    TransferRepository, ensure_audit_describes,
};
use conta_verde_domain::{
    AccountOpening, AuditEntity, AuditEntry, AuditRecord, BankAccountId, CalendarDate, CompanyId,
    Decimal, Transfer, TransferDraft, TransferId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::storage_error;
use crate::postgres_audit;

const SELECT_TRANSFERS: &str = "SELECT id, company_id, from_account_id, to_account_id, \
     amount::text AS amount, to_char(occurred_on, 'YYYY-MM-DD') AS occurred_on, note, reversed \
     FROM account_transfers";

pub struct PostgresTransferRepository {
    pool: PgPool,
}

impl PostgresTransferRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn transfer_from_row(row: &PgRow) -> Result<Transfer, RepositoryError> {
    let id = TransferId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let draft = TransferDraft {
        from: BankAccountId::from_uuid(row.try_get("from_account_id").map_err(storage_error)?),
        to: BankAccountId::from_uuid(row.try_get("to_account_id").map_err(storage_error)?),
        amount: row.try_get("amount").map_err(storage_error)?,
        occurred_on: row.try_get("occurred_on").map_err(storage_error)?,
        note: row.try_get("note").map_err(storage_error)?,
    };
    let reversed: bool = row.try_get("reversed").map_err(storage_error)?;
    Transfer::new(id, company_id, draft)
        .map(|transfer| transfer.with_reversed(reversed))
        .map_err(|error| {
            RepositoryError::Storage(format!("stored transfer {id:?} is invalid: {error}"))
        })
}

/// A sum of amounts read as text, like every amount.
fn decimal(text: &str) -> Result<Decimal, RepositoryError> {
    text.parse::<Decimal>()
        .map_err(|error| RepositoryError::Storage(format!("a sum that is not a number: {error}")))
}

impl TransferRepository for PostgresTransferRepository {
    async fn save(&self, transfer: &Transfer, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, transfer)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO account_transfers \
             (id, company_id, from_account_id, to_account_id, amount, occurred_on, note, reversed) \
             VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC), CAST($6 AS DATE), $7, false)",
        )
        .bind(transfer.id().as_uuid())
        .bind(transfer.company_id().as_uuid())
        .bind(transfer.from().as_uuid())
        .bind(transfer.to().as_uuid())
        .bind(transfer.amount().to_string())
        .bind(transfer.occurred_on().to_string())
        .bind(transfer.note())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn reverse(
        &self,
        reversed: &Transfer,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, reversed)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE account_transfers SET reversed = true \
             WHERE id = $1 AND company_id = $2 AND NOT reversed",
        )
        .bind(reversed.id().as_uuid())
        .bind(reversed.company_id().as_uuid())
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
        id: TransferId,
    ) -> Result<Option<Transfer>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_TRANSFERS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(transfer_from_row).transpose()
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &TransferQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> Result<Vec<Transfer>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_TRANSFERS);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(account) = query.account() {
            sql.push(" AND (from_account_id = ")
                .push_bind(account.as_uuid())
                .push(" OR to_account_id = ")
                .push_bind(account.as_uuid())
                .push(")");
        }
        if let Some((from, until)) = query.span() {
            sql.push(" AND occurred_on >= CAST(")
                .push_bind(from.to_string())
                .push(" AS DATE) AND occurred_on <= CAST(")
                .push_bind(until.to_string())
                .push(" AS DATE)");
        }
        if query.only_unreversed() {
            sql.push(" AND NOT reversed");
        }
        if let Some(cursor) = after {
            sql.push(" AND (occurred_on, id) < (CAST(")
                .push_bind(cursor.date.to_string())
                .push(" AS DATE), ")
                .push_bind(cursor.id)
                .push(")");
        }
        sql.push(" ORDER BY occurred_on DESC, id DESC LIMIT ")
            .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(transfer_from_row).collect()
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: TransferId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Transfer, id.as_uuid()).await
    }

    async fn transfer_candidates(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        incoming: bool,
        amount: Decimal,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<Transfer>, RepositoryError> {
        let side = if incoming {
            "to_account_id"
        } else {
            "from_account_id"
        };
        let rows = sqlx::query(&format!(
            "{SELECT_TRANSFERS} WHERE company_id = $1 AND NOT reversed AND {side} = $2 \
             AND amount = CAST($3 AS NUMERIC) \
             AND occurred_on BETWEEN CAST($4 AS DATE) AND CAST($5 AS DATE) \
             ORDER BY occurred_on, id"
        ))
        .bind(company_id.as_uuid())
        .bind(account.as_uuid())
        .bind(amount.to_string())
        .bind(since.to_string())
        .bind(until.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(transfer_from_row).collect()
    }

    async fn movement_by_day(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<DayMovement>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT to_char(occurred_on, 'YYYY-MM-DD') AS day, \
               COALESCE(SUM(amount) FILTER (WHERE to_account_id = $2), 0)::text AS received, \
               COALESCE(SUM(amount) FILTER (WHERE from_account_id = $2), 0)::text AS paid \
             FROM account_transfers \
             WHERE company_id = $1 AND NOT reversed \
               AND (from_account_id = $2 OR to_account_id = $2) \
               AND occurred_on BETWEEN CAST($3 AS DATE) AND CAST($4 AS DATE) \
             GROUP BY occurred_on ORDER BY occurred_on",
        )
        .bind(company_id.as_uuid())
        .bind(account.as_uuid())
        .bind(since.to_string())
        .bind(until.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter()
            .map(|row| {
                let day: String = row.try_get("day").map_err(storage_error)?;
                Ok(DayMovement {
                    date: CalendarDate::parse(&day).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored day {day} is invalid"))
                    })?,
                    received: decimal(
                        &row.try_get::<String, _>("received")
                            .map_err(storage_error)?,
                    )?,
                    paid: decimal(&row.try_get::<String, _>("paid").map_err(storage_error)?)?,
                })
            })
            .collect()
    }

    async fn movement_of_account(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: Option<CalendarDate>,
        until: CalendarDate,
    ) -> Result<AccountMovement, RepositoryError> {
        let row = sqlx::query(
            "SELECT \
               COALESCE(SUM(amount) FILTER (WHERE to_account_id = $2), 0)::text AS received, \
               COALESCE(SUM(amount) FILTER (WHERE from_account_id = $2), 0)::text AS paid \
             FROM account_transfers \
             WHERE company_id = $1 AND NOT reversed \
               AND (from_account_id = $2 OR to_account_id = $2) \
               AND occurred_on <= CAST($3 AS DATE) \
               AND (CAST($4 AS TEXT) IS NULL OR occurred_on >= CAST($4 AS DATE))",
        )
        .bind(company_id.as_uuid())
        .bind(account.as_uuid())
        .bind(until.to_string())
        .bind(since.map(|day| day.to_string()))
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(AccountMovement {
            received: decimal(
                &row.try_get::<String, _>("received")
                    .map_err(storage_error)?,
            )?,
            paid: decimal(&row.try_get::<String, _>("paid").map_err(storage_error)?)?,
        })
    }
}

pub struct PostgresOpeningRepository {
    pool: PgPool,
}

impl PostgresOpeningRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const SELECT_OPENINGS: &str = "SELECT account_id, company_id, balance::text AS balance, \
     to_char(on_date, 'YYYY-MM-DD') AS on_date FROM account_openings";

fn opening_from_row(row: &PgRow) -> Result<AccountOpening, RepositoryError> {
    let account: Uuid = row.try_get("account_id").map_err(storage_error)?;
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let balance: String = row.try_get("balance").map_err(storage_error)?;
    let on: String = row.try_get("on_date").map_err(storage_error)?;
    AccountOpening::new(company_id, BankAccountId::from_uuid(account), &balance, &on).map_err(
        |error| {
            RepositoryError::Storage(format!("stored opening of {account} is invalid: {error}"))
        },
    )
}

impl OpeningRepository for PostgresOpeningRepository {
    async fn set(
        &self,
        opening: &AccountOpening,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, opening)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO account_openings (account_id, company_id, balance, on_date) \
             VALUES ($1, $2, CAST($3 AS NUMERIC), CAST($4 AS DATE)) \
             ON CONFLICT (account_id) DO UPDATE \
             SET balance = EXCLUDED.balance, on_date = EXCLUDED.on_date \
             WHERE account_openings.company_id = EXCLUDED.company_id",
        )
        .bind(opening.account_id().as_uuid())
        .bind(opening.company_id().as_uuid())
        .bind(opening.balance().to_string())
        .bind(opening.on().to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
    ) -> Result<Option<AccountOpening>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_OPENINGS} WHERE company_id = $1 AND account_id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(account.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(opening_from_row).transpose()
    }

    async fn history(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::AccountOpening,
            account.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AccountOpening>, RepositoryError> {
        let rows = sqlx::query(&format!("{SELECT_OPENINGS} WHERE company_id = $1"))
            .bind(company_id.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(opening_from_row).collect()
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{BalanceService, FinancialEntryService, TransferService};

    use super::*;
    use crate::contract::{self, EntrySeed};
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresPartyRepository, PostgresPaymentMethodRepository,
        RegisterReferences,
    };

    async fn seed(pool: &PgPool, company: CompanyId) -> EntrySeed {
        contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await
    }

    fn references(
        pool: &PgPool,
    ) -> RegisterReferences<
        PostgresCategoryRepository,
        PostgresCostCenterRepository,
        PostgresPartyRepository,
        PostgresBankAccountRepository,
        PostgresPaymentMethodRepository,
        crate::PostgresPeriodLockRepository,
    > {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
            crate::PostgresPeriodLockRepository::new(pool.clone()),
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn moves_money_between_accounts() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let service = TransferService::new(
            PostgresTransferRepository::new(pool.clone()),
            references(&pool),
        );
        contract::moves_money_between_accounts(
            &service,
            &PostgresTransferRepository::new(pool),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn adds_balances_up() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let openings = PostgresOpeningRepository::new(pool.clone());
        let balances = BalanceService::new(
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            PostgresTransferRepository::new(pool.clone()),
            PostgresOpeningRepository::new(pool.clone()),
        );
        let entries = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        let transfers = TransferService::new(
            PostgresTransferRepository::new(pool.clone()),
            references(&pool),
        );
        contract::adds_balances_up(
            &balances,
            &entries,
            &transfers,
            &openings,
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn adds_the_cash_flow_up() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let balances = BalanceService::new(
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            PostgresTransferRepository::new(pool.clone()),
            PostgresOpeningRepository::new(pool.clone()),
        );
        let entries = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        let transfers = TransferService::new(
            PostgresTransferRepository::new(pool.clone()),
            references(&pool),
        );
        contract::adds_the_cash_flow_up(
            &balances,
            &entries,
            &transfers,
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_and_a_reference_to_another_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let other_seed = self::seed(&pool, other_company).await;
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let insert = |company: CompanyId,
                      from: Uuid,
                      to: Uuid,
                      amount: &'static str,
                      day: &'static str,
                      note: Option<&'static str>| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO account_transfers \
                     (id, company_id, from_account_id, to_account_id, amount, occurred_on, note) \
                     VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC), CAST($6 AS DATE), $7)",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(from)
                .bind(to)
                .bind(amount)
                .bind(day)
                .bind(note)
                .execute(&pool)
                .await
            }
        };
        let (from, to) = (
            seed.bank_account.as_uuid(),
            seed.other_bank_account.as_uuid(),
        );
        for (from, to, amount, day, note, constraint) in [
            (
                from,
                from,
                "1",
                "2026-12-10",
                None,
                "account_transfers_two_accounts",
            ),
            (
                from,
                to,
                "0",
                "2026-12-10",
                None,
                "account_transfers_amount_valid",
            ),
            (
                from,
                to,
                "1.001",
                "2026-12-10",
                None,
                "account_transfers_amount_valid",
            ),
            (
                from,
                to,
                "10000000000000",
                "2026-12-10",
                None,
                "account_transfers_amount_valid",
            ),
            (
                from,
                to,
                "1",
                "1899-12-31",
                None,
                "account_transfers_occurred_on_valid",
            ),
            (
                from,
                to,
                "1",
                "2026-12-10",
                Some(""),
                "account_transfers_note_valid",
            ),
            (
                from,
                to,
                "1",
                "2026-12-10",
                Some(" a"),
                "account_transfers_note_valid",
            ),
            (
                from,
                to,
                "1",
                "2026-12-10",
                Some("a\nb"),
                "account_transfers_note_valid",
            ),
            (
                other_seed.bank_account.as_uuid(),
                to,
                "1",
                "2026-12-10",
                None,
                "account_transfers_from_fk",
            ),
            (
                from,
                other_seed.bank_account.as_uuid(),
                "1",
                "2026-12-10",
                None,
                "account_transfers_to_fk",
            ),
        ] {
            assert_eq!(
                refused_by(insert(company, from, to, amount, day, note).await).as_deref(),
                Some(constraint),
                "{amount} {day} {note:?}"
            );
        }
        assert!(
            insert(company, from, to, "1.5", "2026-12-10", Some("Reforço"))
                .await
                .is_ok()
        );

        let opening =
            |company: CompanyId, account: Uuid, balance: &'static str, day: &'static str| {
                let pool = pool.clone();
                async move {
                    sqlx::query(
                        "INSERT INTO account_openings (account_id, company_id, balance, on_date) \
                     VALUES ($1, $2, CAST($3 AS NUMERIC), CAST($4 AS DATE))",
                    )
                    .bind(account)
                    .bind(company.as_uuid())
                    .bind(balance)
                    .bind(day)
                    .execute(&pool)
                    .await
                }
            };
        for (company, account, balance, day, constraint) in [
            (
                company,
                from,
                "1.001",
                "2026-12-01",
                "account_openings_balance_valid",
            ),
            (
                company,
                from,
                "10000000000000",
                "2026-12-01",
                "account_openings_balance_valid",
            ),
            (
                company,
                from,
                "1",
                "2201-01-01",
                "account_openings_on_date_valid",
            ),
            (
                other_company,
                from,
                "1",
                "2026-12-01",
                "account_openings_account_fk",
            ),
        ] {
            assert_eq!(
                refused_by(opening(company, account, balance, day).await).as_deref(),
                Some(constraint),
                "{balance} {day}"
            );
        }
        assert!(opening(company, from, "-20.5", "2026-12-01").await.is_ok());
        // One opening balance per account.
        assert_eq!(
            refused_by(opening(company, from, "1", "2026-12-02").await).as_deref(),
            Some("account_openings_pkey")
        );
    }
}
