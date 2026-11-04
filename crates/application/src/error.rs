use std::fmt;

use conta_verde_domain::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    DuplicateDocument,
    Storage(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    Domain(DomainError),
    DuplicateDocument,
    Storage(String),
}

impl From<DomainError> for ApplicationError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<RepositoryError> for ApplicationError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::DuplicateDocument => Self::DuplicateDocument,
            RepositoryError::Storage(message) => Self::Storage(message),
        }
    }
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDocument => f.write_str("a party with this document already exists"),
            Self::Storage(message) => write!(f, "storage failure: {message}"),
        }
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(f),
            Self::DuplicateDocument => f.write_str("a party with this document already exists"),
            Self::Storage(message) => write!(f, "storage failure: {message}"),
        }
    }
}

impl std::error::Error for RepositoryError {}
impl std::error::Error for ApplicationError {}
