use std::fmt;

use conta_verde_domain::DomainError;

use crate::SecretError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    DuplicateDocument,
    DuplicateEmail,
    Storage(String),
}

/// Why a request was not allowed to proceed. `Forbidden` is also used for companies that do not exist,
/// so callers cannot probe which company ids are taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidCredentials,
    Unauthenticated,
    Forbidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    Domain(DomainError),
    Repository(RepositoryError),
    Auth(AuthError),
    Secret(SecretError),
}

impl From<SecretError> for ApplicationError {
    fn from(error: SecretError) -> Self {
        Self::Secret(error)
    }
}

impl From<DomainError> for ApplicationError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<RepositoryError> for ApplicationError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

impl From<AuthError> for ApplicationError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDocument => f.write_str("a party with this document already exists"),
            Self::DuplicateEmail => f.write_str("an account with this email already exists"),
            Self::Storage(message) => write!(f, "storage failure: {message}"),
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidCredentials => "invalid email or password",
            Self::Unauthenticated => "authentication required",
            Self::Forbidden => "company not found",
        };
        f.write_str(message)
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(f),
            Self::Repository(error) => error.fmt(f),
            Self::Auth(error) => error.fmt(f),
            Self::Secret(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RepositoryError {}
impl std::error::Error for AuthError {}
impl std::error::Error for ApplicationError {}
