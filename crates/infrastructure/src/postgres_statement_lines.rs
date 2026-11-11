use conta_verde_application::{
    DateCursor, MatchedTargets, RepositoryError, StatementLineQuery, StatementLineRepository,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, BankAccountId, CompanyId, LineMatch, LineStatus,
    SettlementId, StatementLine, StatementLineDraft, StatementLineId, TransferId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::storage_error;
use crate::postgres_audit;

const FINGERPRINT_INDEX: &str = "statement_lines_account_fingerprint_key";
const MATCH_INDEXES: [&str; 2] = [
    "statement_lines_settlement_key",
    "statement_lines_transfer_key",
];

const SELECT_LINES: &str = "SELECT id, company_id, account_id, \
     to_char(posted_on, 'YYYY-MM-DD') AS posted_on, amount::text AS amount, description, bank_ref, \
     fingerprint, status, matched_settlement_id, matched_transfer_id FROM statement_lines";

pub struct PostgresStatementLineRepository {
    pool: PgPool,
}

impl PostgresStatementLineRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn write_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(db) if db.constraint() == Some(FINGERPRINT_INDEX) => {
            RepositoryError::DuplicateLine
        }
        sqlx::Error::Database(db)
            if db
                .constraint()
                .is_some_and(|name| MATCH_INDEXES.contains(&name)) =>
        {
            RepositoryError::AlreadyMatched
        }
        _ => storage_error(error),
    }
}

fn targets(line: &StatementLine) -> (Option<Uuid>, Option<Uuid>) {
    match line.matched() {
        Some(LineMatch::Settlement(id)) => (Some(id.as_uuid()), None),
        Some(LineMatch::Transfer(id)) => (None, Some(id.as_uuid())),
        None => (None, None),
    }
}

impl StatementLineRepository for PostgresStatementLineRepository {
    async fn save(&self, line: &StatementLine, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, line)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let (settlement, transfer) = targets(line);
        sqlx::query(
            "INSERT INTO statement_lines \
             (id, company_id, account_id, posted_on, amount, description, bank_ref, fingerprint, status, \
              matched_settlement_id, matched_transfer_id) \
             VALUES ($1, $2, $3, CAST($4 AS DATE), CAST($5 AS NUMERIC), $6, $7, $8, $9, $10, $11)",
        )
        .bind(line.id().as_uuid())
        .bind(line.company_id().as_uuid())
        .bind(line.account_id().as_uuid())
        .bind(line.posted_on().to_string())
        .bind(line.amount().to_string())
        .bind(line.description())
        .bind(line.bank_ref())
        .bind(line.fingerprint())
        .bind(line.status().as_str())
        .bind(settlement)
        .bind(transfer)
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> Result<Option<StatementLine>, RepositoryError> {
        let row = sqlx::query(&format!("{SELECT_LINES} WHERE company_id = $1 AND id = $2"))
            .bind(company_id.as_uuid())
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        row.as_ref().map(line_from_row).transpose()
    }

    async fn update(
        &self,
        line: &StatementLine,
        was: LineStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, line)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let (settlement, transfer) = targets(line);
        let result = sqlx::query(
            "UPDATE statement_lines SET status = $3, matched_settlement_id = $4, matched_transfer_id = $5 \
             WHERE id = $1 AND company_id = $2 AND status = $6",
        )
        .bind(line.id().as_uuid())
        .bind(line.company_id().as_uuid())
        .bind(line.status().as_str())
        .bind(settlement)
        .bind(transfer)
        .bind(was.as_str())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        if result.rows_affected() == 0 {
            // The line is not there, or someone moved it since it was read.
            let there: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM statement_lines WHERE id = $1 AND company_id = $2",
            )
            .bind(line.id().as_uuid())
            .bind(line.company_id().as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage_error)?;
            return Err(if there.is_some() {
                RepositoryError::LineChanged
            } else {
                RepositoryError::NotFound
            });
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::StatementLine,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &StatementLineQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> Result<Vec<StatementLine>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_LINES);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(account) = query.account() {
            sql.push(" AND account_id = ").push_bind(account.as_uuid());
        }
        if let Some(status) = query.status() {
            sql.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(cursor) = after {
            sql.push(" AND (posted_on, id) < (CAST(")
                .push_bind(cursor.date.to_string())
                .push(" AS DATE), ")
                .push_bind(cursor.id)
                .push(")");
        }
        // The table's own column: the SELECT outputs `posted_on` as text and an unqualified ORDER BY would sort by that (ADR 0097).
        sql.push(" ORDER BY statement_lines.posted_on DESC, id DESC LIMIT ")
            .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(line_from_row).collect()
    }

    async fn matched_among(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        settlements: &[SettlementId],
        transfers: &[TransferId],
    ) -> Result<MatchedTargets, RepositoryError> {
        let settlement_ids: Vec<Uuid> = settlements.iter().map(|id| id.as_uuid()).collect();
        let transfer_ids: Vec<Uuid> = transfers.iter().map(|id| id.as_uuid()).collect();
        let rows = sqlx::query(
            "SELECT matched_settlement_id, matched_transfer_id FROM statement_lines \
             WHERE company_id = $1 AND (matched_settlement_id = ANY($2) \
                 OR (matched_transfer_id = ANY($3) AND account_id = $4))",
        )
        .bind(company_id.as_uuid())
        .bind(&settlement_ids)
        .bind(&transfer_ids)
        .bind(account.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let mut found = MatchedTargets::default();
        for row in rows {
            let settlement: Option<Uuid> = row
                .try_get("matched_settlement_id")
                .map_err(storage_error)?;
            let transfer: Option<Uuid> =
                row.try_get("matched_transfer_id").map_err(storage_error)?;
            found
                .settlements
                .extend(settlement.map(SettlementId::from_uuid));
            found.transfers.extend(transfer.map(TransferId::from_uuid));
        }
        Ok(found)
    }
}

fn line_from_row(row: &PgRow) -> Result<StatementLine, RepositoryError> {
    let id = StatementLineId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let account = BankAccountId::from_uuid(row.try_get("account_id").map_err(storage_error)?);
    let draft = StatementLineDraft {
        posted_on: row.try_get("posted_on").map_err(storage_error)?,
        amount: row.try_get("amount").map_err(storage_error)?,
        description: row.try_get("description").map_err(storage_error)?,
        bank_ref: row.try_get("bank_ref").map_err(storage_error)?,
    };
    let status: String = row.try_get("status").map_err(storage_error)?;
    let status = LineStatus::parse(&status)
        .ok_or_else(|| RepositoryError::Storage(format!("stored line with status {status:?}")))?;
    let settlement: Option<Uuid> = row
        .try_get("matched_settlement_id")
        .map_err(storage_error)?;
    let transfer: Option<Uuid> = row.try_get("matched_transfer_id").map_err(storage_error)?;
    let matched = settlement
        .map(|id| LineMatch::Settlement(SettlementId::from_uuid(id)))
        .or(transfer.map(|id| LineMatch::Transfer(TransferId::from_uuid(id))));
    let fingerprint: String = row.try_get("fingerprint").map_err(storage_error)?;
    let line = StatementLine::new(id, company_id, account, draft, 0).map_err(|error| {
        RepositoryError::Storage(format!("stored statement line {id:?} is invalid: {error}"))
    })?;
    Ok(line.with_stored_state(fingerprint, status, matched))
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{FinancialEntryService, ReconciliationService, TransferService};

    use super::*;
    use crate::contract::{self, EntrySeed};
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresPartyRepository, PostgresPaymentMethodRepository,
        PostgresTransferRepository, RegisterReferences,
    };

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

    async fn seed_for(pool: &PgPool, company: CompanyId) -> EntrySeed {
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

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn reconciles_statement_lines() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed_for(&pool, company).await;
        let service = ReconciliationService::new(
            PostgresStatementLineRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            ),
            TransferService::new(
                PostgresTransferRepository::new(pool.clone()),
                references(&pool),
            ),
            references(&pool),
        );
        contract::reconciles_statement_lines(
            &service,
            &PostgresStatementLineRepository::new(pool.clone()),
            &FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            ),
            &TransferService::new(
                PostgresTransferRepository::new(pool.clone()),
                references(&pool),
            ),
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
        let seed = seed_for(&pool, company).await;
        let other_seed = seed_for(&pool, other_company).await;
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let insert = |company: CompanyId,
                      account: Uuid,
                      amount: &'static str,
                      description: &'static str,
                      fingerprint: &'static str,
                      status: &'static str,
                      settlement: Option<Uuid>| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO statement_lines (id, company_id, account_id, posted_on, amount, description, \
                     fingerprint, status, matched_settlement_id) \
                     VALUES ($1, $2, $3, DATE '2026-11-05', CAST($4 AS NUMERIC), $5, $6, $7, $8)",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(account)
                .bind(amount)
                .bind(description)
                .bind(fingerprint)
                .bind(status)
                .bind(settlement)
                .execute(&pool)
                .await
            }
        };
        let account = seed.bank_account.as_uuid();
        for (amount, description, status, settlement, constraint) in [
            ("0", "X", "pending", None, "statement_lines_amount_valid"),
            (
                "1.005",
                "X",
                "pending",
                None,
                "statement_lines_amount_valid",
            ),
            (
                "10000000000000",
                "X",
                "pending",
                None,
                "statement_lines_amount_valid",
            ),
            (
                "1",
                "",
                "pending",
                None,
                "statement_lines_description_valid",
            ),
            (
                "1",
                " X",
                "pending",
                None,
                "statement_lines_description_valid",
            ),
            ("1", "X", "done", None, "statement_lines_status_valid"),
            ("1", "X", "matched", None, "statement_lines_match_whole"),
            (
                "1",
                "X",
                "pending",
                Some(Uuid::new_v4()),
                "statement_lines_match_whole",
            ),
            (
                "1",
                "X",
                "matched",
                Some(Uuid::new_v4()),
                "statement_lines_settlement_fk",
            ),
        ] {
            assert_eq!(
                refused_by(
                    insert(
                        company,
                        account,
                        amount,
                        description,
                        "f",
                        status,
                        settlement
                    )
                    .await
                )
                .as_deref(),
                Some(constraint),
                "{amount} {description:?} {status}"
            );
        }
        assert_eq!(
            refused_by(
                insert(
                    company,
                    other_seed.bank_account.as_uuid(),
                    "1",
                    "X",
                    "f",
                    "pending",
                    None
                )
                .await
            )
            .as_deref(),
            Some("statement_lines_account_fk")
        );
        assert!(
            insert(company, account, "-1", "X", "same", "pending", None)
                .await
                .is_ok()
        );
        assert_eq!(
            refused_by(insert(company, account, "-1", "X", "same", "pending", None).await)
                .as_deref(),
            Some("statement_lines_account_fingerprint_key")
        );
        // The same fingerprint is another line in another account.
        assert!(
            insert(
                company,
                seed.other_bank_account.as_uuid(),
                "-1",
                "X",
                "same",
                "pending",
                None
            )
            .await
            .is_ok()
        );
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn syncs_bank_connections() {
        use conta_verde_application::BankSyncService;

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed_for(&pool, company).await;
        let reconciliation = || {
            ReconciliationService::new(
                PostgresStatementLineRepository::new(pool.clone()),
                FinancialEntryService::new(
                    PostgresFinancialEntryRepository::new(pool.clone()),
                    references(&pool),
                ),
                TransferService::new(
                    PostgresTransferRepository::new(pool.clone()),
                    references(&pool),
                ),
                references(&pool),
            )
        };
        let aggregator =
            crate::FakeBankAggregator::default().with_account("item-1", "acc-1", "Conta");
        let sync = BankSyncService::new(
            crate::PostgresBankConnectionRepository::new(pool.clone()),
            aggregator.clone(),
            reconciliation(),
            crate::PostgresJobQueue::new(pool.clone()),
        );
        crate::contract_integrations::syncs_bank_connections(
            &sync,
            &aggregator,
            &reconciliation(),
            company,
            other_company,
            seed.bank_account,
            seed.other_bank_account,
        )
        .await;

        let third = contract::create_company(&pool).await;
        let third_seed = seed_for(&pool, third).await;
        crate::contract_integrations::an_ended_connection_stays_ended(
            &crate::PostgresBankConnectionRepository::new(pool.clone()),
            third,
            third_seed.bank_account,
        )
        .await;
    }
}
