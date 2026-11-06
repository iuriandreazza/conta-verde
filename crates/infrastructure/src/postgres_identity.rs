use std::time::Duration;

use conta_verde_application::{
    CompanyMembership, IdentityRepository, Member, NewAccount, NewUser, RepositoryError,
    StoredCredentials,
};
use conta_verde_domain::{Company, CompanyId, Email, Role, User, UserId};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;

const EMAIL_UNIQUE_CONSTRAINT: &str = "users_email_key";
const MEMBERSHIP_PKEY: &str = "memberships_pkey";

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
        sqlx::query("INSERT INTO memberships (user_id, company_id, role) VALUES ($1, $2, $3)")
            .bind(account.user.id().as_uuid())
            .bind(account.company.id().as_uuid())
            .bind(Role::Owner.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        tx.commit().await.map_err(storage_error)
    }

    async fn register_user(&self, new_user: &NewUser) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO users (id, name, email, password_hash) VALUES ($1, $2, $3, $4)")
            .bind(new_user.user.id().as_uuid())
            .bind(new_user.user.name())
            .bind(new_user.user.email().as_str())
            .bind(&new_user.password_hash)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| match &error {
                sqlx::Error::Database(db) if db.constraint() == Some(EMAIL_UNIQUE_CONSTRAINT) => {
                    RepositoryError::DuplicateEmail
                }
                _ => storage_error(error),
            })
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

    async fn memberships_of(
        &self,
        user_id: UserId,
    ) -> Result<Vec<CompanyMembership>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT c.id, c.name, m.role FROM companies c \
             JOIN memberships m ON m.company_id = c.id \
             WHERE m.user_id = $1 ORDER BY lower(c.name), c.id",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.iter()
            .map(|row| {
                Ok(CompanyMembership {
                    company: Company::restore(
                        CompanyId::from_uuid(row.try_get("id").map_err(storage_error)?),
                        row.try_get("name").map_err(storage_error)?,
                    ),
                    role: role_from_row(row)?,
                })
            })
            .collect()
    }

    async fn company_name(&self, company_id: CompanyId) -> Result<Option<String>, RepositoryError> {
        sqlx::query_scalar("SELECT name FROM companies WHERE id = $1")
            .bind(company_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)
    }

    async fn members_of(&self, company_id: CompanyId) -> Result<Vec<Member>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT u.id, u.name, u.email, m.role FROM memberships m \
             JOIN users u ON u.id = m.user_id \
             WHERE m.company_id = $1 ORDER BY lower(u.name), u.id",
        )
        .bind(company_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter()
            .map(|row| {
                Ok(Member {
                    user: user_from_row(row)?,
                    role: role_from_row(row)?,
                })
            })
            .collect()
    }

    async fn role_in(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> Result<Option<Role>, RepositoryError> {
        let row =
            sqlx::query("SELECT role FROM memberships WHERE user_id = $1 AND company_id = $2")
                .bind(user_id.as_uuid())
                .bind(company_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage_error)?;
        row.as_ref().map(role_from_row).transpose()
    }

    async fn add_membership(
        &self,
        user_id: UserId,
        company_id: CompanyId,
        role: Role,
    ) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO memberships (user_id, company_id, role) VALUES ($1, $2, $3)")
            .bind(user_id.as_uuid())
            .bind(company_id.as_uuid())
            .bind(role.as_str())
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| match &error {
                sqlx::Error::Database(db) if db.constraint() == Some(MEMBERSHIP_PKEY) => {
                    RepositoryError::DuplicateMembership
                }
                _ => storage_error(error),
            })
    }

    async fn set_role(
        &self,
        user_id: UserId,
        company_id: CompanyId,
        role: Role,
    ) -> Result<(), RepositoryError> {
        let changed = sqlx::query(
            "UPDATE memberships SET role = $3 \
             WHERE user_id = $1 AND company_id = $2 AND role <> 'owner'",
        )
        .bind(user_id.as_uuid())
        .bind(company_id.as_uuid())
        .bind(role.as_str())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        matched_one(changed.rows_affected())
    }

    async fn remove_membership(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> Result<(), RepositoryError> {
        let removed = sqlx::query(
            "DELETE FROM memberships WHERE user_id = $1 AND company_id = $2 AND role <> 'owner'",
        )
        .bind(user_id.as_uuid())
        .bind(company_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        matched_one(removed.rows_affected())
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

/// The table's CHECK keeps the key one of the seven, so anything else is corrupt data.
fn role_from_row(row: &PgRow) -> Result<Role, RepositoryError> {
    let key: String = row.try_get("role").map_err(storage_error)?;
    Role::parse(&key).ok_or_else(|| {
        RepositoryError::Storage(format!("stored membership role `{key}` is unknown"))
    })
}

fn matched_one(rows: u64) -> Result<(), RepositoryError> {
    if rows == 0 {
        Err(RepositoryError::NotFound)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::AuthService;
    use conta_verde_domain::{Account, AccountDraft};

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
    async fn registers_a_user_with_no_company() {
        let pool = contract::test_pool().await;
        contract::registers_a_user_with_no_company(&PostgresIdentityRepository::new(pool)).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn memberships_carry_a_role() {
        let pool = contract::test_pool().await;
        contract::memberships_carry_a_role(&PostgresIdentityRepository::new(pool)).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_a_role_that_does_not_exist_and_one_left_unsaid() {
        let pool = contract::test_pool().await;
        let identities = PostgresIdentityRepository::new(pool.clone());
        let owner = Account::new(
            UserId::generate(),
            CompanyId::generate(),
            AccountDraft {
                user_name: "Ana".into(),
                email: format!("ana-{}@example.com", uuid::Uuid::new_v4()),
                company_name: "Padaria".into(),
            },
        )
        .unwrap();
        identities
            .register_account(&NewAccount {
                account: owner.clone(),
                password_hash: "x".into(),
            })
            .await
            .unwrap();
        let other = UserId::generate();
        sqlx::query(
            "INSERT INTO users (id, name, email, password_hash) VALUES ($1, 'Bia', $2, 'x')",
        )
        .bind(other.as_uuid())
        .bind(format!("bia-{}@example.com", uuid::Uuid::new_v4()))
        .execute(&pool)
        .await
        .unwrap();
        let company = owner.company.id().as_uuid();
        let insert = |role: Option<&'static str>| {
            let pool = pool.clone();
            async move {
                match role {
                    Some(role) => sqlx::query(
                        "INSERT INTO memberships (user_id, company_id, role) VALUES ($1, $2, $3)",
                    )
                    .bind(other.as_uuid())
                    .bind(company)
                    .bind(role)
                    .execute(&pool)
                    .await,
                    // No role at all: there is no default, so nothing becomes an owner by omission.
                    None => {
                        sqlx::query("INSERT INTO memberships (user_id, company_id) VALUES ($1, $2)")
                            .bind(other.as_uuid())
                            .bind(company)
                            .execute(&pool)
                            .await
                    }
                }
            }
        };
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => (
                db.constraint().map(str::to_owned),
                db.code().map(|c| c.into_owned()),
            ),
            _ => (None, None),
        };
        for role in ["root", "Owner", "", "owner "] {
            assert_eq!(
                refused_by(insert(Some(role)).await).0.as_deref(),
                Some("memberships_role_valid"),
                "{role:?}"
            );
        }
        // NOT NULL violation (23502), not a constraint name.
        assert_eq!(refused_by(insert(None).await).1.as_deref(), Some("23502"));
        insert(Some("viewer")).await.unwrap();
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
