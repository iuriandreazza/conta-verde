use std::future::Future;
use std::time::Duration;

use conta_verde_domain::{Account, Company, CompanyId, Email, Role, User, UserId};

use crate::{Member, RepositoryError};

// Neither struct derives Debug: the password hash must not end up in logs or panic messages.

/// A new account together with the hash of its password.
#[derive(Clone)]
pub struct NewAccount {
    pub account: Account,
    pub password_hash: String,
}

/// A new user with no company, together with the hash of their password: someone who joins a
/// company that was made by another.
#[derive(Clone)]
pub struct NewUser {
    pub user: User,
    pub password_hash: String,
}

#[derive(Clone)]
pub struct StoredCredentials {
    pub user: User,
    pub password_hash: String,
}

/// A company a user belongs to, and what they are there.
#[derive(Debug, Clone)]
pub struct CompanyMembership {
    pub company: Company,
    pub role: Role,
}

/// Port for who users are and which companies they may access. Sessions are keyed by a digest of
/// the token, so a leaked database does not leak usable sessions.
pub trait IdentityRepository: Send + Sync {
    /// Stores user, company and the owner's membership atomically.
    fn register_account(
        &self,
        new_account: &NewAccount,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Stores a user who belongs to no company yet. `DuplicateEmail` when the address has an account.
    fn register_user(
        &self,
        new_user: &NewUser,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn find_credentials(
        &self,
        email: &Email,
    ) -> impl Future<Output = Result<Option<StoredCredentials>, RepositoryError>> + Send;

    /// Companies the user belongs to, with their role in each, ordered by company name ignoring
    /// case, then by id.
    fn memberships_of(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<Vec<CompanyMembership>, RepositoryError>> + Send;

    /// The name of the company, `None` when there is no such company.
    fn company_name(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<String>, RepositoryError>> + Send;

    /// The company's members and what each is, ordered by name ignoring case, then by id.
    fn members_of(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<Member>, RepositoryError>> + Send;

    /// What the user is in the company, `None` when they are not a member of it.
    fn role_in(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<Role>, RepositoryError>> + Send;

    /// Makes the user a member of the company. `DuplicateMembership` when they already are: a
    /// second call never changes a role.
    fn add_membership(
        &self,
        user_id: UserId,
        company_id: CompanyId,
        role: Role,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Changes what the user is in the company. `NotFound` when they are not a member, and also
    /// when they are the owner, whose membership this never touches.
    fn set_role(
        &self,
        user_id: UserId,
        company_id: CompanyId,
        role: Role,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Ends the user's membership of the company, with the same `NotFound` as [`Self::set_role`].
    /// The user, their sessions and their other companies are left alone.
    fn remove_membership(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

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

    /// Keeps the digest of a password reset's token for the user, replacing the one they had (a user has one at a
    /// time). `false`, and nothing stored, when the user's last one was made less than `cooldown` ago, which is what
    /// keeps a form from being used to fill someone's inbox.
    fn create_password_reset(
        &self,
        user_id: UserId,
        token_digest: &str,
        lifetime: Duration,
        cooldown: Duration,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// Whether a reset that has not expired has this digest, without spending it: a request that cannot succeed is turned
    /// away before the work of hashing a new password.
    fn password_reset_open(
        &self,
        token_digest: &str,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// Spends the reset with this digest, once: the user's password becomes `new_hash` and **every session of theirs
    /// ends**, so whoever held the old password is out. It says whose password it was. `NotFound` when no reset that has
    /// not expired has this digest.
    fn reset_password(
        &self,
        token_digest: &str,
        new_hash: &str,
    ) -> impl Future<Output = Result<UserId, RepositoryError>> + Send;

    /// Whether the user has confirmed their address.
    fn email_verified(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// Marks the address as confirmed without a link, for an address that has proved itself some other way (an
    /// invitation sent to it and opened).
    fn mark_email_verified(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Keeps the digest of an address confirmation's token for the user; up to five links stay valid at once. `false`,
    /// and nothing stored, when the user's last one was made less than `cooldown` ago.
    fn create_email_confirmation(
        &self,
        user_id: UserId,
        token_digest: &str,
        lifetime: Duration,
        cooldown: Duration,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// Spends the confirmation with this digest: the user's address becomes confirmed and their other links are gone. It
    /// says whose address it was. `NotFound` when no link that has not expired has this digest.
    fn confirm_email(
        &self,
        token_digest: &str,
    ) -> impl Future<Output = Result<UserId, RepositoryError>> + Send;

    /// The user with this id, if there is one.
    fn find_user(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<Option<User>, RepositoryError>> + Send;
}
