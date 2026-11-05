use crate::postgres_audit;
use conta_verde_application::{
    NameCursor, NameSort, PartyKind, PartyQuery, PartyRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AddressDraft, AuditEntity, AuditEntry, AuditRecord, CompanyId, DocumentCheck, Party,
    PartyDraft, PartyId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

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
    let (comparison, direction) = match sort {
        NameSort::NameAsc => (">", "ASC"),
        NameSort::NameDesc => ("<", "DESC"),
    };
    if let Some(cursor) = after {
        sql.push(format!(" AND (lower(name), id) {comparison} (lower("))
            .push_bind(cursor.name.clone())
            .push("), ")
            .push_bind(cursor.id)
            .push(")");
    }
    sql.push(format!(
        " ORDER BY lower(name) {direction}, id {direction} LIMIT "
    ))
    .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
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
        let row = sqlx::query(&format!(
            "{SELECT_PARTIES} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(party_from_row).transpose()
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

        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(party_from_row).collect()
    }
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

fn party_from_row(row: &PgRow) -> Result<Party, RepositoryError> {
    let draft = PartyDraft {
        name: row.try_get("name").map_err(storage_error)?,
        document: row.try_get("document").map_err(storage_error)?,
        email: row.try_get("email").map_err(storage_error)?,
        is_customer: row.try_get("is_customer").map_err(storage_error)?,
        is_supplier: row.try_get("is_supplier").map_err(storage_error)?,
        is_carrier: row.try_get("is_carrier").map_err(storage_error)?,
        is_seller: row.try_get("is_seller").map_err(storage_error)?,
        address: address_from_row(row)?,
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
