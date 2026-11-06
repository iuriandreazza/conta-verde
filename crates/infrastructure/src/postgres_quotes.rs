use std::collections::HashMap;

use conta_verde_application::{
    LinkChange, NumberCursor, NumberSort, QuoteQuery, QuoteRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    Actor, AuditEntity, AuditEntry, AuditRecord, CompanyId, LineDraft, PartyId, PaymentTermId,
    ProductId, Quote, QuoteDraft, QuoteId, QuoteStatus, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::postgres::{next_document_number, snapshot, storage_error};
use crate::postgres_audit;

const SELECT_QUOTES: &str = "SELECT id, company_id, number, customer_id, \
     to_char(issued_on, 'YYYY-MM-DD') AS issued_on, to_char(valid_until, 'YYYY-MM-DD') AS valid_until, \
     payment_term_id, notes, status, sent_by_user_id, sent_by_name FROM quotes";

pub struct PostgresQuoteRepository {
    pool: PgPool,
}

impl PostgresQuoteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Writes the lines of a quote, in order, on the connection of the transaction that writes the quote:
/// they commit with it or not at all.
async fn insert_lines(connection: &mut PgConnection, quote: &Quote) -> Result<(), RepositoryError> {
    for (index, line) in quote.lines().iter().enumerate() {
        sqlx::query(
            "INSERT INTO quote_items \
             (company_id, quote_id, position, product_id, description, quantity, unit_price, discount) \
             VALUES ($1, $2, $3, $4, $5, CAST($6 AS NUMERIC), CAST($7 AS NUMERIC), CAST($8 AS NUMERIC))",
        )
        .bind(quote.company_id().as_uuid())
        .bind(quote.id().as_uuid())
        .bind(i16::try_from(index + 1).map_err(|_| {
            RepositoryError::Storage("a quote has too many lines".to_owned())
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

fn sent_by(quote: &Quote) -> (Option<Uuid>, Option<&str>) {
    match quote.sent_by() {
        Some(Actor::User { id, name }) => (Some(id.as_uuid()), Some(name)),
        None => (None, None),
    }
}

impl QuoteRepository for PostgresQuoteRepository {
    async fn save(&self, quote: &Quote, audit: &AuditEntry) -> Result<i64, RepositoryError> {
        ensure_audit_describes(audit, quote)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let number = next_document_number(&mut tx, quote.company_id(), "quote").await?;
        let (user_id, user_name) = sent_by(quote);
        sqlx::query(
            "INSERT INTO quotes (id, company_id, number, customer_id, issued_on, valid_until, \
             payment_term_id, notes, status, sent_by_user_id, sent_by_name) \
             VALUES ($1, $2, $3, $4, CAST($5 AS DATE), CAST($6 AS DATE), $7, $8, $9, $10, $11)",
        )
        .bind(quote.id().as_uuid())
        .bind(quote.company_id().as_uuid())
        .bind(number)
        .bind(quote.customer_id().as_uuid())
        .bind(quote.issued_on().to_string())
        .bind(quote.valid_until().to_string())
        .bind(quote.payment_term_id().map(|id| id.as_uuid()))
        .bind(quote.notes())
        .bind(quote.status().as_str())
        .bind(user_id)
        .bind(user_name)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        insert_lines(&mut tx, quote).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(number)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: QuoteId,
    ) -> Result<Option<Quote>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_QUOTES} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut quotes = with_lines(&mut tx, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(quotes.pop())
    }

    async fn find_by_link(&self, digest: &str) -> Result<Option<Quote>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!("{SELECT_QUOTES} WHERE link_digest = $1"))
            .bind(digest)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut quotes = with_lines(&mut tx, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(quotes.pop())
    }

    async fn update(
        &self,
        quote: &Quote,
        expected: QuoteStatus,
        link: LinkChange,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, quote)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let (user_id, user_name) = sent_by(quote);
        let (mode, digest) = match &link {
            LinkChange::Keep => ("keep", None),
            LinkChange::Revoke => ("revoke", None),
            LinkChange::Set(digest) => ("set", Some(digest.as_str())),
        };
        let result = sqlx::query(
            "UPDATE quotes SET customer_id = $3, issued_on = CAST($4 AS DATE), \
             valid_until = CAST($5 AS DATE), payment_term_id = $6, notes = $7, status = $8, \
             sent_by_user_id = $9, sent_by_name = $10, \
             link_digest = CASE $11 WHEN 'keep' THEN link_digest WHEN 'revoke' THEN NULL ELSE $12 END \
             WHERE id = $1 AND company_id = $2 AND status = $13",
        )
        .bind(quote.id().as_uuid())
        .bind(quote.company_id().as_uuid())
        .bind(quote.customer_id().as_uuid())
        .bind(quote.issued_on().to_string())
        .bind(quote.valid_until().to_string())
        .bind(quote.payment_term_id().map(|id| id.as_uuid()))
        .bind(quote.notes())
        .bind(quote.status().as_str())
        .bind(user_id)
        .bind(user_name)
        .bind(mode)
        .bind(digest)
        .bind(expected.as_str())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            let exists: Option<i32> =
                sqlx::query_scalar("SELECT 1 FROM quotes WHERE id = $1 AND company_id = $2")
                    .bind(quote.id().as_uuid())
                    .bind(quote.company_id().as_uuid())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(storage_error)?;
            return Err(if exists.is_some() {
                RepositoryError::QuoteChanged
            } else {
                RepositoryError::NotFound
            });
        }
        // The lines are replaced as a whole, in the same transaction as the quote and its audit entry.
        sqlx::query("DELETE FROM quote_items WHERE quote_id = $1 AND company_id = $2")
            .bind(quote.id().as_uuid())
            .bind(quote.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        insert_lines(&mut tx, quote).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: QuoteId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Quote, id.as_uuid()).await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &QuoteQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> Result<Vec<Quote>, RepositoryError> {
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_QUOTES);
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
            sql.push(" AND (number::text = ")
                .push_bind(text.to_owned())
                .push(" OR strpos(lower(coalesce(notes, '')), lower(")
                .push_bind(text.to_owned())
                .push(
                    ")) > 0 OR EXISTS (SELECT 1 FROM quote_items i WHERE i.quote_id = quotes.id \
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
        let quotes = with_lines(&mut tx, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(quotes)
    }
}

/// The quotes of these rows, each with its lines in order: one query for all of them.
async fn with_lines(
    connection: &mut PgConnection,
    rows: Vec<PgRow>,
) -> Result<Vec<Quote>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let line_rows = sqlx::query(
        "SELECT quote_id, product_id, description, quantity::text AS quantity, \
         unit_price::text AS unit_price, discount::text AS discount \
         FROM quote_items WHERE quote_id = ANY($1) ORDER BY quote_id, position",
    )
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    let mut by_quote: HashMap<Uuid, Vec<LineDraft>> = HashMap::new();
    for row in &line_rows {
        let quote_id: Uuid = row.try_get("quote_id").map_err(storage_error)?;
        by_quote.entry(quote_id).or_default().push(LineDraft {
            product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage_error)?),
            description: row.try_get("description").map_err(storage_error)?,
            quantity: row.try_get("quantity").map_err(storage_error)?,
            unit_price: row.try_get("unit_price").map_err(storage_error)?,
            discount: row.try_get("discount").map_err(storage_error)?,
        });
    }
    rows.iter()
        .map(|row| quote_from_row(row, &mut by_quote))
        .collect()
}

fn quote_from_row(
    row: &PgRow,
    lines: &mut HashMap<Uuid, Vec<LineDraft>>,
) -> Result<Quote, RepositoryError> {
    let raw_id: Uuid = row.try_get("id").map_err(storage_error)?;
    let id = QuoteId::from_uuid(raw_id);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let status: String = row.try_get("status").map_err(storage_error)?;
    let status = QuoteStatus::parse(&status)
        .ok_or_else(|| RepositoryError::Storage(format!("stored quote with status {status:?}")))?;
    let term: Option<Uuid> = row.try_get("payment_term_id").map_err(storage_error)?;
    let draft = QuoteDraft {
        customer_id: PartyId::from_uuid(row.try_get("customer_id").map_err(storage_error)?),
        issued_on: row.try_get("issued_on").map_err(storage_error)?,
        valid_until: row.try_get("valid_until").map_err(storage_error)?,
        payment_term_id: term.map(PaymentTermId::from_uuid),
        notes: row.try_get("notes").map_err(storage_error)?,
        lines: lines.remove(&raw_id).unwrap_or_default(),
    };
    let user: Option<Uuid> = row.try_get("sent_by_user_id").map_err(storage_error)?;
    let name: Option<String> = row.try_get("sent_by_name").map_err(storage_error)?;
    let sent_by = user.zip(name).map(|(id, name)| Actor::User {
        id: UserId::from_uuid(id),
        name,
    });
    let number: i64 = row.try_get("number").map_err(storage_error)?;
    Quote::new(id, company_id, draft)
        .map(|quote| quote.restored(number, status, sent_by))
        .map_err(|error| {
            RepositoryError::Storage(format!("stored quote {id:?} is invalid: {error}"))
        })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::QuoteService;

    use super::*;
    use crate::{
        Argon2SecretService, InMemoryEmailSender, PostgresPartyRepository,
        PostgresPaymentTermRepository, PostgresProductRepository, RegisterSalesReferences,
        contract,
    };

    fn references(
        pool: &PgPool,
    ) -> RegisterSalesReferences<
        PostgresPartyRepository,
        PostgresProductRepository,
        PostgresPaymentTermRepository,
    > {
        RegisterSalesReferences::new(
            PostgresPartyRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresPaymentTermRepository::new(pool.clone()),
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_quote_lifecycle() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            company,
        )
        .await;
        contract::sales_references_say_what_is_usable(
            &references(&pool),
            company,
            other_company,
            &seed,
        )
        .await;
        let emails = InMemoryEmailSender::default();
        let service = QuoteService::new(
            PostgresQuoteRepository::new(pool.clone()),
            references(&pool),
            emails.clone(),
            Argon2SecretService::new().unwrap(),
            "https://erp.example",
        );
        contract::runs_the_quote_lifecycle(&service, &emails, company, other_company, &seed).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_and_a_reference_to_another_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            company,
        )
        .await;
        let other_seed = contract::seed_sales_records(
            &PostgresPartyRepository::new(pool.clone()),
            &PostgresProductRepository::new(pool.clone()),
            &PostgresPaymentTermRepository::new(pool.clone()),
            other_company,
        )
        .await;
        fn refused_by<T>(result: Result<T, sqlx::Error>) -> Option<String> {
            match result {
                Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
                _ => None,
            }
        }
        let insert_quote = |customer: Uuid,
                            number: i64,
                            valid_until: &'static str,
                            status: &'static str,
                            link: Option<&'static str>,
                            notes: Option<&'static str>| {
            let pool = pool.clone();
            let id = Uuid::new_v4();
            async move {
                sqlx::query(
                    "INSERT INTO quotes (id, company_id, number, customer_id, issued_on, valid_until, \
                     notes, status, link_digest) \
                     VALUES ($1, $2, $3, $4, DATE '2026-11-06', CAST($5 AS DATE), $6, $7, $8)",
                )
                .bind(id)
                .bind(company.as_uuid())
                .bind(number)
                .bind(customer)
                .bind(valid_until)
                .bind(notes)
                .bind(status)
                .bind(link)
                .execute(&pool)
                .await
                .map(|_| id)
            }
        };
        let customer = seed.customer.as_uuid();
        for (number, valid_until, status, link, notes, constraint) in [
            (0, "2026-11-20", "draft", None, None, "quotes_number_valid"),
            (1, "2026-11-05", "draft", None, None, "quotes_dates_valid"),
            (1, "2026-11-20", "open", None, None, "quotes_status_valid"),
            (
                1,
                "2026-11-20",
                "draft",
                None,
                Some(" x"),
                "quotes_notes_valid",
            ),
            (
                1,
                "2026-11-20",
                "draft",
                Some("abc"),
                None,
                "quotes_link_valid",
            ),
        ] {
            assert_eq!(
                refused_by(insert_quote(customer, number, valid_until, status, link, notes).await)
                    .as_deref(),
                Some(constraint),
                "{number} {valid_until} {status} {link:?} {notes:?}"
            );
        }
        assert_eq!(
            refused_by(
                insert_quote(
                    other_seed.customer.as_uuid(),
                    1,
                    "2026-11-20",
                    "draft",
                    None,
                    None
                )
                .await
            )
            .as_deref(),
            Some("quotes_customer_fk")
        );
        let id = insert_quote(customer, 1, "2026-11-20", "draft", None, None)
            .await
            .unwrap();
        assert_eq!(
            refused_by(insert_quote(customer, 1, "2026-11-20", "draft", None, None).await)
                .as_deref(),
            Some("quotes_company_number_key")
        );

        let insert_item = |product: Uuid,
                           quantity: &'static str,
                           price: &'static str,
                           discount: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO quote_items (company_id, quote_id, position, product_id, description, \
                     quantity, unit_price, discount) \
                     VALUES ($1, $2, 1, $3, 'Café', CAST($4 AS NUMERIC), CAST($5 AS NUMERIC), CAST($6 AS NUMERIC))",
                )
                .bind(company.as_uuid())
                .bind(id)
                .bind(product)
                .bind(quantity)
                .bind(price)
                .bind(discount)
                .execute(&pool)
                .await
            }
        };
        let product = seed.product.as_uuid();
        for (quantity, price, discount, constraint) in [
            ("0", "1", "0", "quote_items_quantity_valid"),
            ("1.0001", "1", "0", "quote_items_quantity_valid"),
            ("1", "1.0000001", "0", "quote_items_unit_price_valid"),
            ("1", "10", "0.001", "quote_items_discount_valid"),
            // The gross is 4.995, which rounds up to 5, so a discount of 5 is the most there is.
            ("1.5", "3.33", "5.01", "quote_items_discount_valid"),
        ] {
            assert_eq!(
                refused_by(insert_item(product, quantity, price, discount).await).as_deref(),
                Some(constraint),
                "{quantity} {price} {discount}"
            );
        }
        assert_eq!(
            refused_by(insert_item(other_seed.product.as_uuid(), "1", "1", "0").await).as_deref(),
            Some("quote_items_product_fk")
        );
        assert!(insert_item(product, "1.5", "3.33", "5").await.is_ok());
    }
}
