use std::collections::HashMap;

use conta_verde_application::{
    NumberCursor, NumberSort, RepositoryError, SaleQuery, SaleRepository, ensure_audit_describes,
};
use conta_verde_domain::{
    Amount, AuditEntity, AuditEntry, AuditRecord, CalendarDate, CategoryId, CompanyId,
    ContractCycle, ContractId, CostCenterId, LineDraft, PartyId, PaymentTermId, ProductId, QuoteId,
    Sale, SaleDraft, SaleId, SaleReturn, SaleReturnId, SaleStatus,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{next_document_number, snapshot, storage_error};
use crate::postgres_audit;

const QUOTE_UNIQUE_INDEX: &str = "sales_quote_key";
const CYCLE_UNIQUE_INDEX: &str = "sales_contract_cycle_key";

const SELECT_SALES: &str = "SELECT id, company_id, number, customer_id, seller_id, \
     to_char(sold_on, 'YYYY-MM-DD') AS sold_on, payment_term_id, category_id, cost_center_id, \
     notes, status, quote_id, contract_id, to_char(cycle_on, 'YYYY-MM-DD') AS cycle_on FROM sales";

pub struct PostgresSaleRepository {
    pool: PgPool,
}

impl PostgresSaleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Writes the lines of a sale, in order, on the connection of the transaction that writes the sale.
async fn insert_lines(connection: &mut PgConnection, sale: &Sale) -> Result<(), RepositoryError> {
    for (index, line) in sale.lines().iter().enumerate() {
        sqlx::query(
            "INSERT INTO sale_items \
             (company_id, sale_id, position, product_id, description, quantity, unit_price, discount) \
             VALUES ($1, $2, $3, $4, $5, CAST($6 AS NUMERIC), CAST($7 AS NUMERIC), CAST($8 AS NUMERIC))",
        )
        .bind(sale.company_id().as_uuid())
        .bind(sale.id().as_uuid())
        .bind(i16::try_from(index + 1).map_err(|_| {
            RepositoryError::Storage("a sale has too many lines".to_owned())
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

impl SaleRepository for PostgresSaleRepository {
    async fn save(&self, sale: &Sale, audit: &AuditEntry) -> Result<i64, RepositoryError> {
        ensure_audit_describes(audit, sale)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let number = next_document_number(&mut tx, sale.company_id(), "sale").await?;
        sqlx::query(
            "INSERT INTO sales (id, company_id, number, customer_id, seller_id, sold_on, \
             payment_term_id, category_id, cost_center_id, notes, status, quote_id, contract_id, cycle_on) \
             VALUES ($1, $2, $3, $4, $5, CAST($6 AS DATE), $7, $8, $9, $10, $11, $12, $13, CAST($14 AS DATE))",
        )
        .bind(sale.id().as_uuid())
        .bind(sale.company_id().as_uuid())
        .bind(number)
        .bind(sale.customer_id().as_uuid())
        .bind(sale.seller_id().map(|id| id.as_uuid()))
        .bind(sale.sold_on().to_string())
        .bind(sale.payment_term_id().map(|id| id.as_uuid()))
        .bind(sale.category_id().as_uuid())
        .bind(sale.cost_center_id().map(|id| id.as_uuid()))
        .bind(sale.notes())
        .bind(sale.status().as_str())
        .bind(sale.quote_id().map(|id| id.as_uuid()))
        .bind(sale.cycle().map(|cycle| cycle.contract_id.as_uuid()))
        .bind(sale.cycle().map(|cycle| cycle.cycle_on.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(QUOTE_UNIQUE_INDEX) => {
                RepositoryError::DuplicateQuoteSale
            }
            sqlx::Error::Database(db) if db.constraint() == Some(CYCLE_UNIQUE_INDEX) => {
                RepositoryError::DuplicateCycle
            }
            _ => storage_error(error),
        })?;
        insert_lines(&mut tx, sale).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(number)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: SaleId,
    ) -> Result<Option<Sale>, RepositoryError> {
        self.find_where("company_id = $1 AND id = $2", company_id, id.as_uuid())
            .await
    }

    async fn find_by_cycle(
        &self,
        company_id: CompanyId,
        cycle: ContractCycle,
    ) -> Result<Option<Sale>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_SALES} WHERE company_id = $1 AND contract_id = $2 AND cycle_on = CAST($3 AS DATE)"
        ))
        .bind(company_id.as_uuid())
        .bind(cycle.contract_id.as_uuid())
        .bind(cycle.cycle_on.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut sales = with_lines(&mut tx, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(sales.pop())
    }

    async fn find_by_quote(
        &self,
        company_id: CompanyId,
        quote_id: QuoteId,
    ) -> Result<Option<Sale>, RepositoryError> {
        self.find_where(
            "company_id = $1 AND quote_id = $2",
            company_id,
            quote_id.as_uuid(),
        )
        .await
    }

    async fn update(
        &self,
        sale: &Sale,
        expected: SaleStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, sale)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE sales SET customer_id = $3, seller_id = $4, sold_on = CAST($5 AS DATE), \
             payment_term_id = $6, category_id = $7, cost_center_id = $8, notes = $9, status = $10 \
             WHERE id = $1 AND company_id = $2 AND status = $11",
        )
        .bind(sale.id().as_uuid())
        .bind(sale.company_id().as_uuid())
        .bind(sale.customer_id().as_uuid())
        .bind(sale.seller_id().map(|id| id.as_uuid()))
        .bind(sale.sold_on().to_string())
        .bind(sale.payment_term_id().map(|id| id.as_uuid()))
        .bind(sale.category_id().as_uuid())
        .bind(sale.cost_center_id().map(|id| id.as_uuid()))
        .bind(sale.notes())
        .bind(sale.status().as_str())
        .bind(expected.as_str())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            let exists: Option<i32> =
                sqlx::query_scalar("SELECT 1 FROM sales WHERE id = $1 AND company_id = $2")
                    .bind(sale.id().as_uuid())
                    .bind(sale.company_id().as_uuid())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(storage_error)?;
            return Err(if exists.is_some() {
                RepositoryError::SaleChanged
            } else {
                RepositoryError::NotFound
            });
        }
        // The lines are replaced as a whole, in the same transaction as the sale and its audit entry.
        sqlx::query("DELETE FROM sale_items WHERE sale_id = $1 AND company_id = $2")
            .bind(sale.id().as_uuid())
            .bind(sale.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        insert_lines(&mut tx, sale).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn set_status(
        &self,
        sale: &Sale,
        expected: SaleStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, sale)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The sale is locked: a return that is being stored takes the same lock before it looks at the status.
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT status FROM sales WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(sale.id().as_uuid())
        .bind(sale.company_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        match stored {
            None => return Err(RepositoryError::NotFound),
            Some(status) if status != expected.as_str() => {
                return Err(RepositoryError::SaleChanged);
            }
            Some(_) => {}
        }
        if sale.status() == SaleStatus::Cancelled {
            let returned: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM sale_returns WHERE company_id = $1 AND sale_id = $2)",
            )
            .bind(sale.company_id().as_uuid())
            .bind(sale.id().as_uuid())
            .fetch_one(&mut *tx)
            .await
            .map_err(storage_error)?;
            if returned {
                return Err(RepositoryError::SaleHasReturns);
            }
        }
        sqlx::query("UPDATE sales SET status = $3 WHERE id = $1 AND company_id = $2")
            .bind(sale.id().as_uuid())
            .bind(sale.company_id().as_uuid())
            .bind(sale.status().as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: SaleId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Sale, id.as_uuid()).await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &SaleQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> Result<Vec<Sale>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_SALES);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(status) = query.status() {
            sql.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(customer) = query.customer() {
            sql.push(" AND customer_id = ")
                .push_bind(customer.as_uuid());
        }
        if let Some(seller) = query.seller() {
            sql.push(" AND seller_id = ").push_bind(seller.as_uuid());
        }
        if query.requires_a_seller() {
            sql.push(" AND seller_id IS NOT NULL");
        }
        if let Some(contract) = query.contract() {
            sql.push(" AND contract_id = ")
                .push_bind(contract.as_uuid());
        }
        if let Some((from, to)) = query.span() {
            sql.push(" AND sold_on BETWEEN CAST(")
                .push_bind(from.to_string())
                .push(" AS DATE) AND CAST(")
                .push_bind(to.to_string())
                .push(" AS DATE)");
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
            sql.push(" AND (number::text = ")
                .push_bind(text.to_owned())
                .push(" OR strpos(lower(coalesce(notes, '')), lower(")
                .push_bind(text.to_owned())
                .push(
                    ")) > 0 OR EXISTS (SELECT 1 FROM sale_items i WHERE i.sale_id = sales.id \
                       AND strpos(lower(i.description), lower(",
                )
                .push_bind(text.to_owned())
                .push(")) > 0))");
        }
        let (comparison, direction) = match query.sort() {
            NumberSort::NumberAsc => (">", "ASC"),
            NumberSort::NumberDesc => ("<", "DESC"),
        };
        if let Some(cursor) = after {
            sql.push(format!(" AND (number, id) {comparison} ("))
                .push_bind(cursor.number)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        sql.push(format!(
            " ORDER BY number {direction}, id {direction} LIMIT "
        ))
        .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));

        let mut tx = snapshot(&self.pool).await?;
        let rows = sql
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_error)?;
        let sales = with_lines(&mut tx, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(sales)
    }

    async fn save_return(
        &self,
        sale_return: &SaleReturn,
        known_returns: usize,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, sale_return)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The sale is locked first: two returns of the same sale take turns, and what was returned before is
        // read after the one before has committed.
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM sales WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(sale_return.sale_id().as_uuid())
        .bind(sale_return.company_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        match status.as_deref() {
            None => return Err(RepositoryError::NotFound),
            Some("completed") => {}
            Some(_) => return Err(RepositoryError::SaleChanged),
        }
        let stored: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sale_returns WHERE company_id = $1 AND sale_id = $2",
        )
        .bind(sale_return.company_id().as_uuid())
        .bind(sale_return.sale_id().as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(storage_error)?;
        if usize::try_from(stored).ok() != Some(known_returns) {
            return Err(RepositoryError::SaleChanged);
        }
        for (line, quantity) in sale_return.items() {
            let fits: Option<bool> = sqlx::query_scalar(
                "SELECT i.quantity >= COALESCE(( \
                     SELECT SUM(r.quantity) FROM sale_return_items r \
                     JOIN sale_returns s ON s.company_id = r.company_id AND s.id = r.return_id \
                     WHERE s.company_id = i.company_id AND s.sale_id = i.sale_id AND r.line = i.position), 0) \
                     + CAST($4 AS NUMERIC) \
                 FROM sale_items i WHERE i.company_id = $1 AND i.sale_id = $2 AND i.position = $3",
            )
            .bind(sale_return.company_id().as_uuid())
            .bind(sale_return.sale_id().as_uuid())
            .bind(i16::try_from(*line).map_err(|_| RepositoryError::ReturnTooMuch)?)
            .bind(quantity.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage_error)?;
            if fits != Some(true) {
                return Err(RepositoryError::ReturnTooMuch);
            }
        }
        sqlx::query(
            "INSERT INTO sale_returns (id, company_id, sale_id, returned_on, reason, refund) \
             VALUES ($1, $2, $3, CAST($4 AS DATE), $5, CAST($6 AS NUMERIC))",
        )
        .bind(sale_return.id().as_uuid())
        .bind(sale_return.company_id().as_uuid())
        .bind(sale_return.sale_id().as_uuid())
        .bind(sale_return.returned_on().to_string())
        .bind(sale_return.reason())
        .bind(sale_return.refund().to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        for (index, (line, quantity)) in sale_return.items().iter().enumerate() {
            sqlx::query(
                "INSERT INTO sale_return_items (company_id, return_id, position, line, quantity) \
                 VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC))",
            )
            .bind(sale_return.company_id().as_uuid())
            .bind(sale_return.id().as_uuid())
            .bind(i16::try_from(index + 1).map_err(|_| RepositoryError::ReturnTooMuch)?)
            .bind(i16::try_from(*line).map_err(|_| RepositoryError::ReturnTooMuch)?)
            .bind(quantity.to_string())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn returns_of_sales(
        &self,
        company_id: CompanyId,
        sale_ids: &[SaleId],
    ) -> Result<Vec<SaleReturn>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let rows = sqlx::query(
            "SELECT id, sale_id, to_char(returned_on, 'YYYY-MM-DD') AS returned_on, reason, \
             refund::text AS refund FROM sale_returns \
             WHERE company_id = $1 AND sale_id = ANY($2) ORDER BY created_at, id",
        )
        .bind(company_id.as_uuid())
        .bind(
            sale_ids
                .iter()
                .map(|id| id.as_uuid())
                .collect::<Vec<Uuid>>(),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        let ids: Vec<Uuid> = rows
            .iter()
            .map(|row| row.try_get("id").map_err(storage_error))
            .collect::<Result<_, _>>()?;
        let item_rows = sqlx::query(
            "SELECT return_id, line, quantity::text AS quantity FROM sale_return_items \
             WHERE company_id = $1 AND return_id = ANY($2) ORDER BY return_id, position",
        )
        .bind(company_id.as_uuid())
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        let mut items: HashMap<Uuid, Vec<(u16, Amount)>> = HashMap::new();
        for row in &item_rows {
            let line: i16 = row.try_get("line").map_err(storage_error)?;
            let quantity: String = row.try_get("quantity").map_err(storage_error)?;
            items
                .entry(row.try_get("return_id").map_err(storage_error)?)
                .or_default()
                .push((
                    u16::try_from(line).map_err(|_| {
                        RepositoryError::Storage(format!("stored return of line {line}"))
                    })?,
                    Amount::parse(&quantity).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored return of {quantity}"))
                    })?,
                ));
        }
        rows.iter()
            .map(|row| {
                let raw: Uuid = row.try_get("id").map_err(storage_error)?;
                let sale_id = SaleId::from_uuid(row.try_get("sale_id").map_err(storage_error)?);
                let day: String = row.try_get("returned_on").map_err(storage_error)?;
                let refund: String = row.try_get("refund").map_err(storage_error)?;
                Ok(SaleReturn::restored(
                    SaleReturnId::from_uuid(raw),
                    company_id,
                    sale_id,
                    CalendarDate::parse(&day).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored day {day} is invalid"))
                    })?,
                    row.try_get("reason").map_err(storage_error)?,
                    items.remove(&raw).unwrap_or_default(),
                    Amount::parse(&refund).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored refund {refund} is invalid"))
                    })?,
                ))
            })
            .collect()
    }

    async fn returns_of(
        &self,
        company_id: CompanyId,
        sale_id: SaleId,
    ) -> Result<Vec<SaleReturn>, RepositoryError> {
        self.returns_of_sales(company_id, &[sale_id]).await
    }
}

impl PostgresSaleRepository {
    async fn find_where(
        &self,
        condition: &str,
        company_id: CompanyId,
        key: Uuid,
    ) -> Result<Option<Sale>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!("{SELECT_SALES} WHERE {condition}"))
            .bind(company_id.as_uuid())
            .bind(key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut sales = with_lines(&mut tx, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(sales.pop())
    }
}

/// The sales of these rows, each with its lines in order: one query for all of them.
async fn with_lines(
    connection: &mut PgConnection,
    rows: Vec<PgRow>,
) -> Result<Vec<Sale>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let line_rows = sqlx::query(
        "SELECT sale_id, product_id, description, quantity::text AS quantity, \
         unit_price::text AS unit_price, discount::text AS discount \
         FROM sale_items WHERE sale_id = ANY($1) ORDER BY sale_id, position",
    )
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut by_sale: HashMap<Uuid, Vec<LineDraft>> = HashMap::new();
    for row in &line_rows {
        let sale_id: Uuid = row.try_get("sale_id").map_err(storage_error)?;
        by_sale.entry(sale_id).or_default().push(LineDraft {
            product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage_error)?),
            description: row.try_get("description").map_err(storage_error)?,
            quantity: row.try_get("quantity").map_err(storage_error)?,
            unit_price: row.try_get("unit_price").map_err(storage_error)?,
            discount: row.try_get("discount").map_err(storage_error)?,
        });
    }
    rows.iter()
        .map(|row| sale_from_row(row, &mut by_sale))
        .collect()
}

fn sale_from_row(
    row: &PgRow,
    lines: &mut HashMap<Uuid, Vec<LineDraft>>,
) -> Result<Sale, RepositoryError> {
    let raw_id: Uuid = row.try_get("id").map_err(storage_error)?;
    let id = SaleId::from_uuid(raw_id);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let status: String = row.try_get("status").map_err(storage_error)?;
    let status = SaleStatus::parse(&status)
        .ok_or_else(|| RepositoryError::Storage(format!("stored sale with status {status:?}")))?;
    let seller: Option<Uuid> = row.try_get("seller_id").map_err(storage_error)?;
    let term: Option<Uuid> = row.try_get("payment_term_id").map_err(storage_error)?;
    let center: Option<Uuid> = row.try_get("cost_center_id").map_err(storage_error)?;
    let quote: Option<Uuid> = row.try_get("quote_id").map_err(storage_error)?;
    let draft = SaleDraft {
        customer_id: PartyId::from_uuid(row.try_get("customer_id").map_err(storage_error)?),
        seller_id: seller.map(PartyId::from_uuid),
        sold_on: row.try_get("sold_on").map_err(storage_error)?,
        payment_term_id: term.map(PaymentTermId::from_uuid),
        category_id: CategoryId::from_uuid(row.try_get("category_id").map_err(storage_error)?),
        cost_center_id: center.map(CostCenterId::from_uuid),
        notes: row.try_get("notes").map_err(storage_error)?,
        lines: lines.remove(&raw_id).unwrap_or_default(),
    };
    let number: i64 = row.try_get("number").map_err(storage_error)?;
    let contract: Option<Uuid> = row.try_get("contract_id").map_err(storage_error)?;
    let cycle_on: Option<String> = row.try_get("cycle_on").map_err(storage_error)?;
    let cycle = contract
        .zip(cycle_on.as_deref().and_then(CalendarDate::parse))
        .map(|(contract_id, cycle_on)| ContractCycle {
            contract_id: ContractId::from_uuid(contract_id),
            cycle_on,
        });
    Sale::new(id, company_id, draft)
        .map(|sale| {
            sale.restored(number, status, quote.map(QuoteId::from_uuid))
                .with_cycle(cycle)
        })
        .map_err(|error| {
            RepositoryError::Storage(format!("stored sale {id:?} is invalid: {error}"))
        })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{FinancialEntryService, QuoteService, SaleService};

    use super::*;
    use crate::{
        Argon2SecretService, InMemoryEmailSender, PostgresBankAccountRepository,
        PostgresCategoryRepository, PostgresCostCenterRepository, PostgresFinancialEntryRepository,
        PostgresPartyRepository, PostgresPaymentMethodRepository, PostgresPaymentTermRepository,
        PostgresProductRepository, PostgresQuoteRepository, RegisterDocumentReferences,
        RegisterReferences, contract,
    };

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

    fn document_references(
        pool: &PgPool,
    ) -> RegisterDocumentReferences<
        PostgresPartyRepository,
        PostgresProductRepository,
        PostgresPaymentTermRepository,
    > {
        RegisterDocumentReferences::new(
            PostgresPartyRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresPaymentTermRepository::new(pool.clone()),
        )
    }

    async fn entry_seed(pool: &PgPool, company: CompanyId) -> contract::EntrySeed {
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

    async fn sales_seed(pool: &PgPool, company: CompanyId) -> contract::SalesSeed {
        contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            company,
        )
        .await
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn voids_entries() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = entry_seed(&pool, company).await;
        let entries = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        contract::voids_entries(&entries, company, other_company, &seed).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_sale_lifecycle() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let entry_seed = entry_seed(&pool, company).await;
        let seed = sales_seed(&pool, company).await;
        let store = PostgresSaleRepository::new(pool.clone());
        let sales = SaleService::new(
            PostgresSaleRepository::new(pool.clone()),
            PostgresQuoteRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            ),
            references(&pool),
            document_references(&pool),
        );
        let quotes = QuoteService::new(
            PostgresQuoteRepository::new(pool.clone()),
            document_references(&pool),
            InMemoryEmailSender::default(),
            Argon2SecretService::new().unwrap(),
            "https://erp.example",
        );
        let entries = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        contract::runs_the_sale_lifecycle(
            &sales,
            &store,
            &quotes,
            &entries,
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
        let entries = entry_seed(&pool, company).await;
        let other_entries = entry_seed(&pool, other_company).await;
        let seed = sales_seed(&pool, company).await;
        let insert_sale = |number: i64,
                           customer: Uuid,
                           category: Uuid,
                           status: &'static str,
                           sold_on: &'static str| {
            let pool = pool.clone();
            let id = Uuid::new_v4();
            async move {
                sqlx::query(
                    "INSERT INTO sales (id, company_id, number, customer_id, sold_on, category_id, status) \
                     VALUES ($1, $2, $3, $4, CAST($5 AS DATE), $6, $7)",
                )
                .bind(id)
                .bind(company.as_uuid())
                .bind(number)
                .bind(customer)
                .bind(sold_on)
                .bind(category)
                .bind(status)
                .execute(&pool)
                .await
                .map(|_| id)
            }
        };
        let (customer, category) = (seed.customer.as_uuid(), entries.category.as_uuid());
        for (number, status, sold_on, constraint) in [
            (0, "open", "2026-11-06", "sales_number_valid"),
            (1, "paid", "2026-11-06", "sales_status_valid"),
            (1, "open", "1899-12-31", "sales_sold_on_valid"),
        ] {
            assert_eq!(
                refused_by(insert_sale(number, customer, category, status, sold_on).await)
                    .as_deref(),
                Some(constraint),
                "{number} {status} {sold_on}"
            );
        }
        assert_eq!(
            refused_by(
                insert_sale(
                    1,
                    customer,
                    other_entries.category.as_uuid(),
                    "open",
                    "2026-11-06"
                )
                .await
            )
            .as_deref(),
            Some("sales_category_fk")
        );
        let id = insert_sale(1, customer, category, "open", "2026-11-06")
            .await
            .unwrap();
        assert_eq!(
            refused_by(insert_sale(1, customer, category, "open", "2026-11-06").await).as_deref(),
            Some("sales_company_number_key")
        );

        // The lines follow the same rules as a quote's.
        let insert_item = |quantity: &'static str, price: &'static str, discount: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO sale_items (company_id, sale_id, position, product_id, description, \
                     quantity, unit_price, discount) \
                     VALUES ($1, $2, 1, $3, 'Café', CAST($4 AS NUMERIC), CAST($5 AS NUMERIC), CAST($6 AS NUMERIC))",
                )
                .bind(company.as_uuid())
                .bind(id)
                .bind(seed.product.as_uuid())
                .bind(quantity)
                .bind(price)
                .bind(discount)
                .execute(&pool)
                .await
            }
        };
        assert_eq!(
            refused_by(insert_item("0", "1", "0").await).as_deref(),
            Some("sale_items_quantity_valid")
        );
        assert_eq!(
            refused_by(insert_item("1.5", "3.33", "5.01").await).as_deref(),
            Some("sale_items_discount_valid")
        );
        assert!(insert_item("1.5", "3.33", "5").await.is_ok());

        // A return takes back a line once, a whole number of cents, and an entry says at most one origin.
        let return_id = Uuid::new_v4();
        let insert_return = |refund: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO sale_returns (id, company_id, sale_id, returned_on, refund) \
                     VALUES ($1, $2, $3, DATE '2026-11-10', CAST($4 AS NUMERIC))",
                )
                .bind(Uuid::new_v4())
                .bind(company.as_uuid())
                .bind(id)
                .bind(refund)
                .execute(&pool)
                .await
            }
        };
        assert_eq!(
            refused_by(insert_return("0").await).as_deref(),
            Some("sale_returns_refund_valid")
        );
        assert_eq!(
            refused_by(insert_return("1.005").await).as_deref(),
            Some("sale_returns_refund_valid")
        );
        sqlx::query(
            "INSERT INTO sale_returns (id, company_id, sale_id, returned_on, refund) \
             VALUES ($1, $2, $3, DATE '2026-11-10', 1)",
        )
        .bind(return_id)
        .bind(company.as_uuid())
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let insert_return_item = |position: i16, line: i16, quantity: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO sale_return_items (company_id, return_id, position, line, quantity) \
                     VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC))",
                )
                .bind(company.as_uuid())
                .bind(return_id)
                .bind(position)
                .bind(line)
                .bind(quantity)
                .execute(&pool)
                .await
            }
        };
        assert!(insert_return_item(1, 1, "1").await.is_ok());
        assert_eq!(
            refused_by(insert_return_item(2, 1, "1").await).as_deref(),
            Some("sale_return_items_line_key")
        );
        assert_eq!(
            refused_by(insert_return_item(2, 2, "0.0001").await).as_deref(),
            Some("sale_return_items_quantity_valid")
        );
        let two_origins = sqlx::query(
            "INSERT INTO financial_entries (id, company_id, kind, description, category_id, competence_date, \
             sale_id, sale_return_id) VALUES ($1, $2, 'receivable', 'X', $3, DATE '2026-11-05', $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(company.as_uuid())
        .bind(category)
        .bind(id)
        .bind(return_id)
        .execute(&pool)
        .await;
        assert_eq!(
            refused_by(two_origins).as_deref(),
            Some("financial_entries_one_origin")
        );
    }
}
