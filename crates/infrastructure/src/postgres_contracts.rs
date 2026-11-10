use std::collections::HashMap;

use conta_verde_application::{
    ContractQuery, ContractRepository, NameCursor, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    Actor, AuditEntity, AuditEntry, AuditRecord, BillingCycle, CalendarDate, CategoryId, CompanyId,
    Contract, ContractDraft, ContractId, ContractStatus, CostCenterId, LineDraft, PartyId,
    PaymentTermId, ProductId, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{push_text_keyset, snapshot, storage_error};
use crate::postgres_audit;

const SELECT_CONTRACTS: &str = "SELECT id, company_id, name, customer_id, seller_id, payment_term_id, \
     category_id, cost_center_id, notes, cycle, to_char(starts_on, 'YYYY-MM-DD') AS starts_on, \
     to_char(ends_on, 'YYYY-MM-DD') AS ends_on, status, \
     to_char(billed_through, 'YYYY-MM-DD') AS billed_through, saved_by_user_id, saved_by_name \
     FROM contracts";

pub struct PostgresContractRepository {
    pool: PgPool,
}

impl PostgresContractRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn saved_by(contract: &Contract) -> (Uuid, &str) {
    // The column holds a person's id and name; no key reaches it ([ADR 0087]).
    let by = contract.saved_by();
    (by.id(), by.name())
}

/// Writes the lines of a contract, in order, on the connection of the transaction that writes the contract.
async fn insert_lines(
    connection: &mut PgConnection,
    contract: &Contract,
) -> Result<(), RepositoryError> {
    for (index, line) in contract.lines().iter().enumerate() {
        sqlx::query(
            "INSERT INTO contract_items \
             (company_id, contract_id, position, product_id, description, quantity, unit_price, discount) \
             VALUES ($1, $2, $3, $4, $5, CAST($6 AS NUMERIC), CAST($7 AS NUMERIC), CAST($8 AS NUMERIC))",
        )
        .bind(contract.company_id().as_uuid())
        .bind(contract.id().as_uuid())
        .bind(i16::try_from(index + 1).map_err(|_| {
            RepositoryError::Storage("a contract has too many lines".to_owned())
        })?)
        .bind(line.product_id().as_uuid())
        .bind(line.description())
        .bind(line.quantity().to_string())
        .bind(line.unit_price().to_string())
        .bind(line.discount().to_string())
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
    }
    Ok(())
}

impl ContractRepository for PostgresContractRepository {
    async fn save(&self, contract: &Contract, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, contract)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let (user_id, user_name) = saved_by(contract);
        sqlx::query(
            "INSERT INTO contracts (id, company_id, name, customer_id, seller_id, payment_term_id, \
             category_id, cost_center_id, notes, cycle, starts_on, ends_on, status, billed_through, \
             saved_by_user_id, saved_by_name) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, CAST($11 AS DATE), CAST($12 AS DATE), \
                     $13, CAST($14 AS DATE), $15, $16)",
        )
        .bind(contract.id().as_uuid())
        .bind(contract.company_id().as_uuid())
        .bind(contract.name())
        .bind(contract.customer_id().as_uuid())
        .bind(contract.seller_id().map(|id| id.as_uuid()))
        .bind(contract.payment_term_id().map(|id| id.as_uuid()))
        .bind(contract.category_id().as_uuid())
        .bind(contract.cost_center_id().map(|id| id.as_uuid()))
        .bind(contract.notes())
        .bind(contract.cycle().as_str())
        .bind(contract.starts_on().to_string())
        .bind(contract.ends_on().map(|day| day.to_string()))
        .bind(contract.status().as_str())
        .bind(contract.billed_through().map(|day| day.to_string()))
        .bind(user_id)
        .bind(user_name)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        insert_lines(&mut tx, contract).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> Result<Option<Contract>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_CONTRACTS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut contracts = with_lines(&mut tx, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(contracts.pop())
    }

    async fn update(&self, contract: &Contract, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, contract)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let (user_id, user_name) = saved_by(contract);
        let result = sqlx::query(
            "UPDATE contracts SET name = $3, customer_id = $4, seller_id = $5, payment_term_id = $6, \
             category_id = $7, cost_center_id = $8, notes = $9, cycle = $10, \
             starts_on = CAST($11 AS DATE), ends_on = CAST($12 AS DATE), status = $13, \
             billed_through = GREATEST(billed_through, CAST($14 AS DATE)), \
             saved_by_user_id = $15, saved_by_name = $16 \
             WHERE id = $1 AND company_id = $2 AND status <> 'terminated'",
        )
        .bind(contract.id().as_uuid())
        .bind(contract.company_id().as_uuid())
        .bind(contract.name())
        .bind(contract.customer_id().as_uuid())
        .bind(contract.seller_id().map(|id| id.as_uuid()))
        .bind(contract.payment_term_id().map(|id| id.as_uuid()))
        .bind(contract.category_id().as_uuid())
        .bind(contract.cost_center_id().map(|id| id.as_uuid()))
        .bind(contract.notes())
        .bind(contract.cycle().as_str())
        .bind(contract.starts_on().to_string())
        .bind(contract.ends_on().map(|day| day.to_string()))
        .bind(contract.status().as_str())
        .bind(contract.billed_through().map(|day| day.to_string()))
        .bind(user_id)
        .bind(user_name)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            let exists: Option<i32> =
                sqlx::query_scalar("SELECT 1 FROM contracts WHERE id = $1 AND company_id = $2")
                    .bind(contract.id().as_uuid())
                    .bind(contract.company_id().as_uuid())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(storage_error)?;
            return Err(if exists.is_some() {
                RepositoryError::ContractChanged
            } else {
                RepositoryError::NotFound
            });
        }
        // The lines are replaced as a whole, in the same transaction as the contract and its audit entry.
        sqlx::query("DELETE FROM contract_items WHERE contract_id = $1 AND company_id = $2")
            .bind(contract.id().as_uuid())
            .bind(contract.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        insert_lines(&mut tx, contract).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn mark_billed(
        &self,
        company_id: CompanyId,
        id: ContractId,
        through: CalendarDate,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE contracts SET billed_through = CAST($3 AS DATE) \
             WHERE id = $1 AND company_id = $2 \
               AND (billed_through IS NULL OR billed_through < CAST($3 AS DATE))",
        )
        .bind(id.as_uuid())
        .bind(company_id.as_uuid())
        .bind(through.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Contract, id.as_uuid()).await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &ContractQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<Contract>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_CONTRACTS);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(status) = query.status() {
            sql.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(customer) = query.customer() {
            sql.push(" AND customer_id = ")
                .push_bind(customer.as_uuid());
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
            sql.push(" AND strpos(lower(name), lower(")
                .push_bind(text)
                .push(")) > 0");
        }
        push_text_keyset(&mut sql, "name", query.sort(), after, limit);
        let mut tx = snapshot(&self.pool).await?;
        let rows = sql
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_error)?;
        let contracts = with_lines(&mut tx, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(contracts)
    }

    async fn list_to_bill(
        &self,
        until: CalendarDate,
        after: Option<ContractId>,
        limit: usize,
    ) -> Result<Vec<Contract>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let rows = sqlx::query(&format!(
            "{SELECT_CONTRACTS} WHERE status IN ('active', 'terminated') AND starts_on <= CAST($1 AS DATE) \
             AND (billed_through IS NULL OR billed_through < CAST($1 AS DATE)) \
             AND (ends_on IS NULL OR billed_through IS NULL OR billed_through < ends_on) \
             AND ($2::uuid IS NULL OR id > $2) ORDER BY id LIMIT $3"
        ))
        .bind(until.to_string())
        .bind(after.map(|id| id.as_uuid()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        let contracts = with_lines(&mut tx, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(contracts)
    }
}

/// The contracts of these rows, each with its lines in order: one query for all of them.
async fn with_lines(
    connection: &mut PgConnection,
    rows: Vec<PgRow>,
) -> Result<Vec<Contract>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let line_rows = sqlx::query(
        "SELECT contract_id, product_id, description, quantity::text AS quantity, \
         unit_price::text AS unit_price, discount::text AS discount \
         FROM contract_items WHERE contract_id = ANY($1) ORDER BY contract_id, position",
    )
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut by_contract: HashMap<Uuid, Vec<LineDraft>> = HashMap::new();
    for row in &line_rows {
        let contract_id: Uuid = row.try_get("contract_id").map_err(storage_error)?;
        by_contract.entry(contract_id).or_default().push(LineDraft {
            product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage_error)?),
            description: row.try_get("description").map_err(storage_error)?,
            quantity: row.try_get("quantity").map_err(storage_error)?,
            unit_price: row.try_get("unit_price").map_err(storage_error)?,
            discount: row.try_get("discount").map_err(storage_error)?,
        });
    }
    rows.iter()
        .map(|row| contract_from_row(row, &mut by_contract))
        .collect()
}

fn contract_from_row(
    row: &PgRow,
    lines: &mut HashMap<Uuid, Vec<LineDraft>>,
) -> Result<Contract, RepositoryError> {
    let raw_id: Uuid = row.try_get("id").map_err(storage_error)?;
    let id = ContractId::from_uuid(raw_id);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let cycle: String = row.try_get("cycle").map_err(storage_error)?;
    let cycle = BillingCycle::parse(&cycle)
        .ok_or_else(|| RepositoryError::Storage(format!("stored contract with cycle {cycle:?}")))?;
    let status: String = row.try_get("status").map_err(storage_error)?;
    let status = ContractStatus::parse(&status).ok_or_else(|| {
        RepositoryError::Storage(format!("stored contract with status {status:?}"))
    })?;
    let seller: Option<Uuid> = row.try_get("seller_id").map_err(storage_error)?;
    let term: Option<Uuid> = row.try_get("payment_term_id").map_err(storage_error)?;
    let center: Option<Uuid> = row.try_get("cost_center_id").map_err(storage_error)?;
    let draft = ContractDraft {
        name: row.try_get("name").map_err(storage_error)?,
        customer_id: PartyId::from_uuid(row.try_get("customer_id").map_err(storage_error)?),
        seller_id: seller.map(PartyId::from_uuid),
        payment_term_id: term.map(PaymentTermId::from_uuid),
        category_id: CategoryId::from_uuid(row.try_get("category_id").map_err(storage_error)?),
        cost_center_id: center.map(CostCenterId::from_uuid),
        notes: row.try_get("notes").map_err(storage_error)?,
        lines: lines.remove(&raw_id).unwrap_or_default(),
        cycle,
        starts_on: row.try_get("starts_on").map_err(storage_error)?,
        ends_on: row.try_get("ends_on").map_err(storage_error)?,
        active: true,
    };
    let saved_by = Actor::User {
        id: UserId::from_uuid(row.try_get("saved_by_user_id").map_err(storage_error)?),
        name: row.try_get("saved_by_name").map_err(storage_error)?,
    };
    let through: Option<String> = row.try_get("billed_through").map_err(storage_error)?;
    Contract::new(id, company_id, draft, saved_by)
        .map(|contract| contract.restored(status, through.as_deref().and_then(CalendarDate::parse)))
        .map_err(|error| {
            RepositoryError::Storage(format!("stored contract {id:?} is invalid: {error}"))
        })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{ContractService, FinancialEntryService, SaleService};

    use conta_verde_application::StockService;

    use super::*;
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresPartyRepository, PostgresPaymentMethodRepository,
        PostgresPaymentTermRepository, PostgresProductRepository, PostgresQuoteRepository,
        PostgresSaleRepository, RegisterDocumentReferences, RegisterReferences, contract,
    };
    use crate::{PostgresStockMovementRepository, PostgresStockSettingsRepository};

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

    fn sales(
        pool: &PgPool,
    ) -> SaleService<
        PostgresSaleRepository,
        PostgresQuoteRepository,
        PostgresFinancialEntryRepository,
        impl conta_verde_application::EntryReferences,
        impl conta_verde_application::DocumentReferences,
        impl conta_verde_application::DocumentStock,
    > {
        SaleService::new(
            PostgresSaleRepository::new(pool.clone()),
            PostgresQuoteRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(pool),
            ),
            references(pool),
            RegisterDocumentReferences::new(
                PostgresPartyRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresPaymentTermRepository::new(pool.clone()),
            ),
            StockService::new(
                PostgresStockMovementRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresStockSettingsRepository::new(pool.clone()),
            ),
        )
    }

    async fn seeds(
        pool: &PgPool,
        company: CompanyId,
    ) -> (contract::EntrySeed, contract::SalesSeed) {
        let entry_seed = contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await;
        let seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            company,
        )
        .await;
        (entry_seed, seed)
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn bills_contracts_every_cycle() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let service =
            ContractService::new(PostgresContractRepository::new(pool.clone()), sales(&pool));
        contract::bills_contracts_every_cycle(
            &service,
            &sales(&pool),
            company,
            other_company,
            &entry_seed,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_and_a_reference_to_another_company() {
        fn refused_by<T>(result: Result<T, sqlx::Error>) -> Option<String> {
            match result {
                Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
                _ => None,
            }
        }
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let (entry_seed, seed) = seeds(&pool, company).await;
        let (other_entry, _) = seeds(&pool, other_company).await;
        let insert = |category: Uuid,
                      name: &'static str,
                      cycle: &'static str,
                      starts_on: &'static str,
                      ends_on: Option<&'static str>,
                      status: &'static str| {
            let pool = pool.clone();
            let customer = seed.customer.as_uuid();
            let id = Uuid::new_v4();
            async move {
                sqlx::query(
                    "INSERT INTO contracts (id, company_id, name, customer_id, category_id, cycle, \
                     starts_on, ends_on, status, saved_by_user_id, saved_by_name) \
                     VALUES ($1, $2, $3, $4, $5, $6, CAST($7 AS DATE), CAST($8 AS DATE), $9, $10, 'Ana')",
                )
                .bind(id)
                .bind(company.as_uuid())
                .bind(name)
                .bind(customer)
                .bind(category)
                .bind(cycle)
                .bind(starts_on)
                .bind(ends_on)
                .bind(status)
                .bind(Uuid::new_v4())
                .execute(&pool)
                .await
                .map(|_| id)
            }
        };
        let category = entry_seed.category.as_uuid();
        for (name, cycle, starts_on, ends_on, status, constraint) in [
            (
                " x",
                "monthly",
                "2026-11-05",
                None,
                "active",
                "contracts_name_valid",
            ),
            (
                "x",
                "weekly",
                "2026-11-05",
                None,
                "active",
                "contracts_cycle_valid",
            ),
            (
                "x",
                "monthly",
                "2026-11-05",
                None,
                "ended",
                "contracts_status_valid",
            ),
            (
                "x",
                "monthly",
                "1899-12-31",
                None,
                "active",
                "contracts_starts_on_valid",
            ),
            (
                "x",
                "monthly",
                "2026-11-05",
                Some("2026-11-04"),
                "active",
                "contracts_ends_on_valid",
            ),
            (
                "x",
                "monthly",
                "2026-11-05",
                None,
                "terminated",
                "contracts_terminated_ends",
            ),
        ] {
            assert_eq!(
                refused_by(insert(category, name, cycle, starts_on, ends_on, status).await)
                    .as_deref(),
                Some(constraint),
                "{name} {cycle} {starts_on} {ends_on:?} {status}"
            );
        }
        assert_eq!(
            refused_by(
                insert(
                    other_entry.category.as_uuid(),
                    "x",
                    "monthly",
                    "2026-11-05",
                    None,
                    "active"
                )
                .await
            )
            .as_deref(),
            Some("contracts_category_fk")
        );
        // Paused, so that the job of another test never reads a row that has no lines: only the domain makes those.
        let id = insert(
            category,
            "Manutenção",
            "quarterly",
            "2026-11-05",
            None,
            "paused",
        )
        .await
        .unwrap();

        // A sale bills a cycle once; it says both the contract and the day, or neither; and it comes from
        // a contract or from a quote, never both.
        let insert_sale = |contract: Option<Uuid>, cycle_on: Option<&'static str>, number: i64| {
            let pool = pool.clone();
            let customer = seed.customer.as_uuid();
            async move {
                sqlx::query(
                    "INSERT INTO sales (id, company_id, number, customer_id, sold_on, category_id, status, \
                     contract_id, cycle_on) \
                     VALUES ($1, $2, $3, $4, DATE '2026-11-05', $5, 'open', $6, CAST($7 AS DATE))",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(number)
                .bind(customer)
                .bind(category)
                .bind(contract)
                .bind(cycle_on)
                .execute(&pool)
                .await
            }
        };
        assert!(insert_sale(Some(id), Some("2026-11-05"), 1).await.is_ok());
        assert_eq!(
            refused_by(insert_sale(Some(id), Some("2026-11-05"), 2).await).as_deref(),
            Some("sales_contract_cycle_key")
        );
        assert_eq!(
            refused_by(insert_sale(Some(id), None, 3).await).as_deref(),
            Some("sales_cycle_whole")
        );
        assert_eq!(
            refused_by(insert_sale(Some(Uuid::new_v4()), Some("2026-12-05"), 4).await).as_deref(),
            Some("sales_contract_fk")
        );
    }
}
