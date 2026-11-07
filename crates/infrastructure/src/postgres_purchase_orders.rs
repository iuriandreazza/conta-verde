use std::collections::HashMap;

use conta_verde_application::{
    NumberCursor, NumberSort, PurchaseOrderQuery, PurchaseOrderRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AccessKey, Amount, AuditEntity, AuditEntry, AuditRecord, CalendarDate, CategoryId, CompanyId,
    CostCenterId, LineDraft, PartyId, PaymentTermId, ProductId, PurchaseOrder, PurchaseOrderDraft,
    PurchaseOrderId, PurchaseReceipt, PurchaseReceiptId, PurchaseStatus, ReceiptItem,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{next_document_number, snapshot, storage_error};
use crate::postgres_audit;

const INVOICE_UNIQUE_INDEX: &str = "purchase_orders_invoice_key_key";

/// ` WHERE company_id = ... AND ...` for what `query` narrows to, shared by the list and the counts so that
/// both agree on what matches.
fn push_filters(
    sql: &mut QueryBuilder<'_, Postgres>,
    company_id: CompanyId,
    query: &PurchaseOrderQuery,
) {
    sql.push(" WHERE company_id = ")
        .push_bind(company_id.as_uuid());
    if let Some(status) = query.status() {
        sql.push(" AND status = ").push_bind(status.as_str());
    }
    if let Some(supplier) = query.supplier() {
        sql.push(" AND supplier_id = ")
            .push_bind(supplier.as_uuid());
    }
    if let Some(text) = query.search() {
        // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
        sql.push(" AND (number::text = ")
            .push_bind(text.to_owned())
            .push(" OR strpos(lower(coalesce(notes, '')), lower(")
            .push_bind(text.to_owned())
            .push(
                ")) > 0 OR EXISTS (SELECT 1 FROM purchase_order_items i \
                   WHERE i.purchase_id = purchase_orders.id \
                   AND strpos(lower(i.description), lower(",
            )
            .push_bind(text.to_owned())
            .push(")) > 0))");
    }
}

const SELECT_ORDERS: &str = "SELECT id, company_id, number, supplier_id, \
     to_char(ordered_on, 'YYYY-MM-DD') AS ordered_on, \
     to_char(expected_on, 'YYYY-MM-DD') AS expected_on, payment_term_id, category_id, \
     cost_center_id, notes, status, invoice_key FROM purchase_orders";

pub struct PostgresPurchaseOrderRepository {
    pool: PgPool,
}

impl PostgresPurchaseOrderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Writes the lines of an order, in order, on the connection of the transaction that writes the order.
async fn insert_lines(
    connection: &mut PgConnection,
    order: &PurchaseOrder,
) -> Result<(), RepositoryError> {
    for (index, line) in order.lines().iter().enumerate() {
        sqlx::query(
            "INSERT INTO purchase_order_items \
             (company_id, purchase_id, position, product_id, description, quantity, unit_price, discount) \
             VALUES ($1, $2, $3, $4, $5, CAST($6 AS NUMERIC), CAST($7 AS NUMERIC), CAST($8 AS NUMERIC))",
        )
        .bind(order.company_id().as_uuid())
        .bind(order.id().as_uuid())
        .bind(i16::try_from(index + 1).map_err(|_| {
            RepositoryError::Storage("a purchase order has too many lines".to_owned())
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

/// The status of the order, locked until the transaction ends: whoever writes to the order or its receipts
/// takes this lock first, so they take turns.
async fn lock_status(
    connection: &mut PgConnection,
    company_id: CompanyId,
    id: PurchaseOrderId,
) -> Result<String, RepositoryError> {
    sqlx::query_scalar(
        "SELECT status FROM purchase_orders WHERE id = $1 AND company_id = $2 FOR UPDATE",
    )
    .bind(id.as_uuid())
    .bind(company_id.as_uuid())
    .fetch_optional(&mut *connection)
    .await
    .map_err(storage_error)?
    .ok_or(RepositoryError::NotFound)
}

impl PurchaseOrderRepository for PostgresPurchaseOrderRepository {
    async fn save(
        &self,
        order: &PurchaseOrder,
        audit: &AuditEntry,
    ) -> Result<i64, RepositoryError> {
        ensure_audit_describes(audit, order)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let number = next_document_number(&mut tx, order.company_id(), "purchase_order").await?;
        sqlx::query(
            "INSERT INTO purchase_orders (id, company_id, number, supplier_id, ordered_on, expected_on, \
             payment_term_id, category_id, cost_center_id, notes, status, invoice_key) \
             VALUES ($1, $2, $3, $4, CAST($5 AS DATE), CAST($6 AS DATE), $7, $8, $9, $10, $11, $12)",
        )
        .bind(order.id().as_uuid())
        .bind(order.company_id().as_uuid())
        .bind(number)
        .bind(order.supplier_id().as_uuid())
        .bind(order.ordered_on().to_string())
        .bind(order.expected_on().map(|day| day.to_string()))
        .bind(order.payment_term_id().map(|id| id.as_uuid()))
        .bind(order.category_id().as_uuid())
        .bind(order.cost_center_id().map(|id| id.as_uuid()))
        .bind(order.notes())
        .bind(order.status().as_str())
        .bind(order.invoice_key().map(AccessKey::as_str))
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(INVOICE_UNIQUE_INDEX) => {
                RepositoryError::DuplicateInvoice
            }
            _ => storage_error(error),
        })?;
        insert_lines(&mut tx, order).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(number)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> Result<Option<PurchaseOrder>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_ORDERS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut orders = with_lines(&mut tx, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(orders.pop())
    }

    async fn find_by_invoice_key(
        &self,
        company_id: CompanyId,
        key: &AccessKey,
    ) -> Result<Option<PurchaseOrder>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_ORDERS} WHERE company_id = $1 AND invoice_key = $2 AND status <> 'cancelled'"
        ))
        .bind(company_id.as_uuid())
        .bind(key.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut orders = with_lines(&mut tx, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(orders.pop())
    }

    async fn update(
        &self,
        order: &PurchaseOrder,
        expected: PurchaseStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, order)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let stored = lock_status(&mut tx, order.company_id(), order.id()).await?;
        if stored != expected.as_str() {
            return Err(RepositoryError::PurchaseChanged);
        }
        sqlx::query(
            "UPDATE purchase_orders SET supplier_id = $3, ordered_on = CAST($4 AS DATE), \
             expected_on = CAST($5 AS DATE), payment_term_id = $6, category_id = $7, \
             cost_center_id = $8, notes = $9, status = $10 WHERE id = $1 AND company_id = $2",
        )
        .bind(order.id().as_uuid())
        .bind(order.company_id().as_uuid())
        .bind(order.supplier_id().as_uuid())
        .bind(order.ordered_on().to_string())
        .bind(order.expected_on().map(|day| day.to_string()))
        .bind(order.payment_term_id().map(|id| id.as_uuid()))
        .bind(order.category_id().as_uuid())
        .bind(order.cost_center_id().map(|id| id.as_uuid()))
        .bind(order.notes())
        .bind(order.status().as_str())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        // The lines are replaced as a whole, in the same transaction as the order and its audit entry.
        sqlx::query("DELETE FROM purchase_order_items WHERE purchase_id = $1 AND company_id = $2")
            .bind(order.id().as_uuid())
            .bind(order.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        insert_lines(&mut tx, order).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn set_status(
        &self,
        order: &PurchaseOrder,
        expected: PurchaseStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, order)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let stored = lock_status(&mut tx, order.company_id(), order.id()).await?;
        if stored != expected.as_str() {
            return Err(RepositoryError::PurchaseChanged);
        }
        sqlx::query("UPDATE purchase_orders SET status = $3 WHERE id = $1 AND company_id = $2")
            .bind(order.id().as_uuid())
            .bind(order.company_id().as_uuid())
            .bind(order.status().as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::PurchaseOrder,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PurchaseOrderQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> Result<Vec<PurchaseOrder>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_ORDERS);
        push_filters(&mut sql, company_id, query);
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
        let orders = with_lines(&mut tx, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(orders)
    }

    async fn count_by_status(
        &self,
        company_id: CompanyId,
        query: &PurchaseOrderQuery,
    ) -> Result<Vec<(PurchaseStatus, u64)>, RepositoryError> {
        let mut sql =
            QueryBuilder::<Postgres>::new("SELECT status, count(*) AS total FROM purchase_orders");
        push_filters(&mut sql, company_id, &query.clone().any_status());
        sql.push(" GROUP BY status");
        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter()
            .map(|row| {
                let status: String = row.get("status");
                let total: i64 = row.get("total");
                let status = PurchaseStatus::parse(&status)
                    .ok_or_else(|| RepositoryError::Storage(format!("unknown status {status}")))?;
                Ok((status, u64::try_from(total).unwrap_or(0)))
            })
            .collect()
    }

    async fn save_receipt(
        &self,
        receipt: &PurchaseReceipt,
        order_after: &PurchaseOrder,
        known_receipts: usize,
        receipt_audit: &AuditEntry,
        order_audit: Option<&AuditEntry>,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(receipt_audit, receipt)?;
        if let Some(audit) = order_audit {
            ensure_audit_describes(audit, order_after)?;
        }
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The order is locked first: two receipts of the same order take turns, and what arrived before is
        // read after the one before has committed.
        let status = lock_status(&mut tx, receipt.company_id(), receipt.purchase_id()).await?;
        if !matches!(status.as_str(), "approved" | "partially_received") {
            return Err(RepositoryError::PurchaseChanged);
        }
        let stored: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM purchase_receipts WHERE company_id = $1 AND purchase_id = $2",
        )
        .bind(receipt.company_id().as_uuid())
        .bind(receipt.purchase_id().as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(storage_error)?;
        if usize::try_from(stored).ok() != Some(known_receipts) {
            return Err(RepositoryError::PurchaseChanged);
        }
        for item in receipt.items() {
            let fits: Option<bool> = sqlx::query_scalar(
                "SELECT i.quantity >= COALESCE(( \
                     SELECT SUM(r.quantity) FROM purchase_receipt_items r \
                     JOIN purchase_receipts s ON s.company_id = r.company_id AND s.id = r.receipt_id \
                     WHERE s.company_id = i.company_id AND s.purchase_id = i.purchase_id \
                       AND r.line = i.position), 0) + CAST($4 AS NUMERIC) \
                 FROM purchase_order_items i \
                 WHERE i.company_id = $1 AND i.purchase_id = $2 AND i.position = $3",
            )
            .bind(receipt.company_id().as_uuid())
            .bind(receipt.purchase_id().as_uuid())
            .bind(i16::try_from(item.line).map_err(|_| RepositoryError::ReceiptTooMuch)?)
            .bind(item.quantity.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage_error)?;
            if fits != Some(true) {
                return Err(RepositoryError::ReceiptTooMuch);
            }
        }
        sqlx::query(
            "INSERT INTO purchase_receipts (id, company_id, purchase_id, received_on, notes) \
             VALUES ($1, $2, $3, CAST($4 AS DATE), $5)",
        )
        .bind(receipt.id().as_uuid())
        .bind(receipt.company_id().as_uuid())
        .bind(receipt.purchase_id().as_uuid())
        .bind(receipt.received_on().to_string())
        .bind(receipt.notes())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        for (index, item) in receipt.items().iter().enumerate() {
            sqlx::query(
                "INSERT INTO purchase_receipt_items (company_id, receipt_id, position, line, quantity, value) \
                 VALUES ($1, $2, $3, $4, CAST($5 AS NUMERIC), CAST($6 AS NUMERIC))",
            )
            .bind(receipt.company_id().as_uuid())
            .bind(receipt.id().as_uuid())
            .bind(i16::try_from(index + 1).map_err(|_| RepositoryError::ReceiptTooMuch)?)
            .bind(i16::try_from(item.line).map_err(|_| RepositoryError::ReceiptTooMuch)?)
            .bind(item.quantity.to_string())
            .bind(item.value.to_string())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        for (index, (due, amount)) in receipt.schedule().iter().enumerate() {
            sqlx::query(
                "INSERT INTO purchase_receipt_installments (company_id, receipt_id, position, due_date, amount) \
                 VALUES ($1, $2, $3, CAST($4 AS DATE), CAST($5 AS NUMERIC))",
            )
            .bind(receipt.company_id().as_uuid())
            .bind(receipt.id().as_uuid())
            .bind(i16::try_from(index + 1).map_err(|_| RepositoryError::ReceiptTooMuch)?)
            .bind(due.to_string())
            .bind(amount.to_string())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        postgres_audit::insert(&mut tx, receipt_audit).await?;
        if let Some(audit) = order_audit {
            sqlx::query("UPDATE purchase_orders SET status = $3 WHERE id = $1 AND company_id = $2")
                .bind(order_after.id().as_uuid())
                .bind(order_after.company_id().as_uuid())
                .bind(order_after.status().as_str())
                .execute(&mut *tx)
                .await
                .map_err(storage_error)?;
            postgres_audit::insert(&mut tx, audit).await?;
        }
        tx.commit().await.map_err(storage_error)
    }

    async fn receipts_of(
        &self,
        company_id: CompanyId,
        order_id: PurchaseOrderId,
    ) -> Result<Vec<PurchaseReceipt>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let rows = sqlx::query(
            "SELECT id, to_char(received_on, 'YYYY-MM-DD') AS received_on, notes \
             FROM purchase_receipts WHERE company_id = $1 AND purchase_id = $2 \
             ORDER BY created_at, id",
        )
        .bind(company_id.as_uuid())
        .bind(order_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        let ids: Vec<Uuid> = rows
            .iter()
            .map(|row| row.try_get("id").map_err(storage_error))
            .collect::<Result<_, _>>()?;
        let item_rows = sqlx::query(
            "SELECT receipt_id, line, quantity::text AS quantity, value::text AS value \
             FROM purchase_receipt_items WHERE company_id = $1 AND receipt_id = ANY($2) \
             ORDER BY receipt_id, position",
        )
        .bind(company_id.as_uuid())
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        let schedule_rows = sqlx::query(
            "SELECT receipt_id, to_char(due_date, 'YYYY-MM-DD') AS due_date, amount::text AS amount \
             FROM purchase_receipt_installments WHERE company_id = $1 AND receipt_id = ANY($2) \
             ORDER BY receipt_id, position",
        )
        .bind(company_id.as_uuid())
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        let mut schedules: HashMap<Uuid, Vec<(CalendarDate, Amount)>> = HashMap::new();
        for row in &schedule_rows {
            let due: String = row.try_get("due_date").map_err(storage_error)?;
            let amount: String = row.try_get("amount").map_err(storage_error)?;
            schedules
                .entry(row.try_get("receipt_id").map_err(storage_error)?)
                .or_default()
                .push((
                    CalendarDate::parse(&due).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored day {due} is invalid"))
                    })?,
                    Amount::parse(&amount).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored installment {amount} is invalid"))
                    })?,
                ));
        }
        let mut items: HashMap<Uuid, Vec<ReceiptItem>> = HashMap::new();
        for row in &item_rows {
            let line: i16 = row.try_get("line").map_err(storage_error)?;
            let quantity: String = row.try_get("quantity").map_err(storage_error)?;
            let value: String = row.try_get("value").map_err(storage_error)?;
            items
                .entry(row.try_get("receipt_id").map_err(storage_error)?)
                .or_default()
                .push(ReceiptItem {
                    line: u16::try_from(line).map_err(|_| {
                        RepositoryError::Storage(format!("stored receipt of line {line}"))
                    })?,
                    quantity: Amount::parse(&quantity).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored receipt of {quantity}"))
                    })?,
                    value: Amount::parse(&value).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored receipt value {value}"))
                    })?,
                });
        }
        rows.iter()
            .map(|row| {
                let raw: Uuid = row.try_get("id").map_err(storage_error)?;
                let day: String = row.try_get("received_on").map_err(storage_error)?;
                Ok(PurchaseReceipt::restored(
                    PurchaseReceiptId::from_uuid(raw),
                    company_id,
                    order_id,
                    CalendarDate::parse(&day).ok_or_else(|| {
                        RepositoryError::Storage(format!("stored day {day} is invalid"))
                    })?,
                    row.try_get("notes").map_err(storage_error)?,
                    items.remove(&raw).unwrap_or_default(),
                    schedules.remove(&raw).unwrap_or_default(),
                ))
            })
            .collect()
    }

    async fn find_receipt(
        &self,
        company_id: CompanyId,
        order_id: PurchaseOrderId,
        id: PurchaseReceiptId,
    ) -> Result<Option<PurchaseReceipt>, RepositoryError> {
        Ok(self
            .receipts_of(company_id, order_id)
            .await?
            .into_iter()
            .find(|made| made.id() == id))
    }
}

/// The orders of these rows, each with its lines in order: one query for all of them.
async fn with_lines(
    connection: &mut PgConnection,
    rows: Vec<PgRow>,
) -> Result<Vec<PurchaseOrder>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let line_rows = sqlx::query(
        "SELECT purchase_id, product_id, description, quantity::text AS quantity, \
         unit_price::text AS unit_price, discount::text AS discount \
         FROM purchase_order_items WHERE purchase_id = ANY($1) ORDER BY purchase_id, position",
    )
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut by_order: HashMap<Uuid, Vec<LineDraft>> = HashMap::new();
    for row in &line_rows {
        let purchase_id: Uuid = row.try_get("purchase_id").map_err(storage_error)?;
        by_order.entry(purchase_id).or_default().push(LineDraft {
            product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage_error)?),
            description: row.try_get("description").map_err(storage_error)?,
            quantity: row.try_get("quantity").map_err(storage_error)?,
            unit_price: row.try_get("unit_price").map_err(storage_error)?,
            discount: row.try_get("discount").map_err(storage_error)?,
        });
    }
    rows.iter()
        .map(|row| order_from_row(row, &mut by_order))
        .collect()
}

fn order_from_row(
    row: &PgRow,
    lines: &mut HashMap<Uuid, Vec<LineDraft>>,
) -> Result<PurchaseOrder, RepositoryError> {
    let raw_id: Uuid = row.try_get("id").map_err(storage_error)?;
    let id = PurchaseOrderId::from_uuid(raw_id);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let status: String = row.try_get("status").map_err(storage_error)?;
    let status = PurchaseStatus::parse(&status).ok_or_else(|| {
        RepositoryError::Storage(format!("stored purchase order with status {status:?}"))
    })?;
    let term: Option<Uuid> = row.try_get("payment_term_id").map_err(storage_error)?;
    let center: Option<Uuid> = row.try_get("cost_center_id").map_err(storage_error)?;
    let draft = PurchaseOrderDraft {
        supplier_id: PartyId::from_uuid(row.try_get("supplier_id").map_err(storage_error)?),
        ordered_on: row.try_get("ordered_on").map_err(storage_error)?,
        expected_on: row.try_get("expected_on").map_err(storage_error)?,
        payment_term_id: term.map(PaymentTermId::from_uuid),
        category_id: CategoryId::from_uuid(row.try_get("category_id").map_err(storage_error)?),
        cost_center_id: center.map(CostCenterId::from_uuid),
        notes: row.try_get("notes").map_err(storage_error)?,
        lines: lines.remove(&raw_id).unwrap_or_default(),
    };
    let number: i64 = row.try_get("number").map_err(storage_error)?;
    let invoice_key: Option<String> = row.try_get("invoice_key").map_err(storage_error)?;
    let invoice_key = invoice_key
        .map(|key| {
            AccessKey::parse(&key).map_err(|_| {
                RepositoryError::Storage(format!("stored purchase order with key {key:?}"))
            })
        })
        .transpose()?;
    PurchaseOrder::new(id, company_id, draft)
        .map(|order| order.restored(number, status).with_invoice_key(invoice_key))
        .map_err(|error| {
            RepositoryError::Storage(format!("stored purchase order {id:?} is invalid: {error}"))
        })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{FinancialEntryService, PurchaseService};

    use conta_verde_application::StockService;

    use super::*;
    use crate::{
        PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
        PostgresFinancialEntryRepository, PostgresPartyRepository, PostgresPaymentMethodRepository,
        PostgresPaymentTermRepository, PostgresProductRepository, RegisterDocumentReferences,
        RegisterReferences, contract,
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

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_purchase_lifecycle() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
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
        let store = PostgresPurchaseOrderRepository::new(pool.clone());
        let purchases = PurchaseService::new(
            PostgresPurchaseOrderRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            ),
            references(&pool),
            document_references(&pool),
            StockService::new(
                PostgresStockMovementRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresStockSettingsRepository::new(pool.clone()),
            ),
        );
        let entries = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        contract::runs_the_purchase_lifecycle(
            &purchases,
            &store,
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
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let other_seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            other_company,
        )
        .await;
        let order_id = Uuid::new_v4();
        let entry_seed = contract::seed_entry_records(
            &PostgresCategoryRepository::new(pool.clone()),
            &PostgresCostCenterRepository::new(pool.clone()),
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresBankAccountRepository::new(pool.clone()),
            &PostgresPaymentMethodRepository::new(pool.clone()),
            company,
        )
        .await;
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        // A supplier of another company is refused by the composite foreign key.
        let result = sqlx::query(
            "INSERT INTO purchase_orders (id, company_id, number, supplier_id, ordered_on, \
             category_id, status) VALUES ($1, $2, 1, $3, DATE '2026-11-06', $4, 'draft')",
        )
        .bind(order_id)
        .bind(company.as_uuid())
        .bind(other_seed.customer.as_uuid())
        .bind(entry_seed.category.as_uuid())
        .execute(&pool)
        .await;
        assert_eq!(
            refused_by(result).as_deref(),
            Some("purchase_orders_supplier_fk")
        );
        // What the domain refuses, the check refuses: a status that is none, an expected day before the order.
        for (status, expected, constraint) in [
            ("shipped", "2026-11-07", "purchase_orders_status_valid"),
            ("draft", "2026-11-05", "purchase_orders_expected_on_valid"),
        ] {
            let result = sqlx::query(
                "INSERT INTO purchase_orders (id, company_id, number, supplier_id, ordered_on, \
                 expected_on, category_id, status) \
                 VALUES ($1, $2, 1, $3, DATE '2026-11-06', CAST($4 AS DATE), $5, $6)",
            )
            .bind(Uuid::new_v4())
            .bind(company.as_uuid())
            .bind(entry_seed.party.as_uuid())
            .bind(expected)
            .bind(entry_seed.category.as_uuid())
            .bind(status)
            .execute(&pool)
            .await;
            assert_eq!(refused_by(result).as_deref(), Some(constraint));
        }
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn enters_a_supplier_invoice_once() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
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
        let stock_of = || {
            StockService::new(
                PostgresStockMovementRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresStockSettingsRepository::new(pool.clone()),
            )
        };
        let purchases = PurchaseService::new(
            PostgresPurchaseOrderRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(&pool),
            ),
            references(&pool),
            document_references(&pool),
            stock_of(),
        );
        let entries = FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(&pool),
        );
        contract::enters_a_supplier_invoice_once(
            &purchases,
            &stock_of(),
            &entries,
            company,
            &entry_seed,
            &seed,
        )
        .await;
    }
}
