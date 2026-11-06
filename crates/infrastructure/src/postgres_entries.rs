use std::collections::HashMap;

use conta_verde_application::{
    AccountMovement, EntryCursor, EntrySort, FinancialEntryQuery, FinancialEntryRepository,
    RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    Amount, AuditEntity, AuditEntry, AuditRecord, BankAccountId, CalendarDate, CategoryId,
    CompanyId, CostCenterId, Decimal, EntryId, EntryInstallmentDraft, EntryKind, FinancialEntry,
    FinancialEntryDraft, PartyId, PaymentMethodId, Settlement, SettlementDraft, SettlementId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{snapshot, storage_error};
use crate::postgres_audit;

const SELECT_ENTRIES: &str = "SELECT id, company_id, kind, description, party_id, category_id, \
     cost_center_id, to_char(competence_date, 'YYYY-MM-DD') AS competence_date \
     FROM financial_entries";

pub struct PostgresFinancialEntryRepository {
    pool: PgPool,
}

impl PostgresFinancialEntryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Writes the installments of an entry, in order, on the connection of the transaction that writes
/// the entry: they commit with it or not at all.
async fn insert_installments(
    connection: &mut PgConnection,
    entry: &FinancialEntry,
) -> Result<(), RepositoryError> {
    for installment in entry.installments() {
        sqlx::query(
            "INSERT INTO financial_entry_installments (company_id, entry_id, number, due_date, amount) \
             VALUES ($1, $2, $3, CAST($4 AS DATE), CAST($5 AS NUMERIC))",
        )
        .bind(entry.company_id().as_uuid())
        .bind(entry.id().as_uuid())
        .bind(i16::try_from(installment.number()).map_err(|_| {
            RepositoryError::Storage("an entry has too many installments".to_owned())
        })?)
        .bind(installment.due_date().to_string())
        .bind(installment.amount().to_string())
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
    }
    Ok(())
}

impl FinancialEntryRepository for PostgresFinancialEntryRepository {
    async fn save(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, entry)?;
        // One transaction: if the entry is refused (a reference to another company) or the audit row
        // cannot be written, the rollback leaves neither the entry nor its installments behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO financial_entries \
             (id, company_id, kind, description, party_id, category_id, cost_center_id, competence_date) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS DATE))",
        )
        .bind(entry.id().as_uuid())
        .bind(entry.company_id().as_uuid())
        .bind(entry.kind().as_str())
        .bind(entry.description())
        .bind(entry.party_id().map(|id| id.as_uuid()))
        .bind(entry.category_id().as_uuid())
        .bind(entry.cost_center_id().map(|id| id.as_uuid()))
        .bind(entry.competence_date().to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        insert_installments(&mut tx, entry).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<Option<FinancialEntry>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_ENTRIES} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut entries = with_installments(&mut tx, company_id, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(entries.pop())
    }

    async fn update(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, entry)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The entry row is locked for the check and the write, so a settlement cannot slip in
        // between: it takes the same lock before it looks at the installments.
        let stored_kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM financial_entries WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(entry.id().as_uuid())
        .bind(entry.company_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(stored_kind) = stored_kind else {
            return Err(RepositoryError::NotFound);
        };
        refuse_to_change_what_was_settled(&mut tx, entry, &stored_kind).await?;
        let result = sqlx::query(
            "UPDATE financial_entries SET kind = $3, description = $4, party_id = $5, \
             category_id = $6, cost_center_id = $7, competence_date = CAST($8 AS DATE) \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(entry.id().as_uuid())
        .bind(entry.company_id().as_uuid())
        .bind(entry.kind().as_str())
        .bind(entry.description())
        .bind(entry.party_id().map(|id| id.as_uuid()))
        .bind(entry.category_id().as_uuid())
        .bind(entry.cost_center_id().map(|id| id.as_uuid()))
        .bind(entry.competence_date().to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        // Matching no row means the entry is not in that company: report it, do not pretend. The
        // transaction is dropped, so the installments it was about to replace are untouched.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        // The installments are replaced as a whole, inside the same transaction as the entry and
        // its audit entry: a refusal anywhere leaves the old ones in place.
        sqlx::query(
            "DELETE FROM financial_entry_installments WHERE entry_id = $1 AND company_id = $2",
        )
        .bind(entry.id().as_uuid())
        .bind(entry.company_id().as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        insert_installments(&mut tx, entry).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn settle(
        &self,
        settlement: &Settlement,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, settlement)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The entry row is locked first, like an edit does, and then the installment: two
        // settlements of the same installment, or one and an edit, take turns, and what is left of the
        // installment is read after the one before has committed.
        let entry_exists: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM financial_entries WHERE id = $1 AND company_id = $2 FOR SHARE",
        )
        .bind(settlement.entry_id().as_uuid())
        .bind(settlement.company_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        if entry_exists.is_none() {
            return Err(RepositoryError::NotFound);
        }
        // The installment row is locked in a statement of its own, and the balance is read in the next
        // one: in READ COMMITTED each statement sees what was committed when it starts, so a
        // settlement that held the lock and committed is counted. Reading the balance in the same
        // statement as the lock would use the snapshot from before the wait.
        let (company, entry, number) = (
            settlement.company_id().as_uuid(),
            settlement.entry_id().as_uuid(),
            i16::try_from(settlement.installment()).map_err(|_| RepositoryError::NotFound)?,
        );
        let locked: Option<Uuid> = sqlx::query_scalar(
            "SELECT entry_id FROM financial_entry_installments \
             WHERE company_id = $1 AND entry_id = $2 AND number = $3 FOR UPDATE",
        )
        .bind(company)
        .bind(entry)
        .bind(number)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        if locked.is_none() {
            return Err(RepositoryError::NotFound);
        }
        let fits: bool = sqlx::query_scalar(
            "SELECT i.amount - COALESCE(( \
                 SELECT SUM(s.applied) FROM entry_settlements s \
                 WHERE s.company_id = i.company_id AND s.entry_id = i.entry_id \
                   AND s.installment = i.number AND NOT s.reversed), 0) >= CAST($4 AS NUMERIC) \
             FROM financial_entry_installments i \
             WHERE i.company_id = $1 AND i.entry_id = $2 AND i.number = $3",
        )
        .bind(company)
        .bind(entry)
        .bind(number)
        .bind(settlement.applied().to_string())
        .fetch_one(&mut *tx)
        .await
        .map_err(storage_error)?;
        if !fits {
            return Err(RepositoryError::Overpaid);
        }
        sqlx::query(
            "INSERT INTO entry_settlements \
             (id, company_id, entry_id, installment, settled_on, applied, interest, fine, discount, \
              bank_account_id, payment_method_id, reversed) \
             VALUES ($1, $2, $3, $4, CAST($5 AS DATE), CAST($6 AS NUMERIC), CAST($7 AS NUMERIC), \
                     CAST($8 AS NUMERIC), CAST($9 AS NUMERIC), $10, $11, false)",
        )
        .bind(settlement.id().as_uuid())
        .bind(settlement.company_id().as_uuid())
        .bind(settlement.entry_id().as_uuid())
        .bind(i16::try_from(settlement.installment()).map_err(|_| RepositoryError::NotFound)?)
        .bind(settlement.settled_on().to_string())
        .bind(settlement.applied().to_string())
        .bind(settlement.interest().to_string())
        .bind(settlement.fine().to_string())
        .bind(settlement.discount().to_string())
        .bind(settlement.bank_account_id().as_uuid())
        .bind(settlement.payment_method_id().map(|id| id.as_uuid()))
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn reverse(
        &self,
        reversed: &Settlement,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, reversed)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE entry_settlements SET reversed = true \
             WHERE id = $1 AND company_id = $2 AND entry_id = $3 AND NOT reversed",
        )
        .bind(reversed.id().as_uuid())
        .bind(reversed.company_id().as_uuid())
        .bind(reversed.entry_id().as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn settlement(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: SettlementId,
    ) -> Result<Option<Settlement>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_SETTLEMENTS} WHERE company_id = $1 AND entry_id = $2 AND id = $3"
        ))
        .bind(company_id.as_uuid())
        .bind(entry_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(settlement_from_row).transpose()
    }

    async fn settlements_of(
        &self,
        company_id: CompanyId,
        entry_ids: &[EntryId],
    ) -> Result<Vec<Settlement>, RepositoryError> {
        let ids: Vec<Uuid> = entry_ids.iter().map(|id| id.as_uuid()).collect();
        let rows = sqlx::query(&format!(
            "{SELECT_SETTLEMENTS} WHERE company_id = $1 AND entry_id = ANY($2) \
             ORDER BY settled_on, id"
        ))
        .bind(company_id.as_uuid())
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(settlement_from_row).collect()
    }

    async fn cash_of_account(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: Option<CalendarDate>,
        until: CalendarDate,
    ) -> Result<AccountMovement, RepositoryError> {
        // The cash of a settlement is added up here and not stored, like everywhere else: the
        // applied amount, plus interest and fine, less the discount.
        let row = sqlx::query(
            "SELECT \
               COALESCE(SUM(s.applied + s.interest + s.fine - s.discount) \
                   FILTER (WHERE e.kind = 'receivable'), 0)::text AS received, \
               COALESCE(SUM(s.applied + s.interest + s.fine - s.discount) \
                   FILTER (WHERE e.kind = 'payable'), 0)::text AS paid \
             FROM entry_settlements s \
             JOIN financial_entries e ON e.company_id = s.company_id AND e.id = s.entry_id \
             WHERE s.company_id = $1 AND s.bank_account_id = $2 AND NOT s.reversed \
               AND s.settled_on <= CAST($3 AS DATE) \
               AND (CAST($4 AS TEXT) IS NULL OR s.settled_on >= CAST($4 AS DATE))",
        )
        .bind(company_id.as_uuid())
        .bind(account.as_uuid())
        .bind(until.to_string())
        .bind(since.map(|day| day.to_string()))
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        let sum = |column: &str| -> Result<Decimal, RepositoryError> {
            row.try_get::<String, _>(column)
                .map_err(storage_error)?
                .parse::<Decimal>()
                .map_err(|error| {
                    RepositoryError::Storage(format!("a sum that is not a number: {error}"))
                })
        };
        Ok(AccountMovement {
            received: sum("received")?,
            paid: sum("paid")?,
        })
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::FinancialEntry,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &FinancialEntryQuery,
        after: Option<&EntryCursor>,
        limit: usize,
    ) -> Result<Vec<FinancialEntry>, RepositoryError> {
        // Every value is bound, never spliced into the text: the pieces pushed as text are fixed.
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_ENTRIES);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(kind) = query.kind() {
            sql.push(" AND kind = ").push_bind(kind.as_str());
        }
        if let Some(category) = query.category() {
            sql.push(" AND category_id = ")
                .push_bind(category.as_uuid());
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
            sql.push(" AND strpos(lower(description), lower(")
                .push_bind(text)
                .push(")) > 0");
        }
        let (comparison, direction) = match query.sort() {
            EntrySort::CompetenceAsc => (">", "ASC"),
            EntrySort::CompetenceDesc => ("<", "DESC"),
        };
        if let Some(cursor) = after {
            sql.push(format!(" AND (competence_date, id) {comparison} (CAST("))
                .push_bind(cursor.date.to_string())
                .push(" AS DATE), ")
                .push_bind(cursor.id)
                .push(")");
        }
        sql.push(format!(
            " ORDER BY competence_date {direction}, id {direction} LIMIT "
        ))
        .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));

        let mut tx = snapshot(&self.pool).await?;
        let rows = sql
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_error)?;
        let entries = with_installments(&mut tx, company_id, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(entries)
    }
}

const SELECT_SETTLEMENTS: &str = "SELECT id, company_id, entry_id, installment, \
     to_char(settled_on, 'YYYY-MM-DD') AS settled_on, applied::text AS applied, \
     interest::text AS interest, fine::text AS fine, discount::text AS discount, \
     bank_account_id, payment_method_id, reversed FROM entry_settlements";

fn settlement_from_row(row: &PgRow) -> Result<Settlement, RepositoryError> {
    let id = SettlementId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let installment: i16 = row.try_get("installment").map_err(storage_error)?;
    let method: Option<Uuid> = row.try_get("payment_method_id").map_err(storage_error)?;
    let draft = SettlementDraft {
        installment: u16::try_from(installment).map_err(|_| {
            RepositoryError::Storage(format!("stored settlement of installment {installment}"))
        })?,
        settled_on: row.try_get("settled_on").map_err(storage_error)?,
        applied: row.try_get("applied").map_err(storage_error)?,
        interest: Some(row.try_get("interest").map_err(storage_error)?),
        fine: Some(row.try_get("fine").map_err(storage_error)?),
        discount: Some(row.try_get("discount").map_err(storage_error)?),
        bank_account_id: BankAccountId::from_uuid(
            row.try_get("bank_account_id").map_err(storage_error)?,
        ),
        payment_method_id: method.map(PaymentMethodId::from_uuid),
    };
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let entry_id = EntryId::from_uuid(row.try_get("entry_id").map_err(storage_error)?);
    let reversed: bool = row.try_get("reversed").map_err(storage_error)?;
    Settlement::new(id, company_id, entry_id, draft)
        .map(|settlement| settlement.with_reversed(reversed))
        .map_err(|error| {
            RepositoryError::Storage(format!("stored settlement {id:?} is invalid: {error}"))
        })
}

/// An entry with settlements that are not reversed keeps its installments and its kind: the balances
/// and the bank movements were made against them ([ADR 0030]). Called with the entry row locked.
async fn refuse_to_change_what_was_settled(
    connection: &mut PgConnection,
    entry: &FinancialEntry,
    stored_kind: &str,
) -> Result<(), RepositoryError> {
    let settled: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM entry_settlements \
         WHERE company_id = $1 AND entry_id = $2 AND NOT reversed)",
    )
    .bind(entry.company_id().as_uuid())
    .bind(entry.id().as_uuid())
    .fetch_one(&mut *connection)
    .await
    .map_err(storage_error)?;
    if !settled {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT to_char(due_date, 'YYYY-MM-DD') AS due_date, amount::text AS amount \
         FROM financial_entry_installments WHERE company_id = $1 AND entry_id = $2 ORDER BY number",
    )
    .bind(entry.company_id().as_uuid())
    .bind(entry.id().as_uuid())
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let stored: Vec<(String, String)> = rows
        .iter()
        .map(|row| {
            let due: String = row.try_get("due_date").map_err(storage_error)?;
            let amount: String = row.try_get("amount").map_err(storage_error)?;
            // The same text the domain writes, whatever trailing zeros the database kept.
            let amount = Amount::parse(&amount).map_or(amount, |parsed| parsed.to_string());
            Ok((due, amount))
        })
        .collect::<Result<_, RepositoryError>>()?;
    let wanted: Vec<(String, String)> = entry
        .installments()
        .iter()
        .map(|item| (item.due_date().to_string(), item.amount().to_string()))
        .collect();
    if stored_kind != entry.kind().as_str() || stored != wanted {
        return Err(RepositoryError::EntrySettled);
    }
    Ok(())
}

/// The entries of these rows, each with its installments in order: one query for all of them.
async fn with_installments(
    connection: &mut PgConnection,
    company_id: CompanyId,
    rows: Vec<PgRow>,
) -> Result<Vec<FinancialEntry>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let installment_rows = sqlx::query(
        "SELECT entry_id, to_char(due_date, 'YYYY-MM-DD') AS due_date, amount::text AS amount \
         FROM financial_entry_installments \
         WHERE company_id = $1 AND entry_id = ANY($2) ORDER BY entry_id, number",
    )
    .bind(company_id.as_uuid())
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut by_entry: HashMap<Uuid, Vec<EntryInstallmentDraft>> = HashMap::new();
    for row in &installment_rows {
        let entry_id: Uuid = row.try_get("entry_id").map_err(storage_error)?;
        by_entry
            .entry(entry_id)
            .or_default()
            .push(EntryInstallmentDraft {
                due_date: row.try_get("due_date").map_err(storage_error)?,
                amount: row.try_get("amount").map_err(storage_error)?,
            });
    }
    rows.iter()
        .map(|row| entry_from_row(row, &mut by_entry))
        .collect()
}

fn entry_from_row(
    row: &PgRow,
    installments: &mut HashMap<Uuid, Vec<EntryInstallmentDraft>>,
) -> Result<FinancialEntry, RepositoryError> {
    let raw_id: Uuid = row.try_get("id").map_err(storage_error)?;
    let id = EntryId::from_uuid(raw_id);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let kind = EntryKind::parse(&kind)
        .ok_or_else(|| RepositoryError::Storage(format!("stored entry with kind {kind:?}")))?;
    let party: Option<Uuid> = row.try_get("party_id").map_err(storage_error)?;
    let cost_center: Option<Uuid> = row.try_get("cost_center_id").map_err(storage_error)?;
    let draft = FinancialEntryDraft {
        kind,
        description: row.try_get("description").map_err(storage_error)?,
        party_id: party.map(PartyId::from_uuid),
        category_id: CategoryId::from_uuid(row.try_get("category_id").map_err(storage_error)?),
        cost_center_id: cost_center.map(CostCenterId::from_uuid),
        competence_date: row.try_get("competence_date").map_err(storage_error)?,
        installments: installments.remove(&raw_id).unwrap_or_default(),
    };
    FinancialEntry::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored entry {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::FinancialEntryService;

    use super::*;
    use crate::contract::{self, EntrySeed};
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresPartyRepository, PostgresPaymentMethodRepository, RegisterReferences,
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
    > {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_registers_say_what_an_entry_may_point_at() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        contract::entry_references_say_what_is_usable(
            &references(&pool),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_entries_with_their_trail() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        contract::stores_entries_with_their_trail(
            &PostgresFinancialEntryRepository::new(pool),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn edits_entries_and_records_only_changes() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let service = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        contract::edits_entries_and_records_only_changes(&service, company, other_company, &seed)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn an_edit_may_keep_what_was_deactivated_since() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let service = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        contract::an_edit_may_keep_what_was_deactivated_since(
            &service,
            &PostgresCategoryRepository::new(pool.clone()),
            company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn settles_installments_and_reverses_them() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let service = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        contract::settles_installments_and_reverses_them(&service, company, other_company, &seed)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn two_settlements_at_the_same_moment_cannot_both_fit_and_each_leaves_its_trail() {
        use conta_verde_application::ApplicationError;
        use conta_verde_domain::{
            EntryInstallmentDraft, EntryKind, FinancialEntryDraft, SettlementDraft,
        };

        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let service = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        let actor = contract::actor();
        let draft = FinancialEntryDraft {
            kind: EntryKind::Receivable,
            description: "Venda".into(),
            party_id: None,
            category_id: seed.category,
            cost_center_id: None,
            competence_date: "2026-11-05".into(),
            installments: vec![EntryInstallmentDraft {
                due_date: "2026-12-10".into(),
                amount: "100".into(),
            }],
        };
        let sixty = || SettlementDraft {
            installment: 1,
            settled_on: "2026-12-10".into(),
            applied: "60".into(),
            interest: None,
            fine: None,
            discount: None,
            bank_account_id: seed.bank_account,
            payment_method_id: None,
        };
        // Several rounds, each on its own entry: the lock has to hold every time, not by luck.
        for _ in 0..8 {
            let entry = service
                .register(company, &actor, draft.clone())
                .await
                .unwrap();
            let (first, second) = tokio::join!(
                service.settle(company, &actor, entry.id(), sixty()),
                service.settle(company, &actor, entry.id(), sixty()),
            );
            let results = [first, second];
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
            let refused = results
                .iter()
                .find_map(|result| result.as_ref().err())
                .unwrap();
            assert_eq!(refused, &ApplicationError::SettlementExceedsBalance);
            let stored = service.settlements(company, entry.id()).await.unwrap();
            assert_eq!(stored.len(), 1, "the refused one left nothing behind");

            // The settlement and its reversal are in the trail, in the write that made them; the
            // refused one is not.
            let trail = |id: uuid::Uuid| {
                let pool = pool.clone();
                async move {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT count(*) FROM audit_log WHERE entity = 'settlement' AND entity_id = $1",
                    )
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
                }
            };
            let id = stored[0].id();
            assert_eq!(trail(id.as_uuid()).await, 1);
            service
                .reverse(company, &actor, entry.id(), id)
                .await
                .unwrap();
            assert_eq!(trail(id.as_uuid()).await, 2);
            let total: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM audit_log WHERE entity = 'settlement' AND company_id = $1",
            )
            .bind(company.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(total >= 2);
        }
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_a_settlement_row_can_show() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let other_seed = self::seed(&pool, other_company).await;
        let entry_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO financial_entries (id, company_id, kind, description, category_id, competence_date) \
             VALUES ($1, $2, 'payable', 'Base', $3, DATE '2026-01-01')",
        )
        .bind(entry_id)
        .bind(company.as_uuid())
        .bind(seed.category.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let insert = |company: CompanyId,
                      entry: Uuid,
                      installment: i16,
                      date: &'static str,
                      amounts: [&'static str; 4],
                      account: Uuid,
                      method: Option<Uuid>| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO entry_settlements \
                     (id, company_id, entry_id, installment, settled_on, applied, interest, fine, discount, \
                      bank_account_id, payment_method_id) \
                     VALUES ($1, $2, $3, $4, CAST($5 AS DATE), CAST($6 AS NUMERIC), CAST($7 AS NUMERIC), \
                             CAST($8 AS NUMERIC), CAST($9 AS NUMERIC), $10, $11)",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(entry)
                .bind(installment)
                .bind(date)
                .bind(amounts[0])
                .bind(amounts[1])
                .bind(amounts[2])
                .bind(amounts[3])
                .bind(account)
                .bind(method)
                .execute(&pool)
                .await
            }
        };
        let account = seed.bank_account.as_uuid();
        let fine = |installment: i16, date: &'static str, amounts: [&'static str; 4]| {
            insert(company, entry_id, installment, date, amounts, account, None)
        };
        for (installment, date, amounts, constraint) in [
            (
                0,
                "2026-12-10",
                ["1", "0", "0", "0"],
                "entry_settlements_installment_valid",
            ),
            (
                121,
                "2026-12-10",
                ["1", "0", "0", "0"],
                "entry_settlements_installment_valid",
            ),
            (
                1,
                "1899-12-31",
                ["1", "0", "0", "0"],
                "entry_settlements_settled_on_valid",
            ),
            (
                1,
                "2026-12-10",
                ["0", "0", "0", "0"],
                "entry_settlements_applied_valid",
            ),
            (
                1,
                "2026-12-10",
                ["1.005", "0", "0", "0"],
                "entry_settlements_applied_valid",
            ),
            (
                1,
                "2026-12-10",
                ["1", "-1", "0", "0"],
                "entry_settlements_adjustments_valid",
            ),
            (
                1,
                "2026-12-10",
                ["1", "0", "0.001", "0"],
                "entry_settlements_adjustments_valid",
            ),
            (
                1,
                "2026-12-10",
                ["1", "0", "0", "1"],
                "entry_settlements_moves_money",
            ),
            (
                1,
                "2026-12-10",
                ["1", "0", "0", "2"],
                "entry_settlements_moves_money",
            ),
        ] {
            assert_eq!(
                refused_by(fine(installment, date, amounts).await).as_deref(),
                Some(constraint),
                "{installment} {date} {amounts:?}"
            );
        }
        assert!(fine(1, "2026-12-10", ["1", "0", "0", "0.99"]).await.is_ok());

        // An account, a method or an entry of another company is refused, and so is one that is not there.
        assert_eq!(
            refused_by(
                insert(
                    company,
                    entry_id,
                    1,
                    "2026-12-10",
                    ["1", "0", "0", "0"],
                    other_seed.bank_account.as_uuid(),
                    None
                )
                .await
            )
            .as_deref(),
            Some("entry_settlements_bank_account_fk")
        );
        assert_eq!(
            refused_by(
                insert(
                    company,
                    entry_id,
                    1,
                    "2026-12-10",
                    ["1", "0", "0", "0"],
                    account,
                    Some(other_seed.payment_method.as_uuid())
                )
                .await
            )
            .as_deref(),
            Some("entry_settlements_payment_method_fk")
        );
        assert_eq!(
            refused_by(
                insert(
                    other_company,
                    entry_id,
                    1,
                    "2026-12-10",
                    ["1", "0", "0", "0"],
                    other_seed.bank_account.as_uuid(),
                    None
                )
                .await
            )
            .as_deref(),
            Some("entry_settlements_entry_fk")
        );
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn filters_and_sorts_entries() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let other_seed = self::seed(&pool, other_company).await;
        let service = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        contract::filters_and_sorts_entries(&service, company, other_company, &other_seed, &seed)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_and_a_reference_to_another_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = seed(&pool, company).await;
        let category = seed.category.as_uuid();
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let insert = |company: CompanyId,
                      kind: &'static str,
                      description: String,
                      category: Uuid,
                      cost_center: Option<Uuid>,
                      date: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO financial_entries \
                     (id, company_id, kind, description, category_id, cost_center_id, competence_date) \
                     VALUES ($1, $2, $3, $4, $5, $6, CAST($7 AS DATE))",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(kind)
                .bind(description)
                .bind(category)
                .bind(cost_center)
                .bind(date)
                .execute(&pool)
                .await
            }
        };
        let fine = |kind, description: &str, date| {
            insert(company, kind, description.to_owned(), category, None, date)
        };

        assert_eq!(
            refused_by(fine("sale", "x", "2026-01-01").await).as_deref(),
            Some("financial_entries_kind_valid")
        );
        for description in ["", " x", "x ", "a\nb", &"x".repeat(201)] {
            assert_eq!(
                refused_by(fine("payable", description, "2026-01-01").await).as_deref(),
                Some("financial_entries_description_valid"),
                "{description:?}"
            );
        }
        for date in ["1899-12-31", "2201-01-01"] {
            assert_eq!(
                refused_by(fine("payable", "x", date).await).as_deref(),
                Some("financial_entries_competence_date_valid"),
                "{date}"
            );
        }
        // A category or a cost center of another company is refused by the database, and so is one
        // that does not exist: a reference is to a record of the same company.
        let foreign_company = |category: Uuid, cost_center: Option<Uuid>| {
            insert(
                other_company,
                "payable",
                "x".into(),
                category,
                cost_center,
                "2026-01-01",
            )
        };
        assert_eq!(
            refused_by(foreign_company(category, None).await).as_deref(),
            Some("financial_entries_category_fk")
        );
        assert_eq!(
            refused_by(foreign_company(Uuid::new_v4(), None).await).as_deref(),
            Some("financial_entries_category_fk")
        );
        assert!(
            insert(
                company,
                "payable",
                "x".into(),
                category,
                Some(seed.cost_center.as_uuid()),
                "2026-01-01"
            )
            .await
            .is_ok()
        );
        assert_eq!(
            refused_by(
                insert(
                    company,
                    "payable",
                    "x".into(),
                    category,
                    Some(Uuid::new_v4()),
                    "2026-01-01"
                )
                .await
            )
            .as_deref(),
            Some("financial_entries_cost_center_fk")
        );

        // An installment: a number from 1 to 120, a real date, an amount in cents and above zero.
        let entry_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO financial_entries (id, company_id, kind, description, category_id, competence_date) \
             VALUES ($1, $2, 'payable', 'Base', $3, DATE '2026-01-01')",
        )
        .bind(entry_id)
        .bind(company.as_uuid())
        .bind(category)
        .execute(&pool)
        .await
        .unwrap();
        let installment = |company: Uuid, number: i16, due: &'static str, amount: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO financial_entry_installments (company_id, entry_id, number, due_date, amount) \
                     VALUES ($1, $2, $3, CAST($4 AS DATE), CAST($5 AS NUMERIC))",
                )
                .bind(company)
                .bind(entry_id)
                .bind(number)
                .bind(due)
                .bind(amount)
                .execute(&pool)
                .await
            }
        };
        let own = company.as_uuid();
        for (number, due, amount, constraint) in [
            (
                0,
                "2026-02-01",
                "1",
                "financial_entry_installments_number_valid",
            ),
            (
                121,
                "2026-02-01",
                "1",
                "financial_entry_installments_number_valid",
            ),
            (
                1,
                "1899-01-01",
                "1",
                "financial_entry_installments_due_date_valid",
            ),
            (
                1,
                "2026-02-01",
                "0",
                "financial_entry_installments_amount_valid",
            ),
            (
                1,
                "2026-02-01",
                "-1",
                "financial_entry_installments_amount_valid",
            ),
            (
                1,
                "2026-02-01",
                "1.005",
                "financial_entry_installments_amount_valid",
            ),
            (
                1,
                "2026-02-01",
                "10000000000000",
                "financial_entry_installments_amount_valid",
            ),
        ] {
            assert_eq!(
                refused_by(installment(own, number, due, amount).await).as_deref(),
                Some(constraint),
                "{number} {due} {amount}"
            );
        }
        // An installment is in its entry's company.
        assert_eq!(
            refused_by(installment(other_company.as_uuid(), 1, "2026-02-01", "1").await).as_deref(),
            Some("financial_entry_installments_entry_fk")
        );
        assert!(
            installment(own, 1, "2026-02-01", "9999999999999.99")
                .await
                .is_ok()
        );
        assert_eq!(
            refused_by(installment(own, 1, "2026-03-01", "1").await).as_deref(),
            Some("financial_entry_installments_pkey")
        );
    }
}
