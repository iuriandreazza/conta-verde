use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyName,
    InvalidDocument,
    InvalidEmail,
    MissingRole,
    WeakPassword,
    NoCnpjToVerify,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyName => "name must not be empty",
            Self::InvalidDocument => "document is not a valid CPF or CNPJ",
            Self::InvalidEmail => "email is not valid",
            Self::MissingRole => "party must be a customer, a supplier or both",
            Self::WeakPassword => "password must have between 8 and 128 characters",
            Self::NoCnpjToVerify => "the party has no CNPJ to verify",
        };
        f.write_str(message)
    }
}

impl std::error::Error for DomainError {}
