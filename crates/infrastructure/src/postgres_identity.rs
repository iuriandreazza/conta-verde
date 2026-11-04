use std::time::Duration;

use conta_verde_application::{IdentityRepository, NewAccount, RepositoryError, StoredCredentials};
use conta_verde_domain::{Company, CompanyId, Email, User, UserId};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;

const EMAIL_UNIQUE_CONSTRAINT: &str = "users_email_key";

pub struct PostgresIdentityRepository {
    pool: PgPool,
}

impl PostgresIdentityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl IdentityRepository for PostgresIdentityRepository {
    async fn register_account(&self, new_account: &NewAccount) -> Result<(), RepositoryError> {
        let NewAccount {
            account,
            password_hash,
        } = new_account;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;

        sqlx::query("INSERT INTO companies (id, name) VALUES ($1, $2)")
            .bind(account.company.id().as_uuid())
            .bind(account.company.name())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        sqlx::query("INSERT INTO users (id, name, email, password_hash) VALUES ($1, $2, $3, $4)")
            .bind(account.user.id().as_uuid())
            .bind(account.user.name())
            .bind(account.user.email().as_str())
            .bind(password_hash)
            .execute(&mut *tx)
            .await
            .map_err(|error| match &error {
                sqlx::Error::Database(db) if db.constraint() == Some(EMAIL_UNIQUE_CONSTRAINT) => {
                    RepositoryError::DuplicateEmail
                }
                _ => storage_error(error),
            })?;
        sqlx::query("INSERT INTO memberships (user_id, company_id) VALUES ($1, $2)")
            .bind(account.user.id().as_uuid())
            .bind(account.company.id().as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        tx.commit().await.map_err(storage_error)
    }

    async fn find_credentials(
        &self,
        email: &Email,
    ) -> Result<Option<StoredCredentials>, RepositoryError> {
        let row = sqlx::query("SELECT id, name, email, password_hash FROM users WHERE email = $1")
            .bind(email.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;

        row.map(|row| {
            Ok(StoredCredentials {
                user: user_from_row(&row)?,
                password_hash: row.try_get("password_hash").map_err(storage_error)?,
            })
        })
        .transpose()
    }

    async fn companies_of(&self, user_id: UserId) -> Result<Vec<Company>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT c.id, c.name FROM companies c \
             JOIN memberships m ON m.company_id = c.id \
             WHERE m.user_id = $1 ORDER BY lower(c.name), c.id",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.iter()
            .map(|row| {
                Ok(Company::restore(
                    CompanyId::from_uuid(row.try_get("id").map_err(storage_error)?),
                    row.try_get("name").map_err(storage_error)?,
                ))
            })
            .collect()
    }

    async fn is_member(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> Result<bool, RepositoryError> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM memberships WHERE user_id = $1 AND company_id = $2)",
        )
        .bind(user_id.as_uuid())
        .bind(company_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)
    }

    async fn create_session(
        &self,
        user_id: UserId,
        token_digest: &str,
        lifetime: Duration,
    ) -> Result<(), RepositoryError> {
        // Expired rows are only ever dead weight, so sweep them while we are writing anyway.
        sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
            .execute(&self.pool)
            .await
            .map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO sessions (token_digest, user_id, expires_at) \
             VALUES ($1, $2, now() + make_interval(secs => $3))",
        )
        .bind(token_digest)
        .bind(user_id.as_uuid())
        .bind(lifetime.as_secs_f64())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn find_session_user(&self, token_digest: &str) -> Result<Option<User>, RepositoryError> {
        let row = sqlx::query(
            "SELECT u.id, u.name, u.email FROM sessions s \
             JOIN users u ON u.id = s.user_id \
             WHERE s.token_digest = $1 AND s.expires_at > now()",
        )
        .bind(token_digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.as_ref().map(user_from_row).transpose()
    }

    async fn delete_session(&self, token_digest: &str) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM sessions WHERE token_digest = $1")
            .bind(token_digest)
            .execute(&self.pool)
            .await
            .map_err(storage_error)?;
        Ok(())
    }
}

fn user_from_row(row: &PgRow) -> Result<User, RepositoryError> {
    let id = UserId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let raw_email: String = row.try_get("email").map_err(storage_error)?;
    let email = Email::parse(&raw_email).map_err(|error| {
        RepositoryError::Storage(format!("stored user {id:?} has an invalid email: {error}"))
    })?;
    Ok(User::restore(
        id,
        row.try_get("name").map_err(storage_error)?,
        email,
    ))
}

#[cfg(test)]
mod tests {
    use conta_verde_application::AuthService;

    use super::*;
    use crate::contract;

    // Needs a running database: `docker compose up -d db` then `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_account_lifecycle() {
        let pool = contract::test_pool().await;
        let auth = AuthService::new(
            PostgresIdentityRepository::new(pool),
            crate::Argon2SecretService::new().unwrap(),
            Duration::from_secs(3600),
        );
        contract::runs_the_account_lifecycle(&auth).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn expired_sessions_are_not_found() {
        let pool = contract::test_pool().await;
        let identities = PostgresIdentityRepository::new(pool);
        let auth = AuthService::new(
            identities,
            crate::Argon2SecretService::new().unwrap(),
            Duration::ZERO,
        );
        contract::expired_sessions_are_rejected(&auth).await;
    }
}
