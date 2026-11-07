use std::sync::{Arc, Mutex};

use conta_verde_application::{
    EmailError, EmailMessage, EmailSender, InvitationRepository, InvitationState, NewInvitation,
    RepositoryError, StoredInvitation,
};
use conta_verde_domain::{CompanyId, Invitation, InvitationId};

use crate::InMemoryIdentityRepository;

struct Stored {
    invitation: Invitation,
    token_digest: String,
    state: InvitationState,
}

/// Invitations held in memory, for tests. It asks the identity repository it was built over for the
/// names of companies, as the database joins them.
#[derive(Clone)]
pub struct InMemoryInvitationRepository {
    identities: InMemoryIdentityRepository,
    invitations: Arc<Mutex<Vec<Stored>>>,
}

impl InMemoryInvitationRepository {
    pub fn new(identities: InMemoryIdentityRepository) -> Self {
        Self {
            identities,
            invitations: Arc::default(),
        }
    }
}

impl InvitationRepository for InMemoryInvitationRepository {
    async fn create(
        &self,
        new_invitation: &NewInvitation,
        now_millis: i64,
    ) -> Result<(), RepositoryError> {
        let invitation = &new_invitation.invitation;
        let mut stored = self.invitations.lock().expect("repository lock poisoned");
        let in_the_way = |other: &Stored| {
            other.state == InvitationState::Open
                && other.invitation.company_id() == invitation.company_id()
                && other.invitation.email() == invitation.email()
        };
        if stored
            .iter()
            .any(|other| in_the_way(other) && !other.invitation.is_expired_at(now_millis))
        {
            return Err(RepositoryError::DuplicateInvitation);
        }
        // An open one that has expired is withdrawn, as the database does.
        for other in stored.iter_mut().filter(|other| in_the_way(other)) {
            other.state = InvitationState::Revoked;
        }
        stored.push(Stored {
            invitation: invitation.clone(),
            token_digest: new_invitation.token_digest.clone(),
            state: InvitationState::Open,
        });
        Ok(())
    }

    async fn find_by_token_digest(
        &self,
        token_digest: &str,
    ) -> Result<Option<StoredInvitation>, RepositoryError> {
        let stored = self.invitations.lock().expect("repository lock poisoned");
        let Some(found) = stored
            .iter()
            .find(|other| other.token_digest == token_digest)
        else {
            return Ok(None);
        };
        Ok(Some(StoredInvitation {
            invitation: found.invitation.clone(),
            company_name: self
                .identities
                .company_name(found.invitation.company_id())
                .unwrap_or_default(),
            state: found.state,
        }))
    }

    async fn open_invitations(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<Invitation>, RepositoryError> {
        let stored = self.invitations.lock().expect("repository lock poisoned");
        let mut open: Vec<Invitation> = stored
            .iter()
            .filter(|other| {
                other.state == InvitationState::Open && other.invitation.company_id() == company_id
            })
            .map(|other| other.invitation.clone())
            .collect();
        open.sort_by_key(|invitation| (invitation.created_at_millis(), invitation.id().as_uuid()));
        Ok(open)
    }

    async fn revoke(&self, company_id: CompanyId, id: InvitationId) -> Result<(), RepositoryError> {
        let mut stored = self.invitations.lock().expect("repository lock poisoned");
        let found = stored.iter_mut().find(|other| {
            other.state == InvitationState::Open
                && other.invitation.id() == id
                && other.invitation.company_id() == company_id
        });
        match found {
            Some(other) => {
                other.state = InvitationState::Revoked;
                Ok(())
            }
            None => Err(RepositoryError::NotFound),
        }
    }

    async fn claim(&self, id: InvitationId, now_millis: i64) -> Result<(), RepositoryError> {
        let mut stored = self.invitations.lock().expect("repository lock poisoned");
        let found = stored.iter_mut().find(|other| {
            other.state == InvitationState::Open
                && other.invitation.id() == id
                && !other.invitation.is_expired_at(now_millis)
        });
        match found {
            Some(other) => {
                other.state = InvitationState::Accepted;
                Ok(())
            }
            None => Err(RepositoryError::NotFound),
        }
    }

    async fn release(&self, id: InvitationId) -> Result<(), RepositoryError> {
        let mut stored = self.invitations.lock().expect("repository lock poisoned");
        let found = stored
            .iter_mut()
            .find(|other| other.state == InvitationState::Accepted && other.invitation.id() == id);
        match found {
            Some(other) => {
                other.state = InvitationState::Open;
                Ok(())
            }
            None => Err(RepositoryError::NotFound),
        }
    }
}

/// An e-mail sender that keeps what it was asked to send, for tests, and can be made to fail.
#[derive(Clone, Default)]
pub struct InMemoryEmailSender {
    sent: Arc<Mutex<Vec<EmailMessage>>>,
    broken: bool,
}

impl InMemoryEmailSender {
    /// A sender whose every send fails, like an unreachable mail server.
    pub fn broken() -> Self {
        Self {
            broken: true,
            ..Self::default()
        }
    }

    pub fn sent(&self) -> Vec<EmailMessage> {
        self.sent.lock().expect("sender lock poisoned").clone()
    }

    /// Forgets what was sent, so a test can look at what one step sends and nothing before it.
    pub fn clear(&self) {
        self.sent.lock().expect("sender lock poisoned").clear();
    }
}

impl EmailSender for InMemoryEmailSender {
    async fn send(&self, message: &EmailMessage) -> Result<(), EmailError> {
        if self.broken {
            return Err(EmailError);
        }
        self.sent
            .lock()
            .expect("sender lock poisoned")
            .push(message.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use conta_verde_application::{AuthService, MemberService};
    use conta_verde_domain::{AccountDraft, Actor, Role};

    use super::*;
    use crate::{Argon2SecretService, contract};

    fn secrets() -> Argon2SecretService {
        Argon2SecretService::new().unwrap()
    }

    #[tokio::test]
    async fn runs_the_invitation_lifecycle() {
        let identities = InMemoryIdentityRepository::default();
        let emails = InMemoryEmailSender::default();
        let auth = AuthService::new(identities.clone(), secrets(), Duration::from_secs(3600));
        // Both services see the same invitations, as two handles to one database would.
        let invitations = InMemoryInvitationRepository::new(identities.clone());
        let normal = MemberService::new(
            invitations.clone(),
            identities.clone(),
            emails.clone(),
            secrets(),
            Duration::from_secs(7 * 86_400),
            "https://erp.example",
        );
        let short = MemberService::new(
            invitations,
            identities.clone(),
            emails.clone(),
            secrets(),
            Duration::from_millis(1),
            "https://erp.example",
        );
        contract::runs_the_invitation_lifecycle(&auth, &normal, &short, &emails).await;
    }

    #[tokio::test]
    async fn claims_an_invitation_once_and_only_while_it_lasts() {
        let identities = InMemoryIdentityRepository::default();
        let invitations = InMemoryInvitationRepository::new(identities.clone());
        contract::claims_an_invitation_once_and_only_while_it_lasts(&invitations, &identities)
            .await;
    }

    #[tokio::test]
    async fn an_email_that_cannot_be_sent_does_not_fail_the_invitation() {
        let identities = InMemoryIdentityRepository::default();
        let auth = AuthService::new(identities.clone(), secrets(), Duration::from_secs(3600));
        let ana = auth
            .register(
                AccountDraft {
                    user_name: "Ana".into(),
                    email: "ana@example.com".into(),
                    company_name: "Padaria".into(),
                },
                "correct horse",
            )
            .await
            .unwrap()
            .profile;
        let service = MemberService::new(
            InMemoryInvitationRepository::new(identities.clone()),
            identities,
            InMemoryEmailSender::broken(),
            secrets(),
            Duration::from_secs(86_400),
            "https://erp.example/",
        );
        let actor = Actor::User {
            id: ana.user.id(),
            name: "Ana".into(),
        };
        let invited = service
            .invite(
                ana.memberships[0].company.id(),
                &actor,
                "bia@example.com",
                Role::Viewer,
            )
            .await
            .unwrap();
        // The invitation exists and its link is there to pass on; only the e-mail was not sent.
        assert!(!invited.emailed);
        assert!(invited.link.starts_with("https://erp.example/invitations/"));
    }

    #[tokio::test]
    async fn a_line_break_in_a_name_cannot_add_a_header_or_forge_a_log_line() {
        let identities = InMemoryIdentityRepository::default();
        let auth = AuthService::new(identities.clone(), secrets(), Duration::from_secs(3600));
        let ana = auth
            .register(
                AccountDraft {
                    user_name: "Ana\r\nBcc: x@example.com".into(),
                    email: "ana@example.com".into(),
                    company_name: "Padaria\nSubject: oops".into(),
                },
                "correct horse",
            )
            .await
            .unwrap()
            .profile;
        let emails = InMemoryEmailSender::default();
        let service = MemberService::new(
            InMemoryInvitationRepository::new(identities.clone()),
            identities,
            emails.clone(),
            secrets(),
            Duration::from_secs(86_400),
            "https://erp.example",
        );
        let actor = Actor::User {
            id: ana.user.id(),
            name: ana.user.name().to_owned(),
        };
        service
            .invite(
                ana.memberships[0].company.id(),
                &actor,
                "bia@example.com",
                Role::Viewer,
            )
            .await
            .unwrap();
        let message = &emails.sent()[0];
        assert!(
            !message.subject.chars().any(char::is_control),
            "{:?}",
            message.subject
        );
        // Only the template's own line breaks are left in the body.
        assert!(!message.body.contains('\r'));
        assert_eq!(message.body.matches('\n').count(), 6, "{:?}", message.body);
    }
}
