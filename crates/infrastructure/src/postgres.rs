use conta_verde_application::{PartyCursor, PartyRepository, RepositoryError};
use conta_verde_domain::{CompanyId, DocumentCheck, Party, PartyDraft, PartyId};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

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

pub struct PostgresPartyRepository {
    pool: PgPool,
}

impl PostgresPartyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const SELECT_PARTIES: &str = "SELECT id, company_id, name, document, email, is_customer, is_supplier, \
     document_check, registry_legal_name, registry_situation \
     FROM parties WHERE company_id = $1";

impl PartyRepository for PostgresPartyRepository {
    async fn save(&self, party: &Party) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO parties (id, company_id, name, document, email, is_customer, is_supplier, \
             document_check, registry_legal_name, registry_situation) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(party.id().as_uuid())
        .bind(party.company_id().as_uuid())
        .bind(party.name())
        .bind(party.document().map(|document| document.as_str()))
        .bind(party.email().map(|email| email.as_str()))
        .bind(party.is_customer())
        .bind(party.is_supplier())
        .bind(check_kind(party.document_check()))
        .bind(registry_legal_name(party.document_check()))
        .bind(registry_situation(party.document_check()))
        .execute(&self.pool)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(DOCUMENT_UNIQUE_INDEX) => {
                RepositoryError::DuplicateDocument
            }
            _ => storage_error(error),
        })?;
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<Option<Party>, RepositoryError> {
        let row = sqlx::query(&format!("{SELECT_PARTIES} AND id = $2"))
            .bind(company_id.as_uuid())
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        row.as_ref().map(party_from_row).transpose()
    }

    async fn update_document_check(&self, party: &Party) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE parties SET document_check = $3, registry_legal_name = $4, registry_situation = $5 \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(party.id().as_uuid())
        .bind(party.company_id().as_uuid())
        .bind(check_kind(party.document_check()))
        .bind(registry_legal_name(party.document_check()))
        .bind(registry_situation(party.document_check()))
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        // Matching no row means the party is not in that company: report it, do not pretend.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        after: Option<&PartyCursor>,
        limit: usize,
    ) -> Result<Vec<Party>, RepositoryError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        // Separate statements keep the keyset condition out of the plan when there is no cursor.
        let rows = match after {
            None => {
                sqlx::query(&format!(
                    "{SELECT_PARTIES} ORDER BY lower(name), id LIMIT $2"
                ))
                .bind(company_id.as_uuid())
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            Some(cursor) => {
                sqlx::query(&format!(
                    "{SELECT_PARTIES} AND (lower(name), id) > (lower($3), $4) \
                     ORDER BY lower(name), id LIMIT $2"
                ))
                .bind(company_id.as_uuid())
                .bind(limit)
                .bind(&cursor.name)
                .bind(cursor.id.as_uuid())
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(storage_error)?;

        rows.iter().map(party_from_row).collect()
    }
}

fn party_from_row(row: &PgRow) -> Result<Party, RepositoryError> {
    let draft = PartyDraft {
        name: row.try_get("name").map_err(storage_error)?,
        document: row.try_get("document").map_err(storage_error)?,
        email: row.try_get("email").map_err(storage_error)?,
        is_customer: row.try_get("is_customer").map_err(storage_error)?,
        is_supplier: row.try_get("is_supplier").map_err(storage_error)?,
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
        };

        service
            .register(company, draft("Padaria", Some("11.222.333/0001-81")))
            .await
            .unwrap();
        let duplicate = service
            .register(company, draft("Padaria 2", Some("11.222.333/0001-81")))
            .await;
        assert_eq!(
            duplicate.unwrap_err(),
            ApplicationError::Repository(RepositoryError::DuplicateDocument)
        );

        service
            .register(company, draft("zeca", None))
            .await
            .unwrap();
        service.register(company, draft("ana", None)).await.unwrap();

        let listed = service.list(company, None, None).await.unwrap().items;
        let names: Vec<&str> = listed.iter().map(Party::name).collect();
        assert_eq!(names, ["ana", "Padaria", "zeca"]);
        assert_eq!(listed[1].document().unwrap().as_str(), "11222333000181");
        assert!(listed[1].is_customer() && listed[1].is_supplier());
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
                    "INSERT INTO parties (id, company_id, name, document, is_customer, is_supplier, document_check) \
                     VALUES (gen_random_uuid(), $1, 'Teste', $2, true, false, $3)",
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
                PartyDraft {
                    name: "Orfa".into(),
                    document: None,
                    email: None,
                    is_customer: true,
                    is_supplier: false,
                },
            )
            .await;
        assert!(matches!(
            result,
            Err(ApplicationError::Repository(RepositoryError::Storage(_)))
        ));
    }
}
