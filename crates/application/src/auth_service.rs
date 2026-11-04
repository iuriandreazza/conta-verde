use std::time::Duration;

use conta_verde_domain::{
    Account, AccountDraft, Company, CompanyId, Email, User, UserId, validate_password,
};

use crate::{
    ApplicationError, AuthError, IdentityRepository, NewAccount, SecretService, StoredCredentials,
};

/// A user and the companies they can switch between.
#[derive(Debug, Clone)]
pub struct Profile {
    pub user: User,
    pub companies: Vec<Company>,
}

/// The outcome of registering or logging in: the raw session token is shown to the client once.
#[derive(Debug, Clone)]
pub struct SignedIn {
    pub token: String,
    pub profile: Profile,
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
                companies: vec![account.company],
            },
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
                let companies = self.identities.companies_of(user.id()).await?;
                Ok(SignedIn {
                    token,
                    profile: Profile { user, companies },
                })
            }
            _ => Err(AuthError::InvalidCredentials.into()),
        }
    }

    pub async fn logout(&self, token: &str) -> Result<(), ApplicationError> {
        let digest = self.secrets.digest_session_token(token);
        Ok(self.identities.delete_session(&digest).await?)
    }

    pub async fn profile(&self, token: &str) -> Result<Profile, ApplicationError> {
        let user = self.authenticate(token).await?;
        let companies = self.identities.companies_of(user.id()).await?;
        Ok(Profile { user, companies })
    }

    /// Proves the session owner belongs to the company. Companies the user cannot see and
    /// companies that do not exist are indistinguishable on purpose.
    pub async fn authorize(
        &self,
        token: &str,
        company_id: CompanyId,
    ) -> Result<User, ApplicationError> {
        let user = self.authenticate(token).await?;
        if self.identities.is_member(user.id(), company_id).await? {
            Ok(user)
        } else {
            Err(AuthError::Forbidden.into())
        }
    }

    async fn authenticate(&self, token: &str) -> Result<User, ApplicationError> {
        let digest = self.secrets.digest_session_token(token);
        self.identities
            .find_session_user(&digest)
            .await?
            .ok_or_else(|| AuthError::Unauthenticated.into())
    }

    async fn open_session(&self, user_id: UserId) -> Result<String, ApplicationError> {
        let token = self.secrets.new_session_token();
        let digest = self.secrets.digest_session_token(&token);
        self.identities
            .create_session(user_id, &digest, self.session_lifetime)
            .await?;
        Ok(token)
    }
}
