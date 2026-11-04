use conta_verde_application::{PartyRepository, RepositoryError};
use conta_verde_domain::{CompanyId, Party, PartyDraft, PartyId};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

pub struct PostgresPartyRepository {
    pool: PgPool,
}

impl PostgresPartyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                RepositoryError::DuplicateDocument
            }
            _ => storage_error(error),
        })?;
        Ok(())
    }

    async fn list_by_company(&self, company_id: CompanyId) -> Result<Vec<Party>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, company_id, name, document, email, is_customer, is_supplier \
             FROM parties WHERE company_id = $1 ORDER BY name",
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
    use conta_verde_application::PartyService;

    use super::*;

    // Needs a running database: `docker compose up -d db` then `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_parties_and_enforces_unique_document_per_company() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let pool = PgPool::connect(&url).await.unwrap();
        migrate(&pool).await.unwrap();
        let service = PartyService::new(PostgresPartyRepository::new(pool));
        let company = CompanyId::generate();
        let draft = |name: &str| PartyDraft {
            name: name.into(),
            document: Some("11.222.333/0001-81".into()),
            email: Some("contato@example.com".into()),
            is_customer: true,
            is_supplier: true,
        };

        service.register(company, draft("Padaria")).await.unwrap();
        let duplicate = service.register(company, draft("Padaria 2")).await;
        assert_eq!(
            duplicate.unwrap_err(),
            conta_verde_application::ApplicationError::DuplicateDocument
        );

        let listed = service.list(company).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].document().unwrap().digits(), "11222333000181");
        assert!(listed[0].is_customer() && listed[0].is_supplier());
    }
}
