use std::collections::HashMap;

use conta_verde_application::{
    EntryCursor, EntrySort, FinancialEntryQuery, FinancialEntryRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CategoryId, CompanyId, CostCenterId, EntryId,
    EntryInstallmentDraft, EntryKind, FinancialEntry, FinancialEntryDraft, PartyId,
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
                .push_bind(cursor.competence_date.to_string())
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
        PostgresCategoryRepository, PostgresCostCenterRepository, PostgresPartyRepository,
        RegisterReferences,
    };

    async fn seed(pool: &PgPool, company: CompanyId) -> EntrySeed {
        contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
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
    > {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
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
