use std::time::{Duration, SystemTime, UNIX_EPOCH};

use conta_verde_domain::{
    Actor, CompanyId, Email, Invitation, InvitationId, Role, User, UserId, validate_password,
};

use crate::{
    ApplicationError, EmailMessage, EmailSender, IdentityRepository, InvitationRepository,
    InvitationState, Member, NewInvitation, NewUser, RepositoryError, SecretService,
    StoredInvitation,
};

/// A company's invitation just made, and the link that accepts it.
#[derive(Debug, Clone)]
pub struct Invited {
    pub invitation: Invitation,
    /// The only time the link exists: the store keeps a digest of its token.
    pub link: String,
    /// Whether the e-mail with the link was sent. It is not a failure when it was not: the person
    /// who invited can pass the link on.
    pub emailed: bool,
}

/// What a person sees before accepting an invitation.
#[derive(Debug, Clone)]
pub struct InvitationPreview {
    pub company_name: String,
    pub email: Email,
    pub role: Role,
    pub expires_at_millis: i64,
}

/// Who belongs to a company, and how people come to ([ADR 0027]).
pub struct MemberService<I, M, E, S> {
    invitations: I,
    identities: M,
    emails: E,
    secrets: S,
    invitation_lifetime: Duration,
    /// Where the web app is, so a link in an e-mail leads to it.
    public_url: String,
}

impl<I, M, E, S> MemberService<I, M, E, S>
where
    I: InvitationRepository,
    M: IdentityRepository,
    E: EmailSender,
    S: SecretService,
{
    pub fn new(
        invitations: I,
        identities: M,
        emails: E,
        secrets: S,
        invitation_lifetime: Duration,
        public_url: &str,
    ) -> Self {
        Self {
            invitations,
            identities,
            emails,
            secrets,
            invitation_lifetime,
            public_url: public_url.trim_end_matches('/').to_owned(),
        }
    }

    /// The company's members, with what each is, ordered by name ignoring case.
    pub async fn members(&self, company_id: CompanyId) -> Result<Vec<Member>, ApplicationError> {
        Ok(self.identities.members_of(company_id).await?)
    }

    /// The company's open invitations, oldest first, expired ones included.
    pub async fn open_invitations(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<Invitation>, ApplicationError> {
        Ok(self.invitations.open_invitations(company_id).await?)
    }

    /// Invites an e-mail address to the company in a role, and e-mails the link. Nobody is invited
    /// as an owner, and nobody who is already a member, or already has an open invitation.
    pub async fn invite(
        &self,
        company_id: CompanyId,
        inviter: &Actor,
        email: &str,
        role: Role,
    ) -> Result<Invited, ApplicationError> {
        let Actor::User {
            id: inviter_id,
            name: inviter_name,
        } = inviter;
        let now = now_millis();
        let invitation = Invitation::new(
            InvitationId::generate(),
            company_id,
            email,
            role,
            *inviter_id,
            now,
            i64::try_from(self.invitation_lifetime.as_millis()).unwrap_or(i64::MAX),
        )?;
        if let Some(existing) = self.identities.find_credentials(invitation.email()).await?
            && self
                .identities
                .role_in(existing.user.id(), company_id)
                .await?
                .is_some()
        {
            return Err(RepositoryError::DuplicateMembership.into());
        }

        let token = self.secrets.new_token();
        self.invitations
            .create(
                &NewInvitation {
                    invitation: invitation.clone(),
                    token_digest: self.secrets.digest_token(&token),
                },
                now,
            )
            .await?;

        let link = format!("{}/invitations/{token}", self.public_url);
        let company_name = self
            .identities
            .memberships_of(*inviter_id)
            .await?
            .into_iter()
            .find(|membership| membership.company.id() == company_id)
            .map(|membership| membership.company.name().to_owned())
            .unwrap_or_default();
        // The names are typed by people: a line break in one would add headers to the message or
        // forge lines in a log.
        let inviter_name = one_line(inviter_name);
        let company_name = one_line(&company_name);
        let message = EmailMessage {
            to: invitation.email().clone(),
            subject: format!("{inviter_name} convidou você para {company_name} no conta-verde"),
            body: format!(
                "{inviter_name} convidou você para participar de {company_name} no conta-verde.\n\n\
                 Para aceitar, entre com este endereço de e-mail e abra o link:\n{link}\n\n\
                 O convite vale por {} dias.\n",
                self.invitation_lifetime.as_secs() / 86_400
            ),
        };
        let emailed = self.emails.send(&message).await.is_ok();
        Ok(Invited {
            invitation,
            link,
            emailed,
        })
    }

    /// Withdraws an open invitation of the company, so its link stops working.
    pub async fn revoke(
        &self,
        company_id: CompanyId,
        id: InvitationId,
    ) -> Result<(), ApplicationError> {
        self.invitations
            .revoke(company_id, id)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::InvitationNotFound,
                other => other.into(),
            })
    }

    /// What a link leads to, for the page that asks the person to accept: the company, the address
    /// it was sent to and the role. Nothing is checked about who asks: the token is the secret.
    pub async fn preview(&self, token: &str) -> Result<InvitationPreview, ApplicationError> {
        let stored = self.usable(token).await?;
        Ok(InvitationPreview {
            company_name: stored.company_name,
            email: stored.invitation.email().clone(),
            role: stored.invitation.role(),
            expires_at_millis: stored.invitation.expires_at_millis(),
        })
    }

    /// Makes the signed-in user a member of the invitation's company, in its role. The user must
    /// be the address it was sent to: a link that leaks, or is forwarded, ties the invitation to
    /// that address and no one else's account, but does not prove who owns the mailbox: the token
    /// is what does ([ADR 0027]).
    pub async fn accept(&self, token: &str, user: &User) -> Result<CompanyId, ApplicationError> {
        let stored = self.usable(token).await?;
        let invitation = stored.invitation;
        if user.email() != invitation.email() {
            return Err(ApplicationError::InvitationForAnotherEmail);
        }
        // The invitation is claimed first, in one atomic step that also checks it is still open and
        // unexpired: an accept that races a withdrawal, or the end of its time, loses, and no
        // membership is made behind a refusal. If the membership then cannot be written the claim is
        // given back, so the person can try again.
        self.invitations
            .claim(invitation.id(), now_millis())
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::InvitationNoLongerValid,
                other => other.into(),
            })?;
        match self
            .identities
            .add_membership(user.id(), invitation.company_id(), invitation.role())
            .await
        {
            // Already a member (a second accept that raced the first): the company is theirs.
            Ok(()) | Err(RepositoryError::DuplicateMembership) => {}
            Err(error) => {
                // Best effort: if even this fails the invitation stays used, and the person asks
                // for a new one.
                let _ = self.invitations.release(invitation.id()).await;
                return Err(error.into());
            }
        }
        Ok(invitation.company_id())
    }

    /// Creates an account for the address an invitation was sent to, and makes it a member of the
    /// invitation's company in its role, with **no company of its own**. The account must be for
    /// that address: the token reached it by e-mail, so signing up through the link is the proof
    /// of the mailbox that an ordinary sign-up lacks ([ADR 0027]). Everything is checked before
    /// anything is stored (the password, the invitation, the address), so a refusal leaves no account
    /// behind; an address that already has one is `DuplicateEmail`, and the person signs in instead.
    pub async fn join(
        &self,
        token: &str,
        name: &str,
        email: &str,
        password: &str,
    ) -> Result<(User, CompanyId), ApplicationError> {
        validate_password(password)?;
        let stored = self.usable(token).await?;
        let user = User::new(UserId::generate(), name, email)?;
        if user.email() != stored.invitation.email() {
            return Err(ApplicationError::InvitationForAnotherEmail);
        }
        let password_hash = self.secrets.hash_password(password).await?;
        self.identities
            .register_user(&NewUser {
                user: user.clone(),
                password_hash,
            })
            .await?;
        // The invitation could be withdrawn in the instant since it was read: then the account
        // exists and the person signs in to find no company, and asks for a new invitation.
        let company_id = self.accept(token, &user).await?;
        Ok((user, company_id))
    }

    /// The invitation a token leads to, if it can still be accepted. An unknown token and a token
    /// that was used, withdrawn or has expired are told apart: the first is a mistake in the link,
    /// the others are things the person can ask to have fixed.
    async fn usable(&self, token: &str) -> Result<StoredInvitation, ApplicationError> {
        let stored = self
            .invitations
            .find_by_token_digest(&self.secrets.digest_token(token))
            .await?
            .ok_or(ApplicationError::InvitationNotFound)?;
        if stored.state != InvitationState::Open || stored.invitation.is_expired_at(now_millis()) {
            return Err(ApplicationError::InvitationNoLongerValid);
        }
        Ok(stored)
    }
}

/// The text on one line: every control character, a line break included, becomes a space.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}
