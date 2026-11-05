use std::future::Future;

use conta_verde_domain::{CompanyId, Invitation, InvitationId};

use crate::RepositoryError;

/// An invitation and the digest of the token its link carries: the token itself is never stored.
#[derive(Debug, Clone)]
pub struct NewInvitation {
    pub invitation: Invitation,
    pub token_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitationState {
    /// Not yet accepted or revoked. It may still have expired: see [`Invitation::is_expired_at`].
    Open,
    Accepted,
    Revoked,
}

/// What a link's token leads to.
#[derive(Debug, Clone)]
pub struct StoredInvitation {
    pub invitation: Invitation,
    pub company_name: String,
    pub state: InvitationState,
}

/// Port for invitations. Neither accepting nor revoking one is audited ([ADR 0008]): they are
/// membership writes, which the trail never holds.
pub trait InvitationRepository: Send + Sync {
    /// Stores the invitation. A company has at most one open invitation per e-mail address:
    /// `DuplicateInvitation` when there is one, unless it has already expired at `now_millis`, in
    /// which case it is revoked and replaced.
    fn create(
        &self,
        new_invitation: &NewInvitation,
        now_millis: i64,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn find_by_token_digest(
        &self,
        token_digest: &str,
    ) -> impl Future<Output = Result<Option<StoredInvitation>, RepositoryError>> + Send;

    /// The company's open invitations, expired ones included (the person who sent them sees that they
    /// expired), oldest first.
    fn open_invitations(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<Invitation>, RepositoryError>> + Send;

    /// Withdraws an open invitation of the company. `NotFound` when the company has no such open
    /// invitation (another company's looks the same).
    fn revoke(
        &self,
        company_id: CompanyId,
        id: InvitationId,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Marks an open invitation as accepted. `NotFound` when it is not open.
    fn mark_accepted(
        &self,
        id: InvitationId,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
}
