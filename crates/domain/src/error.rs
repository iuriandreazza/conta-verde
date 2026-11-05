use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyName,
    InvalidDocument,
    InvalidEmail,
    MissingRole,
    WeakPassword,
    NoCnpjToVerify,
    InvalidName,
    InvalidSku,
    InvalidUnit,
    InvalidNcm,
    /// Names the field: a price and a stock are all written the same way.
    InvalidAmount(&'static str),
    ServiceCannotHaveNcm,
    ServiceCannotHaveMinimumStock,
    KindCannotChange,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => f.write_str("name must not be empty"),
            Self::InvalidDocument => f.write_str("document is not a valid CPF or CNPJ"),
            Self::InvalidEmail => f.write_str("email is not valid"),
            Self::MissingRole => f.write_str("party must be a customer, a supplier or both"),
            Self::WeakPassword => f.write_str("password must have between 8 and 128 characters"),
            Self::NoCnpjToVerify => f.write_str("the party has no CNPJ to verify"),
            Self::InvalidName => {
                f.write_str("name must have at most 200 characters and no null characters")
            }
            Self::InvalidSku => {
                f.write_str("SKU must have at most 60 letters, digits or the characters . - _ /")
            }
            Self::InvalidUnit => {
                f.write_str("unit must have 1 to 6 letters or digits, like UN or KG")
            }
            Self::InvalidNcm => f.write_str("NCM must have 8 digits"),
            Self::InvalidAmount(field) => write!(
                f,
                "{field} must be a decimal with up to 13 integer and 6 fractional digits, written like 10.50"
            ),
            Self::ServiceCannotHaveNcm => f.write_str("a service has no NCM"),
            Self::ServiceCannotHaveMinimumStock => f.write_str("a service has no stock"),
            Self::KindCannotChange => {
                f.write_str("a product cannot become a service, nor the other way around")
            }
        }
    }
}

impl std::error::Error for DomainError {}
