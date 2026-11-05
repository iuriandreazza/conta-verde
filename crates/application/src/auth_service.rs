use std::time::Duration;

use conta_verde_domain::{
    Access, Account, AccountDraft, Area, CompanyId, Email, Role, User, UserId, validate_password,
};

use crate::{
    ApplicationError, AuthError, CompanyMembership, IdentityRepository, NewAccount, SecretService,
    StoredCredentials,
};

/// A user and the companies they can switch between, with their role in each.
#[derive(Debug, Clone)]
pub struct Profile {
    pub user: User,
    pub memberships: Vec<CompanyMembership>,
}

/// A user proven to belong to a company, and what they are there.
#[derive(Debug, Clone)]
pub struct Member {
    pub user: User,
    pub role: Role,
}

impl Member {
    /// Refuses unless the role reaches the area as far as `access`.
    pub fn require(&self, area: Area, access: Access) -> Result<(), AuthError> {
        if self.role.can(area, access) {
            Ok(())
        } else {
            Err(AuthError::NotAllowed)
        }
    }
}

/// The outcome of registering or logging in: the raw session token is shown to the client once.
#[derive(Clone)]
pub struct SignedIn {
    pub token: String,
    pub profile: Profile,
}

// Manual impl so a logged or unwrapped `SignedIn` never prints a live session token.
impl std::fmt::Debug for SignedIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedIn")
            .field("token", &"<redacted>")
            .field("profile", &self.profile)
            .finish()
    }
}

pub struct AuthService<I, S> {
    identities: I,
    secrets: S,
    session_lifetime: Duration,
}

impl<I: IdentityRepository, S: SecretService> AuthService<I, S> {
    pub fn new(identities: I, secrets: S, session_lifetime: Duration) -> Self {
        Self {
            identities,
            secrets,
            session_lifetime,
        }
    }

    pub fn session_lifetime(&self) -> Duration {
        self.session_lifetime
    }

    pub async fn register(
        &self,
        draft: AccountDraft,
        password: &str,
    ) -> Result<SignedIn, ApplicationError> {
        validate_password(password)?;
        let account = Account::new(UserId::generate(), CompanyId::generate(), draft)?;
        let password_hash = self.secrets.hash_password(password).await?;
        self.identities
            .register_account(&NewAccount {
                account: account.clone(),
                password_hash,
            })
            .await?;
        let token = self.open_session(account.user.id()).await?;
        Ok(SignedIn {
            token,
            profile: Profile {
                user: account.user,
                // The person who creates a company is its owner.
                memberships: vec![CompanyMembership {
                    company: account.company,
                    role: Role::Owner,
                }],
            },
        })
    }

    /// Opens a session for a user who was just created some other way than by registering, like
    /// one who joined a company through an invitation, and says which companies they belong to.
    /// It checks no credential: the caller must have just proved the right to this user, as
    /// `MemberService::join` does by creating it.
    pub async fn sign_in_new_member(&self, user: User) -> Result<SignedIn, ApplicationError> {
        let token = self.open_session(user.id()).await?;
        let memberships = self.identities.memberships_of(user.id()).await?;
        Ok(SignedIn {
            token,
            profile: Profile { user, memberships },
        })
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<SignedIn, ApplicationError> {
        // A malformed email gets the same answer as a wrong password.
        let credentials = match Email::parse(email) {
            Ok(email) => self.identities.find_credentials(&email).await?,
            Err(_) => None,
        };
        let stored_hash = credentials
            .as_ref()
            .map(|credentials| credentials.password_hash.as_str());
        let verified = self.secrets.verify_password(password, stored_hash).await;

        match credentials {
            Some(StoredCredentials { user, .. }) if verified => {
                let token = self.open_session(user.id()).await?;
                let memberships = self.identities.memberships_of(user.id()).await?;
                Ok(SignedIn {
                    token,
                    profile: Profile { user, memberships },
                })
            }
            _ => Err(AuthError::InvalidCredentials.into()),
        }
    }

    pub async fn logout(&self, token: &str) -> Result<(), ApplicationError> {
        let digest = self.secrets.digest_token(token);
        Ok(self.identities.delete_session(&digest).await?)
    }

    pub async fn profile(&self, token: &str) -> Result<Profile, ApplicationError> {
        let user = self.authenticate(token).await?;
        let memberships = self.identities.memberships_of(user.id()).await?;
        Ok(Profile { user, memberships })
    }

    /// Proves the session owner belongs to the company, and says what they are there. Companies
    /// the user cannot see and companies that do not exist are indistinguishable on purpose.
    pub async fn authorize(
        &self,
        token: &str,
        company_id: CompanyId,
    ) -> Result<Member, ApplicationError> {
        let user = self.authenticate(token).await?;
        match self.identities.role_in(user.id(), company_id).await? {
            Some(role) => Ok(Member { user, role }),
            None => Err(AuthError::Forbidden.into()),
        }
    }

    /// The user a session belongs to, for what is asked of a person who is not (yet) a member of any
    /// company in particular, like accepting an invitation.
    pub async fn current_user(&self, token: &str) -> Result<User, ApplicationError> {
        self.authenticate(token).await
    }

    async fn authenticate(&self, token: &str) -> Result<User, ApplicationError> {
        let digest = self.secrets.digest_token(token);
        self.identities
            .find_session_user(&digest)
            .await?
            .ok_or_else(|| AuthError::Unauthenticated.into())
    }

    async fn open_session(&self, user_id: UserId) -> Result<String, ApplicationError> {
        let token = self.secrets.new_token();
        let digest = self.secrets.digest_token(&token);
        self.identities
            .create_session(user_id, &digest, self.session_lifetime)
            .await?;
        Ok(token)
    }
}
