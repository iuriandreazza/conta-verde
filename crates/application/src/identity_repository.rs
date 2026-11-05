use std::future::Future;
use std::time::Duration;

use conta_verde_domain::{Account, Company, CompanyId, Email, User, UserId};

use crate::RepositoryError;

// Neither struct derives Debug: the password hash must not end up in logs or panic messages.

/// A new account together with the hash of its password.
#[derive(Clone)]
pub struct NewAccount {
    pub account: Account,
    pub password_hash: String,
}

#[derive(Clone)]
pub struct StoredCredentials {
    pub user: User,
    pub password_hash: String,
}

/// Port for who users are and which companies they may access. Sessions are keyed by a digest of
/// the token, so a leaked database does not leak usable sessions.
pub trait IdentityRepository: Send + Sync {
    /// Stores user, company and the owner's membership atomically.
    fn register_account(
        &self,
        new_account: &NewAccount,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn find_credentials(
        &self,
        email: &Email,
    ) -> impl Future<Output = Result<Option<StoredCredentials>, RepositoryError>> + Send;

    /// Companies the user belongs to, ordered by name ignoring case, then by id.
    fn companies_of(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<Vec<Company>, RepositoryError>> + Send;

    fn is_member(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    fn create_session(
        &self,
        user_id: UserId,
        token_digest: &str,
        lifetime: Duration,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The owner of a session that has not expired.
    fn find_session_user(
        &self,
        token_digest: &str,
    ) -> impl Future<Output = Result<Option<User>, RepositoryError>> + Send;

    fn delete_session(
        &self,
        token_digest: &str,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
}
