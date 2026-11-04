use conta_verde_application::{PartyRepository, RepositoryError};
use conta_verde_domain::{CompanyId, Party, PartyDraft, PartyId};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

const DOCUMENT_UNIQUE_INDEX: &str = "parties_company_document_key";

pub struct PostgresPartyRepository {
    pool: PgPool,
}

impl PostgresPartyRepository {
    pub async fn connect_and_migrate(
        database_url: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pool = PgPool::connect(database_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }
}

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
        .bind(party.email())
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

    async fn list_by_company(&self, company_id: CompanyId) -> Result<Vec<Party>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, company_id, name, document, email, is_customer, is_supplier \
             FROM parties WHERE company_id = $1 ORDER BY lower(name), id",
        )
        .bind(company_id.as_uuid())
        .fetch_all(&self.pool)
        .await
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

fn storage_error(error: sqlx::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{ApplicationError, PartyService};

    use super::*;

    // Needs a running database: `docker compose up -d db` then `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_parties_and_enforces_unique_document_per_company() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let repository = PostgresPartyRepository::connect_and_migrate(&url)
            .await
            .unwrap();
        let service = PartyService::new(repository);
        let company = CompanyId::generate();
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

        let listed = service.list(company).await.unwrap();
        let names: Vec<&str> = listed.iter().map(Party::name).collect();
        assert_eq!(names, ["ana", "Padaria", "zeca"]);
        assert_eq!(listed[1].document().unwrap().digits(), "11222333000181");
        assert!(listed[1].is_customer() && listed[1].is_supplier());
    }
}
