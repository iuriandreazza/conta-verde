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
    NoInstallments,
    TooManyInstallments,
    InvalidDescription,
    /// Names the date: an entry has a competence date and a due date for each installment.
    InvalidDate(&'static str),
    InvalidInstallmentAmount,
    InstallmentDatesDecrease,
    NoEntryInstallments,
    InvalidInstallmentNumber,
    /// Names the amount: what is applied, or the interest, fine or discount.
    InvalidSettlementAmount(&'static str),
    SettlementMovesNothing,
    TooManyEntryInstallments,
    InvalidInstallmentDays,
    InstallmentDaysNotIncreasing,
    InvalidPercent,
    PercentsDoNotSumTo100,
    InvalidPostalCode,
    InvalidState,
    /// Names the field: an address has several free-text parts.
    InvalidAddressField(&'static str),
    /// Names the field: a contact has a name and a job title.
    InvalidContactField(&'static str),
    InvalidPhone,
    TooManyContacts,
    InvalidTag,
    TooManyTags,
    OwnerCannotBeInvited,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => f.write_str("name must not be empty"),
            Self::InvalidDocument => f.write_str("document is not a valid CPF or CNPJ"),
            Self::InvalidEmail => f.write_str("email is not valid"),
            Self::MissingRole => f.write_str("party must be a customer, a supplier, a carrier or a seller"),
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
            Self::InvalidDescription => f.write_str(
                "description must be one line of 1 to 200 characters, without control characters",
            ),
            Self::InvalidDate(field) => {
                write!(f, "{field} must be a real date written YYYY-MM-DD, from 1900 to 2200")
            }
            Self::InvalidInstallmentAmount => f.write_str(
                "an installment amount must be more than 0, in cents (at most two decimals)",
            ),
            Self::InstallmentDatesDecrease => {
                f.write_str("the due dates must not decrease from one installment to the next")
            }
            Self::NoEntryInstallments => f.write_str("an entry needs at least one installment"),
            Self::InvalidInstallmentNumber => {
                f.write_str("installment must be a number from 1 to 120")
            }
            Self::InvalidSettlementAmount("applied") => {
                f.write_str("applied must be more than 0, in cents (at most two decimals)")
            }
            Self::InvalidSettlementAmount(field) => {
                write!(f, "{field} must be 0 or more, in cents (at most two decimals)")
            }
            Self::SettlementMovesNothing => f.write_str(
                "a settlement must move some money: what is applied, with interest and fine, less the discount, must be more than 0",
            ),
            Self::TooManyEntryInstallments => f.write_str("an entry has at most 120 installments"),
            Self::NoInstallments => f.write_str("a payment term needs at least one installment"),
            Self::TooManyInstallments => f.write_str("a payment term has at most 24 installments"),
            Self::InvalidInstallmentDays => {
                f.write_str("an installment is due in 0 to 3650 days")
            }
            Self::InstallmentDaysNotIncreasing => {
                f.write_str("the days of the installments must increase, each later than the one before")
            }
            Self::InvalidPercent => f.write_str(
                "a percentage must be more than 0 and at most 100, with at most 2 decimals, written like 33.33",
            ),
            Self::PercentsDoNotSumTo100 => f.write_str("the percentages must add up to exactly 100"),
            Self::InvalidPostalCode => f.write_str("postal code (CEP) must have 8 digits"),
            Self::InvalidState => f.write_str("state must be one of the 27 two-letter abbreviations, like SP"),
            Self::InvalidAddressField(field) => write!(
                f,
                "address {field} is required, or too long or has null characters"
            ),
            Self::InvalidContactField(field) => write!(
                f,
                "contact {field} is required, or too long or has null characters"
            ),
            Self::InvalidPhone => f.write_str(
                "phone must have 8 to 15 digits, written with only spaces and the characters + - ( ) .",
            ),
            Self::TooManyContacts => f.write_str("a party has at most 20 contacts"),
            Self::InvalidTag => f.write_str(
                "a tag has 1 to 30 characters, with no commas or control characters",
            ),
            Self::TooManyTags => f.write_str("a party has at most 10 tags"),
            Self::OwnerCannotBeInvited => {
                f.write_str("nobody is invited as an owner: creating a company is what makes one")
            }
            Self::KindCannotChange => f.write_str("the kind cannot change once registered"),
        }
    }
}

impl std::error::Error for DomainError {}
