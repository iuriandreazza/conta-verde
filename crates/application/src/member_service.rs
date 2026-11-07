use std::time::{Duration, SystemTime, UNIX_EPOCH};

use conta_verde_domain::{
    Actor, CompanyId, Email, Invitation, InvitationId, Role, User, UserId, validate_password,
};

use crate::text::one_line;
use crate::{
    ApplicationError, AuthError, EmailMessage, EmailSender, IdentityRepository,
    InvitationRepository, InvitationState, Member, NewInvitation, NewUser, RepositoryError,
    SecretService, StoredInvitation,
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

/// How long a password reset link works.
const RESET_LIFETIME: Duration = Duration::from_secs(60 * 60);
/// How long an address confirmation link works.
const CONFIRMATION_LIFETIME: Duration = Duration::from_secs(3 * 24 * 60 * 60);
/// How soon after one link another can be asked for the same account.
const RESET_COOLDOWN: Duration = Duration::from_secs(60);

/// Who belongs to a company, and how people come to ([ADR 0027]). It also gives back an account to its owner when the
/// password is lost ([ADR 0060]): it has what that needs, the accounts, the e-mail and the tokens.
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

    /// Sends a link to choose a new password to the address, when it has an account. It answers the same whether it has
    /// one or not, so the form cannot be used to find out who has an account; and one link a minute at most is made per
    /// account. The link is only in the e-mail, never in the answer.
    pub async fn request_password_reset(&self, email: &str) -> Result<(), ApplicationError> {
        let Ok(email) = Email::parse(email) else {
            return Ok(());
        };
        let Some(credentials) = self.identities.find_credentials(&email).await? else {
            return Ok(());
        };
        let token = self.secrets.new_token();
        let made = self
            .identities
            .create_password_reset(
                credentials.user.id(),
                &self.secrets.digest_token(&token),
                RESET_LIFETIME,
                RESET_COOLDOWN,
            )
            .await?;
        if !made {
            return Ok(());
        }
        let message = EmailMessage {
            to: email,
            subject: "Redefinição de senha no conta-verde".to_owned(),
            body: format!(
                "Recebemos um pedido para redefinir a senha da sua conta no conta-verde.\n\n\
                 Para escolher uma nova senha, abra o link:\n{}/reset-password/{token}\n\n\
                 O link vale por {} minutos e só pode ser usado uma vez. Se não foi você, ignore este e-mail: \
                 sua senha continua a mesma.\n",
                self.public_url,
                RESET_LIFETIME.as_secs() / 60
            ),
        };
        // A message that cannot be sent is the mail server's to say in the log: the person can ask again.
        let _ = self.emails.send(&message).await;
        Ok(())
    }

    /// Chooses a new password with the link's token. The link works once, and every session of the account ends.
    pub async fn reset_password(
        &self,
        token: &str,
        password: &str,
    ) -> Result<(), ApplicationError> {
        validate_password(password)?;
        let password_hash = self.secrets.hash_password(password).await?;
        let user_id = match self
            .identities
            .reset_password(&self.secrets.digest_token(token), &password_hash)
            .await
        {
            Err(RepositoryError::NotFound) => return Err(AuthError::InvalidResetLink.into()),
            other => other?,
        };
        // The link reached the mailbox, so the address is confirmed.
        self.identities.mark_email_verified(user_id).await?;
        // Whoever changed it, the owner of the address is told: if it was not them, this is how they find out.
        if let Some(user) = self.identities.find_user(user_id).await? {
            self.notify(
                user.email().clone(),
                "Sua senha no conta-verde foi alterada".to_owned(),
                format!(
                    "A senha da sua conta no conta-verde foi alterada e você foi desconectado de todos os \
                     aparelhos.\n\nSe foi você, não precisa fazer nada. Se não foi, peça uma nova redefinição \
                     de senha em {}/forgot-password agora.\n",
                    self.public_url
                ),
            )
            .await;
        }
        Ok(())
    }

    /// E-mails the user a link that confirms their address, unless it is confirmed already or one was sent a moment ago.
    /// It is how an account made with a company of its own shows that the address is the person's ([ADR 0063]).
    pub async fn send_email_confirmation(&self, user: &User) -> Result<(), ApplicationError> {
        if self.identities.email_verified(user.id()).await? {
            return Ok(());
        }
        let token = self.secrets.new_token();
        let made = self
            .identities
            .create_email_confirmation(
                user.id(),
                &self.secrets.digest_token(&token),
                CONFIRMATION_LIFETIME,
                RESET_COOLDOWN,
            )
            .await?;
        if made {
            self.notify(
                user.email().clone(),
                "Confirme seu e-mail no conta-verde".to_owned(),
                format!(
                    "Olá, {}.\n\nPara confirmar que este endereço é seu, abra o link:\n{}/confirm-email/{token}\n\n\
                     O link vale por {} dias. Se você não criou uma conta no conta-verde, ignore este e-mail.\n",
                    one_line(user.name()),
                    self.public_url,
                    CONFIRMATION_LIFETIME.as_secs() / 86_400
                ),
            )
            .await;
        }
        Ok(())
    }

    /// Confirms the address the link was sent to. The link works once.
    pub async fn confirm_email(&self, token: &str) -> Result<(), ApplicationError> {
        match self
            .identities
            .confirm_email(&self.secrets.digest_token(token))
            .await
        {
            Err(RepositoryError::NotFound) => Err(AuthError::InvalidConfirmationLink.into()),
            other => other.map(|_| ()).map_err(Into::into),
        }
    }

    /// A notice nobody waits for: if it cannot be queued the mail server's log says so, and what happened stands.
    async fn notify(&self, to: Email, subject: String, body: String) {
        let _ = self.emails.send(&EmailMessage { to, subject, body }).await;
    }

    /// The company's members, with what each is, ordered by name ignoring case.
    pub async fn members(&self, company_id: CompanyId) -> Result<Vec<Member>, ApplicationError> {
        Ok(self.identities.members_of(company_id).await?)
    }

    /// Changes what a member is in the company. The owner is never changed, and nobody is made one.
    pub async fn change_role(
        &self,
        company_id: CompanyId,
        user_id: UserId,
        role: Role,
    ) -> Result<(), ApplicationError> {
        if role == Role::Owner {
            return Err(ApplicationError::OwnerIsFixed);
        }
        self.ensure_removable(company_id, user_id).await?;
        self.identities
            .set_role(user_id, company_id, role)
            .await
            .map_err(member_gone)
    }

    /// Takes a member out of the company: they lose access at their next request, and can be invited
    /// again. The owner cannot be removed.
    pub async fn remove(
        &self,
        company_id: CompanyId,
        user_id: UserId,
    ) -> Result<(), ApplicationError> {
        self.ensure_removable(company_id, user_id).await?;
        self.identities
            .remove_membership(user_id, company_id)
            .await
            .map_err(member_gone)
    }

    /// The owner is told apart from a stranger so the answer says why. The adapters refuse the owner
    /// as well, so a write that races this check still cannot reach it.
    async fn ensure_removable(
        &self,
        company_id: CompanyId,
        user_id: UserId,
    ) -> Result<(), ApplicationError> {
        match self.identities.role_in(user_id, company_id).await? {
            None => Err(ApplicationError::MemberNotFound),
            Some(Role::Owner) => Err(ApplicationError::OwnerIsFixed),
            Some(_) => Ok(()),
        }
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
            Ok(()) => self.tell_the_inviter(&invitation, user).await?,
            // Already a member (a second accept that raced the first): the company is theirs.
            Err(RepositoryError::DuplicateMembership) => {}
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
        // The link reached the mailbox, so the address is confirmed.
        self.identities.mark_email_verified(user.id()).await?;
        let company_id = self.accept(token, &user).await?;
        Ok((user, company_id))
    }

    /// Tells whoever sent the invitation that it was accepted, so they know the person is in.
    async fn tell_the_inviter(
        &self,
        invitation: &Invitation,
        user: &User,
    ) -> Result<(), ApplicationError> {
        let Some(inviter) = self.identities.find_user(invitation.invited_by()).await? else {
            return Ok(());
        };
        let company_name = self
            .identities
            .company_name(invitation.company_id())
            .await?
            .unwrap_or_default();
        let (person, company) = (one_line(user.name()), one_line(&company_name));
        self.notify(
            inviter.email().clone(),
            format!("{person} aceitou seu convite para {company}"),
            format!(
                "{person} ({}) aceitou seu convite e agora faz parte de {company} no conta-verde.\n",
                user.email().as_str()
            ),
        )
        .await;
        Ok(())
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

/// A write that matched no membership means they were removed in the meantime.
fn member_gone(error: RepositoryError) -> ApplicationError {
    match error {
        RepositoryError::NotFound => ApplicationError::MemberNotFound,
        other => other.into(),
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}
