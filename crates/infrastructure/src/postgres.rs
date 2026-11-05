use conta_verde_application::{PartyCursor, PartyRepository, RepositoryError};
use conta_verde_domain::{CompanyId, Party, PartyDraft, PartyId};
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

const SELECT_PARTIES: &str = "SELECT id, company_id, name, document, email, is_customer, is_supplier \
     FROM parties WHERE company_id = $1";

impl PartyRepository for PostgresPartyRepository {
    async fn save(&self, party: &Party) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO parties (id, company_id, name, document, email, is_customer, is_supplier) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(party.id().as_uuid())
        .bind(party.company_id().as_uuid())
        .bind(party.name())
        .bind(party.document().map(|document| document.digits()))
        .bind(party.email().map(|email| email.as_str()))
        .bind(party.is_customer())
        .bind(party.is_supplier())
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

    Party::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored party {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{ApplicationError, PartyService};

    use super::*;
    use crate::contract;

    // Needs a running database: `docker compose up -d db` then `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_parties_and_enforces_unique_document_per_company() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = PartyService::new(PostgresPartyRepository::new(pool));
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
        assert_eq!(listed[1].document().unwrap().digits(), "11222333000181");
        assert!(listed[1].is_customer() && listed[1].is_supplier());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn pages_through_parties_in_name_order() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = PartyService::new(PostgresPartyRepository::new(pool));
        contract::pages_through_parties(&service, company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn rejects_parties_of_unknown_companies() {
        let pool = contract::test_pool().await;
        let service = PartyService::new(PostgresPartyRepository::new(pool));
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
