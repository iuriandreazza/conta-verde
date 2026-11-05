use crate::{CompanyId, DomainError, Email, InvitationId, Role, UserId};

/// An offer to join a company in a role, sent to an e-mail address. It carries no secret: the link
/// that accepts it holds a token whose digest the store keeps ([ADR 0027]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    id: InvitationId,
    company_id: CompanyId,
    email: Email,
    role: Role,
    invited_by: UserId,
    created_at_millis: i64,
    expires_at_millis: i64,
}

impl Invitation {
    /// `now_millis` and `lifetime_millis` are milliseconds, the first since the Unix epoch.
    pub fn new(
        id: InvitationId,
        company_id: CompanyId,
        email: &str,
        role: Role,
        invited_by: UserId,
        now_millis: i64,
        lifetime_millis: i64,
    ) -> Result<Self, DomainError> {
        // Owning a company is not something one is invited to: it is what creating one makes you.
        if role == Role::Owner {
            return Err(DomainError::OwnerCannotBeInvited);
        }
        Ok(Self {
            id,
            company_id,
            email: Email::parse(email)?,
            role,
            invited_by,
            created_at_millis: now_millis,
            expires_at_millis: now_millis.saturating_add(lifetime_millis),
        })
    }

    /// Rebuilds an invitation a store holds. Nothing is checked: it was when it was made.
    pub fn restore(
        id: InvitationId,
        company_id: CompanyId,
        email: Email,
        role: Role,
        invited_by: UserId,
        created_at_millis: i64,
        expires_at_millis: i64,
    ) -> Self {
        Self {
            id,
            company_id,
            email,
            role,
            invited_by,
            created_at_millis,
            expires_at_millis,
        }
    }

    pub fn id(&self) -> InvitationId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn email(&self) -> &Email {
        &self.email
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn invited_by(&self) -> UserId {
        self.invited_by
    }

    pub fn created_at_millis(&self) -> i64 {
        self.created_at_millis
    }

    pub fn expires_at_millis(&self) -> i64 {
        self.expires_at_millis
    }

    pub fn is_expired_at(&self, now_millis: i64) -> bool {
        self.expires_at_millis <= now_millis
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(email: &str, role: Role) -> Result<Invitation, DomainError> {
        Invitation::new(
            InvitationId::generate(),
            CompanyId::generate(),
            email,
            role,
            UserId::generate(),
            1_000,
            500,
        )
    }

    #[test]
    fn normalizes_the_email_and_expires_after_its_lifetime() {
        let invitation = build("  Bia@Example.COM ", Role::Sales).unwrap();
        assert_eq!(invitation.email().as_str(), "bia@example.com");
        assert_eq!(invitation.expires_at_millis(), 1_500);
        assert!(!invitation.is_expired_at(1_499));
        assert!(invitation.is_expired_at(1_500));
    }

    #[test]
    fn anyone_but_an_owner_can_be_invited() {
        for role in Role::ALL {
            let result = build("bia@example.com", role);
            if role == Role::Owner {
                assert_eq!(result, Err(DomainError::OwnerCannotBeInvited));
            } else {
                assert!(result.is_ok(), "{role:?}");
            }
        }
    }

    #[test]
    fn refuses_a_malformed_email() {
        assert_eq!(
            build("not-an-email", Role::Viewer),
            Err(DomainError::InvalidEmail)
        );
    }
}
