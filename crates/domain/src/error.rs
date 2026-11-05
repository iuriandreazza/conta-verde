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
    TransferGroupsGoTogether,
    TransferCategoryHasNoChildren,
    TransferCategoryStaysActive,
    CategoryTooDeep,
    /// A top-level category has to say where it lands.
    MissingGroups,
    /// A subcategory takes its groups from its parent.
    GroupsAreInherited,
    InvalidBankCode,
    InvalidBranch,
    InvalidAccountNumber,
    CashAccountHasNoBank,
    CardAccountHasNoNumber,
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
            Self::TransferGroupsGoTogether => {
                f.write_str("the transfers DRE group and the transfer cash-flow group go together")
            }
            Self::TransferCategoryHasNoChildren => {
                f.write_str("the transfer category cannot have subcategories")
            }
            Self::TransferCategoryStaysActive => {
                f.write_str("the transfer category can be renamed but not deactivated")
            }
            Self::CategoryTooDeep => f.write_str("categories have at most 5 levels"),
            Self::MissingGroups => {
                f.write_str("a top-level category needs a DRE group and a cash-flow group")
            }
            Self::GroupsAreInherited => {
                f.write_str("a subcategory takes its DRE and cash-flow groups from its parent")
            }
            Self::InvalidBankCode => {
                f.write_str("bank code must have 3 digits, or 8 for an institution without one")
            }
            Self::InvalidBranch => {
                f.write_str("branch must have at most 10 letters or digits, without spaces")
            }
            Self::InvalidAccountNumber => {
                f.write_str("account number must have at most 20 letters or digits, without spaces")
            }
            Self::CashAccountHasNoBank => {
                f.write_str("a cash account has no bank, branch or account number")
            }
            Self::CardAccountHasNoNumber => f.write_str(
                "a card account has no branch or account number: a card number is not stored",
            ),
            Self::KindCannotChange => f.write_str("the kind cannot change once registered"),
        }
    }
}

impl std::error::Error for DomainError {}
