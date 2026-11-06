use std::collections::HashMap;

use crate::postgres_audit;
use conta_verde_application::{
    NameCursor, NameSort, PartyKind, PartyQuery, PartyRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AddressDraft, AuditEntity, AuditEntry, AuditRecord, CompanyId, ContactDraft, DocumentCheck,
    Party, PartyDraft, PartyId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

const DOCUMENT_UNIQUE_INDEX: &str = "parties_company_document_key";

pub async fn connect_and_migrate(
    database_url: &str,
) -> Result<PgPool, Box<dyn std::error::Error + Send + Sync>> {
    let pool = PgPool::connect(database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub(crate) fn storage_error(error: sqlx::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// The end of every list query (ADR 0018): resume right after the cursor, in the order of the
/// sort, and stop at `limit`. The statement must already end in a `WHERE` that has a condition.
pub(crate) fn push_name_keyset(
    sql: &mut QueryBuilder<'_, Postgres>,
    sort: NameSort,
    after: Option<&NameCursor>,
    limit: usize,
) {
    push_text_keyset(sql, "name", sort, after, limit);
}

/// The same keyset over another text column (`column` is fixed text, never a value from outside).
pub(crate) fn push_text_keyset(
    sql: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    sort: NameSort,
    after: Option<&NameCursor>,
    limit: usize,
) {
    let (comparison, direction) = match sort {
        NameSort::NameAsc => (">", "ASC"),
        NameSort::NameDesc => ("<", "DESC"),
    };
    if let Some(cursor) = after {
        sql.push(format!(" AND (lower({column}), id) {comparison} (lower("))
            .push_bind(cursor.name.clone())
            .push("), ")
            .push_bind(cursor.id)
            .push(")");
    }
    sql.push(format!(
        " ORDER BY lower({column}) {direction}, id {direction} LIMIT "
    ))
    .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
}

/// A transaction that sees one moment of the database. Records with children (a payment term's
/// installments, a party's contacts and tags) are read in several statements, and a write
/// committed between them would otherwise give a record that never existed: the old name with the
/// new children.
pub(crate) async fn snapshot(
    pool: &PgPool,
) -> Result<sqlx::Transaction<'static, Postgres>, RepositoryError> {
    let mut tx = pool.begin().await.map_err(storage_error)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
    Ok(tx)
}

pub struct PostgresPartyRepository {
    pool: PgPool,
}

impl PostgresPartyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const SELECT_PARTIES: &str = "SELECT id, company_id, name, document, email, is_customer, is_supplier, \
     is_carrier, is_seller, postal_code, street, street_number, complement, district, city, state, \
     document_check, registry_legal_name, registry_situation \
     FROM parties";

impl PartyRepository for PostgresPartyRepository {
    async fn save(&self, party: &Party, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, party)?;
        // One transaction: if the party is refused (a duplicate document) or the audit row cannot
        // be written, the rollback leaves neither behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        bind_party(
            sqlx::query(
                "INSERT INTO parties (id, company_id, name, document, email, is_customer, \
                 is_supplier, is_carrier, is_seller, postal_code, street, street_number, \
                 complement, district, city, state, document_check, registry_legal_name, \
                 registry_situation) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, \
                 $17, $18, $19)",
            ),
            party,
        )
        .execute(&mut *tx)
        .await
        .map_err(duplicate_document_or_storage_error)?;
        insert_children(&mut tx, party).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(&self.pool, company_id, AuditEntity::Party, id.as_uuid()).await
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<Option<Party>, RepositoryError> {
        let mut tx = snapshot(&self.pool).await?;
        let row = sqlx::query(&format!(
            "{SELECT_PARTIES} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        let Some(row) = row else { return Ok(None) };
        let mut parties = with_children(&mut tx, company_id, vec![row]).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(parties.pop())
    }

    async fn update(&self, party: &Party, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, party)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = bind_party(
            sqlx::query(
                "UPDATE parties SET name = $3, document = $4, email = $5, is_customer = $6, \
                 is_supplier = $7, is_carrier = $8, is_seller = $9, postal_code = $10, \
                 street = $11, street_number = $12, complement = $13, district = $14, \
                 city = $15, state = $16, document_check = $17, registry_legal_name = $18, \
                 registry_situation = $19 \
                 WHERE company_id = $2 AND id = $1",
            ),
            party,
        )
        .execute(&mut *tx)
        .await
        .map_err(duplicate_document_or_storage_error)?;
        // Matching no row means the party is not in that company: report it, do not pretend. The
        // transaction is dropped, so no audit row is left for a write that did not happen.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        // The contacts and the tags are replaced as a whole, inside the same transaction as the
        // party and its audit entry: a refusal anywhere leaves the old ones in place.
        for table in ["party_contacts", "party_tags"] {
            sqlx::query(&format!(
                "DELETE FROM {table} WHERE party_id = $1 AND company_id = $2"
            ))
            .bind(party.id().as_uuid())
            .bind(party.company_id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        insert_children(&mut tx, party).await?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PartyQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<Party>, RepositoryError> {
        // Every value is bound, never spliced into the text: the pieces pushed as text are fixed.
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_PARTIES);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        match query.kind() {
            Some(PartyKind::Customer) => sql.push(" AND is_customer"),
            Some(PartyKind::Supplier) => sql.push(" AND is_supplier"),
            Some(PartyKind::Carrier) => sql.push(" AND is_carrier"),
            Some(PartyKind::Seller) => sql.push(" AND is_seller"),
            None => &mut sql,
        };
        if let Some(tag) = query.tag() {
            sql.push(
                " AND EXISTS (SELECT 1 FROM party_tags WHERE party_id = parties.id AND tag = ",
            )
            .push_bind(tag.as_str())
            .push(")");
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
            sql.push(" AND (strpos(lower(name), lower(")
                .push_bind(text)
                .push(")) > 0 OR strpos(lower(coalesce(email, '')), lower(")
                .push_bind(text)
                .push(")) > 0");
            if let Some(document) = query.document_search() {
                sql.push(" OR strpos(coalesce(document, ''), ")
                    .push_bind(document)
                    .push(") > 0");
            }
            sql.push(")");
        }
        push_name_keyset(&mut sql, query.sort(), after, limit);

        let mut tx = snapshot(&self.pool).await?;
        let rows = sql
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(storage_error)?;
        let parties = with_children(&mut tx, company_id, rows).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(parties)
    }
}

/// Writes the contacts, in order, and the tags of a party on the connection of the transaction that
/// writes the party: they commit with it or not at all.
async fn insert_children(
    connection: &mut PgConnection,
    party: &Party,
) -> Result<(), RepositoryError> {
    for (index, contact) in party.contacts().iter().enumerate() {
        sqlx::query(
            "INSERT INTO party_contacts (company_id, party_id, position, name, job_title, email, \
             phone) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(party.company_id().as_uuid())
        .bind(party.id().as_uuid())
        .bind(
            i16::try_from(index + 1).map_err(|_| {
                RepositoryError::Storage("a party has too many contacts".to_owned())
            })?,
        )
        .bind(contact.name())
        .bind(contact.job_title())
        .bind(contact.email().map(|email| email.as_str()))
        .bind(contact.phone())
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
    }
    for tag in party.tags() {
        sqlx::query("INSERT INTO party_tags (company_id, party_id, tag) VALUES ($1, $2, $3)")
            .bind(party.company_id().as_uuid())
            .bind(party.id().as_uuid())
            .bind(tag.as_str())
            .execute(&mut *connection)
            .await
            .map_err(storage_error)?;
    }
    Ok(())
}

#[derive(Default)]
struct PartyChildren {
    contacts: Vec<ContactDraft>,
    tags: Vec<String>,
}

/// The parties of these rows, each with its contacts and tags: two queries for all of them.
async fn with_children(
    connection: &mut PgConnection,
    company_id: CompanyId,
    rows: Vec<PgRow>,
) -> Result<Vec<Party>, RepositoryError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id").map_err(storage_error))
        .collect::<Result<_, _>>()?;
    let mut children: HashMap<Uuid, PartyChildren> = HashMap::new();
    let contacts = sqlx::query(
        "SELECT party_id, name, job_title, email, phone FROM party_contacts \
         WHERE company_id = $1 AND party_id = ANY($2) ORDER BY party_id, position",
    )
    .bind(company_id.as_uuid())
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    for row in &contacts {
        let party_id: Uuid = row.try_get("party_id").map_err(storage_error)?;
        children
            .entry(party_id)
            .or_default()
            .contacts
            .push(ContactDraft {
                name: row.try_get("name").map_err(storage_error)?,
                job_title: row.try_get("job_title").map_err(storage_error)?,
                email: row.try_get("email").map_err(storage_error)?,
                phone: row.try_get("phone").map_err(storage_error)?,
            });
    }
    let tags = sqlx::query(
        "SELECT party_id, tag FROM party_tags \
         WHERE company_id = $1 AND party_id = ANY($2) ORDER BY party_id, tag",
    )
    .bind(company_id.as_uuid())
    .bind(&ids)
    .fetch_all(&mut *connection)
    .await
    .map_err(storage_error)?;
    for row in &tags {
        let party_id: Uuid = row.try_get("party_id").map_err(storage_error)?;
        children
            .entry(party_id)
            .or_default()
            .tags
            .push(row.try_get("tag").map_err(storage_error)?);
    }
    rows.iter()
        .zip(ids)
        .map(|(row, id)| party_from_row(row, children.remove(&id).unwrap_or_default()))
        .collect()
}

type PartyQueryBuilder<'q> = sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>;

/// Binds the columns `$1` to `$19` of the INSERT and the UPDATE, which list them in the same order.
fn bind_party<'q>(query: PartyQueryBuilder<'q>, party: &'q Party) -> PartyQueryBuilder<'q> {
    let address = party.address();
    query
        .bind(party.id().as_uuid())
        .bind(party.company_id().as_uuid())
        .bind(party.name())
        .bind(party.document().map(|document| document.as_str()))
        .bind(party.email().map(|email| email.as_str()))
        .bind(party.is_customer())
        .bind(party.is_supplier())
        .bind(party.is_carrier())
        .bind(party.is_seller())
        .bind(address.map(|address| address.postal_code()))
        .bind(address.map(|address| address.street()))
        .bind(address.and_then(|address| address.number()))
        .bind(address.and_then(|address| address.complement()))
        .bind(address.and_then(|address| address.district()))
        .bind(address.map(|address| address.city()))
        .bind(address.map(|address| address.state()))
        .bind(check_kind(party.document_check()))
        .bind(registry_legal_name(party.document_check()))
        .bind(registry_situation(party.document_check()))
}

fn duplicate_document_or_storage_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(db) if db.constraint() == Some(DOCUMENT_UNIQUE_INDEX) => {
            RepositoryError::DuplicateDocument
        }
        _ => storage_error(error),
    }
}

fn address_from_row(row: &PgRow) -> Result<Option<AddressDraft>, RepositoryError> {
    let postal_code: Option<String> = row.try_get("postal_code").map_err(storage_error)?;
    let street: Option<String> = row.try_get("street").map_err(storage_error)?;
    let city: Option<String> = row.try_get("city").map_err(storage_error)?;
    let state: Option<String> = row.try_get("state").map_err(storage_error)?;
    // The table's CHECK makes the four go together; a row without them has no address.
    let (Some(postal_code), Some(street), Some(city), Some(state)) =
        (postal_code, street, city, state)
    else {
        return Ok(None);
    };
    Ok(Some(AddressDraft {
        postal_code,
        street,
        number: row.try_get("street_number").map_err(storage_error)?,
        complement: row.try_get("complement").map_err(storage_error)?,
        district: row.try_get("district").map_err(storage_error)?,
        city,
        state,
    }))
}

fn party_from_row(row: &PgRow, children: PartyChildren) -> Result<Party, RepositoryError> {
    let draft = PartyDraft {
        name: row.try_get("name").map_err(storage_error)?,
        document: row.try_get("document").map_err(storage_error)?,
        email: row.try_get("email").map_err(storage_error)?,
        is_customer: row.try_get("is_customer").map_err(storage_error)?,
        is_supplier: row.try_get("is_supplier").map_err(storage_error)?,
        is_carrier: row.try_get("is_carrier").map_err(storage_error)?,
        is_seller: row.try_get("is_seller").map_err(storage_error)?,
        address: address_from_row(row)?,
        contacts: children.contacts,
        tags: children.tags,
    };
    let id = PartyId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);

    let check = document_check_from_row(row)?;

    Party::new(id, company_id, draft)
        .map(|party| party.with_document_check(check))
        .map_err(|error| {
            RepositoryError::Storage(format!("stored party {id:?} is invalid: {error}"))
        })
}

fn check_kind(check: &DocumentCheck) -> &'static str {
    match check {
        DocumentCheck::NotApplicable => "not_applicable",
        DocumentCheck::Verified { .. } => "verified",
        DocumentCheck::NotFound => "not_found",
        DocumentCheck::Unverified => "unverified",
    }
}

fn registry_legal_name(check: &DocumentCheck) -> Option<&str> {
    match check {
        DocumentCheck::Verified { legal_name, .. } => Some(legal_name),
        _ => None,
    }
}

fn registry_situation(check: &DocumentCheck) -> Option<&str> {
    match check {
        DocumentCheck::Verified { situation, .. } => Some(situation),
        _ => None,
    }
}

fn document_check_from_row(row: &PgRow) -> Result<DocumentCheck, RepositoryError> {
    let kind: String = row.try_get("document_check").map_err(storage_error)?;
    let legal_name: Option<String> = row.try_get("registry_legal_name").map_err(storage_error)?;
    let situation: Option<String> = row.try_get("registry_situation").map_err(storage_error)?;
    match (kind.as_str(), legal_name, situation) {
        ("not_applicable", None, None) => Ok(DocumentCheck::NotApplicable),
        ("not_found", None, None) => Ok(DocumentCheck::NotFound),
        ("unverified", None, None) => Ok(DocumentCheck::Unverified),
        ("verified", Some(legal_name), Some(situation)) => Ok(DocumentCheck::Verified {
            legal_name,
            situation,
        }),
        // The table's CHECK constraints rule this out; reading it anyway would hide corrupt data.
        (kind, ..) => Err(RepositoryError::Storage(format!(
            "stored document check `{kind}` is inconsistent"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{ApplicationError, PartyService};

    use super::*;
    use crate::{InMemoryCnpjRegistry, contract};

    // Needs a running database: `docker compose up -d db` then `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_parties_and_enforces_unique_document_per_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = PartyService::new(PostgresPartyRepository::new(pool), contract::registry());
        let draft = |name: &str, document: Option<&str>| PartyDraft {
            name: name.into(),
            document: document.map(Into::into),
            email: Some("contato@example.com".into()),
            is_customer: true,
            is_supplier: true,
            ..PartyDraft::default()
        };

        service
            .register(
                company,
                &contract::actor(),
                draft("Padaria", Some("11.222.333/0001-81")),
            )
            .await
            .unwrap();
        let duplicate = service
            .register(
                company,
                &contract::actor(),
                draft("Padaria 2", Some("11.222.333/0001-81")),
            )
            .await;
        assert_eq!(
            duplicate.unwrap_err(),
            ApplicationError::Repository(RepositoryError::DuplicateDocument)
        );

        service
            .register(company, &contract::actor(), draft("zeca", None))
            .await
            .unwrap();
        service
            .register(company, &contract::actor(), draft("ana", None))
            .await
            .unwrap();

        let listed = service
            .list(company, &PartyQuery::default(), None, None)
            .await
            .unwrap()
            .items;
        let names: Vec<&str> = listed.iter().map(Party::name).collect();
        assert_eq!(names, ["ana", "Padaria", "zeca"]);
        assert_eq!(listed[1].document().unwrap().as_str(), "11222333000181");
        assert!(listed[1].is_customer() && listed[1].is_supplier());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn edits_parties_and_records_only_changes() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = PartyService::new(PostgresPartyRepository::new(pool), contract::registry());
        contract::edits_parties_and_records_only_changes(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn imports_parties_and_records_each_creation() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = PartyService::new(PostgresPartyRepository::new(pool), contract::registry());
        contract::imports_parties_and_records_each_creation(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_party_contacts_and_tags() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = PartyService::new(PostgresPartyRepository::new(pool), contract::registry());
        contract::keeps_party_contacts_and_tags(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_in_contacts_and_tags() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let party = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO parties (id, company_id, name, is_customer, is_supplier, is_carrier, \
             is_seller, document_check) VALUES ($1, $2, 'Teste', true, false, false, false, \
             'not_applicable')",
        )
        .bind(party)
        .bind(company.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let tag = |tag: String, company: Uuid| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO party_tags (company_id, party_id, tag) VALUES ($1, $2, $3)",
                )
                .bind(company)
                .bind(party)
                .bind(tag)
                .execute(&pool)
                .await
            }
        };
        let mine = company.as_uuid();
        for refused in [
            "".to_owned(),
            "VIP".to_owned(),
            " vip".to_owned(),
            "vip\t".to_owned(),
            "\n".to_owned(),
            "a,b".to_owned(),
            "a\u{1}b".to_owned(),
            "a\u{85}b".to_owned(),
            "a".repeat(31),
        ] {
            assert_eq!(
                refused_by(tag(refused.clone(), mine).await).as_deref(),
                Some("party_tags_tag_valid"),
                "{refused:?}"
            );
        }
        // What the domain accepts, the database accepts: accents and a space inside.
        for accepted in [
            "vip",
            "distribuição",
            "água mineral",
            "istanbul",
            &"a".repeat(30),
        ] {
            tag(accepted.to_owned(), mine).await.unwrap();
        }
        assert_eq!(
            refused_by(tag("vip".into(), mine).await).as_deref(),
            Some("party_tags_pkey")
        );
        // The tag must be in its party's company.
        assert_eq!(
            refused_by(tag("novo".into(), Uuid::new_v4()).await).as_deref(),
            Some("party_tags_party_fk")
        );

        let contact = |position: i16,
                       name: &'static str,
                       job_title: Option<&'static str>,
                       email: Option<&'static str>,
                       phone: Option<&'static str>| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO party_contacts (company_id, party_id, position, name, job_title, \
                     email, phone) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(mine)
                .bind(party)
                .bind(position)
                .bind(name)
                .bind(job_title)
                .bind(email)
                .bind(phone)
                .execute(&pool)
                .await
            }
        };
        let cases = [
            (
                contact(0, "Ana", None, None, None).await,
                "party_contacts_position_valid",
            ),
            (
                contact(21, "Ana", None, None, None).await,
                "party_contacts_position_valid",
            ),
            (
                contact(1, "", None, None, None).await,
                "party_contacts_text_valid",
            ),
            (
                contact(1, "\tAna", None, None, None).await,
                "party_contacts_text_valid",
            ),
            (
                contact(1, "Ana\n", None, None, None).await,
                "party_contacts_text_valid",
            ),
            (
                contact(1, "Ana", Some(" "), None, None).await,
                "party_contacts_text_valid",
            ),
            (
                contact(1, "Ana", Some(""), None, None).await,
                "party_contacts_text_valid",
            ),
            (
                contact(1, "Ana", None, Some("Ana@x.com"), None).await,
                "party_contacts_email_valid",
            ),
            (
                contact(1, "Ana", None, Some("ana"), None).await,
                "party_contacts_email_valid",
            ),
            (
                contact(1, "Ana", None, Some("@x.com"), None).await,
                "party_contacts_email_valid",
            ),
            (
                contact(1, "Ana", None, Some("ana@x"), None).await,
                "party_contacts_email_valid",
            ),
            (
                contact(1, "Ana", None, Some(" ana@x.com"), None).await,
                "party_contacts_email_valid",
            ),
            (
                contact(1, "Ana", None, None, Some("")).await,
                "party_contacts_phone_valid",
            ),
            (
                contact(1, "Ana", None, None, Some("1234567")).await,
                "party_contacts_phone_valid",
            ),
            (
                contact(1, "Ana", None, None, Some("1234567890123456")).await,
                "party_contacts_phone_valid",
            ),
            (
                contact(1, "Ana", None, None, Some("ligar 12345678")).await,
                "party_contacts_phone_valid",
            ),
            (
                contact(1, "Ana", None, None, Some(" 12345678")).await,
                "party_contacts_phone_valid",
            ),
            (
                contact(1, "Ana", None, None, Some("+55 (81) 99999-0000 0000 0000")).await,
                "party_contacts_phone_valid",
            ),
        ];
        for (result, constraint) in cases {
            assert_eq!(refused_by(result).as_deref(), Some(constraint));
        }
        contact(
            1,
            "Ana",
            Some("Compras"),
            Some("ana@x.com"),
            Some("(81) 99999-0000"),
        )
        .await
        .unwrap();
        contact(2, "Bruno", None, None, None).await.unwrap();
        assert_eq!(
            refused_by(contact(1, "Carla", None, None, None).await).as_deref(),
            Some("party_contacts_pkey")
        );
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn pages_through_parties_in_name_order() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = PartyService::new(
            PostgresPartyRepository::new(pool),
            InMemoryCnpjRegistry::default(),
        );
        contract::pages_through_parties(&service, company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn breaks_ties_between_equal_names() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = PartyService::new(
            PostgresPartyRepository::new(pool),
            InMemoryCnpjRegistry::default(),
        );
        contract::breaks_ties_between_equal_names(&service, company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn filters_and_sorts_parties() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = PartyService::new(PostgresPartyRepository::new(pool), contract::registry());
        contract::filters_and_sorts_parties(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_what_the_registry_said() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = PartyService::new(PostgresPartyRepository::new(pool), contract::registry());
        contract::keeps_what_the_registry_said(&service, company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn keeps_the_unverified_mark() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = PartyService::new(
            PostgresPartyRepository::new(pool),
            InMemoryCnpjRegistry::unreachable(),
        );
        contract::keeps_the_unverified_mark(&service, company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_a_registry_check_on_anything_but_a_cnpj() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let insert = |document: Option<&'static str>, check: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO parties (id, company_id, name, document, is_customer, is_supplier, \
                     is_carrier, is_seller, document_check) \
                     VALUES (gen_random_uuid(), $1, 'Teste', $2, true, false, false, false, $3)",
                )
                .bind(company.as_uuid())
                .bind(document)
                .bind(check)
                .execute(&pool)
                .await
            }
        };

        // A CPF and a party without a document can only be 'not_applicable'.
        for check in ["unverified", "not_found"] {
            assert!(
                insert(Some("52998224725"), check).await.is_err(),
                "CPF as {check}"
            );
            assert!(insert(None, check).await.is_err(), "no document as {check}");
        }
        insert(Some("52998224725"), "not_applicable").await.unwrap();
        insert(None, "not_applicable").await.unwrap();
        // A CNPJ may carry any of them, including the status added by the second migration.
        insert(Some("11222333000181"), "not_found").await.unwrap();
        insert(Some("12ABC34501DE35"), "unverified").await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would_in_roles_and_addresses() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        // Roles: customer, supplier, carrier, seller; the address columns: CEP, street, number,
        // complement, district, city, state.
        let insert = |roles: [bool; 4], address: [Option<&'static str>; 7]| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO parties (id, company_id, name, is_customer, is_supplier, \
                     is_carrier, is_seller, postal_code, street, street_number, complement, \
                     district, city, state, document_check) \
                     VALUES (gen_random_uuid(), $1, 'Teste', $2, $3, $4, $5, $6, $7, $8, $9, $10, \
                     $11, $12, 'not_applicable')",
                )
                .bind(company.as_uuid())
                .bind(roles[0])
                .bind(roles[1])
                .bind(roles[2])
                .bind(roles[3])
                .bind(address[0])
                .bind(address[1])
                .bind(address[2])
                .bind(address[3])
                .bind(address[4])
                .bind(address[5])
                .bind(address[6])
                .execute(&pool)
                .await
            }
        };
        let none = [None; 7];
        let whole = [
            Some("01310100"),
            Some("Avenida Paulista"),
            None,
            None,
            None,
            Some("São Paulo"),
            Some("SP"),
        ];
        let with = |index: usize, value: Option<&'static str>| {
            let mut address = whole;
            address[index] = value;
            address
        };

        assert_eq!(
            refused_by(insert([false; 4], none).await).as_deref(),
            Some("parties_has_role")
        );
        // Any one role is enough, and the address is optional.
        for role in 0..4 {
            let mut roles = [false; 4];
            roles[role] = true;
            insert(roles, none).await.unwrap();
        }
        insert([true, false, false, false], whole).await.unwrap();

        let customer = [true, false, false, false];
        let cases = [
            (with(0, None), "parties_address_is_whole"),
            (with(1, None), "parties_address_is_whole"),
            (with(5, None), "parties_address_is_whole"),
            (with(6, None), "parties_address_is_whole"),
            // Only the number, the complement and the district alone are not an address.
            (
                [None, None, Some("10"), None, None, None, None],
                "parties_address_is_whole",
            ),
            (with(0, Some("0131010")), "parties_postal_code_valid"),
            (with(0, Some("01310-100")), "parties_postal_code_valid"),
            (with(6, Some("XX")), "parties_state_valid"),
            (with(6, Some("sp")), "parties_state_valid"),
            (with(1, Some("")), "parties_address_text_valid"),
            (with(1, Some(" Rua A")), "parties_address_text_valid"),
            // White space the domain trims but `btrim` does not: a tab, a line break, only blanks.
            (with(1, Some("\tRua A")), "parties_address_text_valid"),
            (with(1, Some("\t")), "parties_address_text_valid"),
            (with(5, Some("Recife\n")), "parties_address_text_valid"),
            (with(5, Some("Recife ")), "parties_address_text_valid"),
            (with(2, Some("")), "parties_address_text_valid"),
        ];
        for (address, constraint) in cases {
            assert_eq!(
                refused_by(insert(customer, address).await).as_deref(),
                Some(constraint),
                "{address:?}"
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn records_what_was_written() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresPartyRepository::new(pool);
        contract::records_what_was_written(&repository, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn leaves_no_trace_of_a_write_that_did_not_happen() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresPartyRepository::new(pool);
        contract::leaves_no_trace_of_a_write_that_did_not_happen(
            &repository,
            company,
            other_company,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn a_party_is_not_stored_when_its_audit_row_cannot_be_written() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let repository = PostgresPartyRepository::new(pool);
        let party = Party::new(
            PartyId::generate(),
            company,
            PartyDraft {
                name: "Padaria".into(),
                document: None,
                email: None,
                is_customer: true,
                is_supplier: false,
                ..PartyDraft::default()
            },
        )
        .unwrap();
        // The entry names a company that does not exist, so the audit insert violates its foreign
        // key *after* the party insert succeeded inside the transaction.
        let broken = AuditEntry {
            company_id: CompanyId::generate(),
            ..contract::created_entry(&party)
        };

        assert!(matches!(
            repository.save(&party, &broken).await,
            Err(RepositoryError::Storage(_))
        ));
        assert_eq!(repository.find(company, party.id()).await.unwrap(), None);
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn an_edit_refused_after_the_old_contacts_were_deleted_keeps_them() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        // A trigger that refuses one contact name makes the insert fail *after* the update of the
        // party and the delete of the old lists, inside the transaction of the edit.
        for statement in [
            "CREATE OR REPLACE FUNCTION refuse_boom_contact() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN IF NEW.name = 'Boom' THEN RAISE EXCEPTION 'boom'; END IF; RETURN NEW; END $$",
            "DROP TRIGGER IF EXISTS refuse_boom_contact ON party_contacts",
            "CREATE TRIGGER refuse_boom_contact BEFORE INSERT ON party_contacts \
             FOR EACH ROW EXECUTE FUNCTION refuse_boom_contact()",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        let service = PartyService::new(
            PostgresPartyRepository::new(pool.clone()),
            InMemoryCnpjRegistry::default(),
        );
        let contact = |name: &str| ContactDraft {
            name: name.into(),
            ..ContactDraft::default()
        };
        let draft = |name: &str, tags: &[&str], contacts: Vec<ContactDraft>| PartyDraft {
            name: name.into(),
            is_customer: true,
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            contacts,
            ..PartyDraft::default()
        };
        let party = service
            .register(
                company,
                &contract::actor(),
                draft("Padaria", &["vip"], vec![contact("Ana"), contact("Bruno")]),
            )
            .await
            .unwrap();

        let refused = service
            .update(
                company,
                &contract::actor(),
                party.id(),
                draft("Padaria Nova", &["novo"], vec![contact("Boom")]),
            )
            .await;
        sqlx::query("DROP TRIGGER refuse_boom_contact ON party_contacts")
            .execute(&pool)
            .await
            .unwrap();
        assert!(refused.is_err());

        let kept = service
            .list(company, &PartyQuery::default(), None, None)
            .await
            .unwrap()
            .items;
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].name(), "Padaria");
        assert_eq!(kept[0].tags().len(), 1);
        assert_eq!(kept[0].tags()[0].as_str(), "vip");
        let names: Vec<&str> = kept[0].contacts().iter().map(|c| c.name()).collect();
        assert_eq!(names, ["Ana", "Bruno"]);
        assert_eq!(service.history(company, party.id()).await.unwrap().len(), 1);
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_audit_log_cannot_be_changed_or_deleted() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let repository = PostgresPartyRepository::new(pool.clone());
        let party = Party::new(
            PartyId::generate(),
            company,
            PartyDraft {
                name: "Padaria".into(),
                document: None,
                email: None,
                is_customer: true,
                is_supplier: false,
                ..PartyDraft::default()
            },
        )
        .unwrap();
        repository
            .save(&party, &contract::created_entry(&party))
            .await
            .unwrap();

        // Scoped to this company's rows, so a missing trigger cannot touch another test's data.
        let update = sqlx::query("UPDATE audit_log SET actor_name = 'Eve' WHERE company_id = $1")
            .bind(company.as_uuid())
            .execute(&pool)
            .await;
        assert!(update.is_err(), "an audit row was updated");
        let delete = sqlx::query("DELETE FROM audit_log WHERE company_id = $1")
            .bind(company.as_uuid())
            .execute(&pool)
            .await;
        assert!(delete.is_err(), "an audit row was deleted");

        let history = repository.history(company, party.id()).await.unwrap();
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn finds_parties_and_replaces_their_document_check() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresPartyRepository::new(pool);
        contract::finds_parties_and_replaces_their_document_check(
            &repository,
            company,
            other_company,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn rejects_parties_of_unknown_companies() {
        let pool = contract::test_pool().await;
        let service = PartyService::new(
            PostgresPartyRepository::new(pool),
            InMemoryCnpjRegistry::default(),
        );
        let result = service
            .register(
                CompanyId::generate(),
                &contract::actor(),
                PartyDraft {
                    name: "Orfa".into(),
                    document: None,
                    email: None,
                    is_customer: true,
                    is_supplier: false,
                    ..PartyDraft::default()
                },
            )
            .await;
        assert!(matches!(
            result,
            Err(ApplicationError::Repository(RepositoryError::Storage(_)))
        ));
    }
}
