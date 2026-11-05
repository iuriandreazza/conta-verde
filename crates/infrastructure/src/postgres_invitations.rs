use conta_verde_application::{
    InvitationRepository, InvitationState, NewInvitation, RepositoryError, StoredInvitation,
};
use conta_verde_domain::{CompanyId, Email, Invitation, InvitationId, Role, UserId};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;

const OPEN_UNIQUE_INDEX: &str = "invitations_one_open_key";

/// What every query reads of an invitation; the times as milliseconds since the Unix epoch.
const COLUMNS: &str = "i.id, i.company_id, i.email, i.role, i.invited_by, \
     (extract(epoch FROM i.created_at) * 1000)::bigint AS created_at_millis, \
     (extract(epoch FROM i.expires_at) * 1000)::bigint AS expires_at_millis";

pub struct PostgresInvitationRepository {
    pool: PgPool,
}

impl PostgresInvitationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl InvitationRepository for PostgresInvitationRepository {
    async fn create(
        &self,
        new_invitation: &NewInvitation,
        now_millis: i64,
    ) -> Result<(), RepositoryError> {
        let invitation = &new_invitation.invitation;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // An open invitation that has expired is in the way of a new one: it is withdrawn.
        sqlx::query(
            "UPDATE invitations SET revoked_at = to_timestamp($3::double precision / 1000) \
             WHERE company_id = $1 AND email = $2 AND accepted_at IS NULL AND revoked_at IS NULL \
             AND expires_at <= to_timestamp($3::double precision / 1000)",
        )
        .bind(invitation.company_id().as_uuid())
        .bind(invitation.email().as_str())
        .bind(now_millis)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO invitations (id, company_id, email, role, token_digest, invited_by, \
             created_at, expires_at) VALUES ($1, $2, $3, $4, $5, $6, \
             to_timestamp($7::double precision / 1000), to_timestamp($8::double precision / 1000))",
        )
        .bind(invitation.id().as_uuid())
        .bind(invitation.company_id().as_uuid())
        .bind(invitation.email().as_str())
        .bind(invitation.role().as_str())
        .bind(&new_invitation.token_digest)
        .bind(invitation.invited_by().as_uuid())
        .bind(invitation.created_at_millis())
        .bind(invitation.expires_at_millis())
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.constraint() == Some(OPEN_UNIQUE_INDEX) => {
                RepositoryError::DuplicateInvitation
            }
            _ => storage_error(error),
        })?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find_by_token_digest(
        &self,
        token_digest: &str,
    ) -> Result<Option<StoredInvitation>, RepositoryError> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS}, c.name AS company_name, i.accepted_at IS NOT NULL AS accepted, \
             i.revoked_at IS NOT NULL AS revoked \
             FROM invitations i JOIN companies c ON c.id = i.company_id \
             WHERE i.token_digest = $1"
        ))
        .bind(token_digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.map(|row| {
            let accepted: bool = row.try_get("accepted").map_err(storage_error)?;
            let revoked: bool = row.try_get("revoked").map_err(storage_error)?;
            Ok(StoredInvitation {
                invitation: invitation_from_row(&row)?,
                company_name: row.try_get("company_name").map_err(storage_error)?,
                state: match (accepted, revoked) {
                    (true, _) => InvitationState::Accepted,
                    (_, true) => InvitationState::Revoked,
                    _ => InvitationState::Open,
                },
            })
        })
        .transpose()
    }

    async fn open_invitations(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<Invitation>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM invitations i \
             WHERE i.company_id = $1 AND i.accepted_at IS NULL AND i.revoked_at IS NULL \
             ORDER BY i.created_at, i.id"
        ))
        .bind(company_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(invitation_from_row).collect()
    }

    async fn revoke(&self, company_id: CompanyId, id: InvitationId) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE invitations SET revoked_at = now() \
             WHERE id = $1 AND company_id = $2 AND accepted_at IS NULL AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(company_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn mark_accepted(&self, id: InvitationId) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE invitations SET accepted_at = now() \
             WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }
}

fn invitation_from_row(row: &PgRow) -> Result<Invitation, RepositoryError> {
    let email: String = row.try_get("email").map_err(storage_error)?;
    let role: String = row.try_get("role").map_err(storage_error)?;
    Ok(Invitation::restore(
        InvitationId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        Email::parse(&email).map_err(|error| {
            RepositoryError::Storage(format!("stored invitation has an invalid email: {error}"))
        })?,
        Role::parse(&role).ok_or_else(|| {
            RepositoryError::Storage(format!("stored invitation role `{role}` is unknown"))
        })?,
        UserId::from_uuid(row.try_get("invited_by").map_err(storage_error)?),
        row.try_get("created_at_millis").map_err(storage_error)?,
        row.try_get("expires_at_millis").map_err(storage_error)?,
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use conta_verde_application::{AuthService, MemberService};

    use super::*;
    use crate::{Argon2SecretService, InMemoryEmailSender, PostgresIdentityRepository, contract};

    // Needs a running database: `docker compose up -d db` then `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_invitation_lifecycle() {
        let pool = contract::test_pool().await;
        let emails = InMemoryEmailSender::default();
        let service = |lifetime: Duration| {
            MemberService::new(
                PostgresInvitationRepository::new(pool.clone()),
                PostgresIdentityRepository::new(pool.clone()),
                emails.clone(),
                Argon2SecretService::new().unwrap(),
                lifetime,
                "https://erp.example",
            )
        };
        let auth = AuthService::new(
            PostgresIdentityRepository::new(pool.clone()),
            Argon2SecretService::new().unwrap(),
            Duration::from_secs(3600),
        );
        contract::runs_the_invitation_lifecycle(
            &auth,
            &service(Duration::from_secs(7 * 86_400)),
            &service(Duration::from_millis(1)),
            &emails,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let user = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, name, email, password_hash) VALUES ($1, 'Ana', $2, 'x')",
        )
        .bind(user)
        .bind(format!("ana-{user}@example.com"))
        .execute(&pool)
        .await
        .unwrap();
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let insert = |email: &'static str, role: &'static str, lifetime_seconds: i32| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO invitations (id, company_id, email, role, token_digest, invited_by, \
                     created_at, expires_at) VALUES (gen_random_uuid(), $1, $2, $3, gen_random_uuid()::text, $4, \
                     now(), now() + make_interval(secs => $5))",
                )
                .bind(company.as_uuid())
                .bind(email)
                .bind(role)
                .bind(user)
                .bind(lifetime_seconds)
                .execute(&pool)
                .await
            }
        };
        let cases = [
            (
                insert("bia@example.com", "owner", 60).await,
                "invitations_role_valid",
            ),
            (
                insert("bia@example.com", "root", 60).await,
                "invitations_role_valid",
            ),
            (
                insert("Bia@example.com", "sales", 60).await,
                "invitations_email_valid",
            ),
            (
                insert(" bia@example.com", "sales", 60).await,
                "invitations_email_valid",
            ),
            (insert("bia", "sales", 60).await, "invitations_email_valid"),
            (
                insert("bia@example", "sales", 60).await,
                "invitations_email_valid",
            ),
            (
                insert("bia@example.com", "sales", 0).await,
                "invitations_expires_after_creation",
            ),
        ];
        for (result, constraint) in cases {
            assert_eq!(refused_by(result).as_deref(), Some(constraint));
        }
        insert("bia@example.com", "sales", 60).await.unwrap();
        // One open invitation per address in a company.
        assert_eq!(
            refused_by(insert("bia@example.com", "viewer", 60).await).as_deref(),
            Some("invitations_one_open_key")
        );
        // Used and withdrawn at once is refused.
        sqlx::query("UPDATE invitations SET accepted_at = now() WHERE company_id = $1")
            .bind(company.as_uuid())
            .execute(&pool)
            .await
            .unwrap();
        let both = sqlx::query("UPDATE invitations SET revoked_at = now() WHERE company_id = $1")
            .bind(company.as_uuid())
            .execute(&pool)
            .await;
        assert_eq!(
            refused_by(both).as_deref(),
            Some("invitations_resolved_once")
        );
    }
}
