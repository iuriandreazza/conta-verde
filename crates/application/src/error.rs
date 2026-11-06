use std::fmt;

use conta_verde_domain::DomainError;

use crate::SecretError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    DuplicateDocument,
    DuplicateEmail,
    DuplicateSku,
    /// The name is taken by another record of the same kind and place (a cost center of the
    /// company, a category under the same parent), ignoring case.
    DuplicateName,
    DuplicateMembership,
    DuplicateInvitation,
    /// The company already has a category for transfers between its accounts.
    DuplicateTransferCategory,
    /// The company already has an account with this bank, branch and number.
    DuplicateBankAccount,
    /// An update matched no row.
    NotFound,
    /// A settlement would settle more of an installment than is left of it.
    Overpaid,
    /// The entry has settlements that are not reversed, so its installments and kind are fixed.
    EntrySettled,
    /// The account already has this statement line.
    DuplicateLine,
    /// Another statement line is matched to the settlement or the transfer.
    AlreadyMatched,
    /// The statement line is no longer in the status the move was made from.
    LineChanged,
    /// The recurrence already has the entry of that occurrence day.
    DuplicateOccurrence,
    Storage(String),
}

/// Why a request was not allowed to proceed. `Forbidden` is also used for companies that do not exist,
/// so callers cannot probe which company ids are taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidCredentials,
    Unauthenticated,
    Forbidden,
    /// A member of the company whose role does not reach what was asked.
    NotAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    Domain(DomainError),
    Repository(RepositoryError),
    Auth(AuthError),
    Secret(SecretError),
    /// No party with this id in the company (another company's party looks the same).
    PartyNotFound,
    /// No product or service with this id in the company (another company's looks the same).
    ProductNotFound,
    /// No category with this id in the company (another company's looks the same).
    CategoryNotFound,
    /// The category a new one was to go under does not exist in the company.
    ParentCategoryNotFound,
    /// An active category is never under an inactive one.
    CategoryParentInactive,
    /// A category with active subcategories cannot be deactivated.
    CategoryHasActiveChildren,
    /// The default template is for a company that has no categories yet.
    CategoriesAlreadyExist,
    CategoryLimitReached,
    /// No bank account with this id in the company (another company's looks the same).
    BankAccountNotFound,
    /// No payment term with this id in the company (another company's looks the same).
    PaymentTermNotFound,
    /// No financial entry with this id in the company (another company's looks the same).
    EntryNotFound,
    /// No attachment with this id in the entry (another company's looks the same), or it was removed
    /// already.
    AttachmentNotFound,
    /// An entry holds a limited number of files at once.
    TooManyAttachments,
    /// A cash flow was asked for a span that is not one or is too long to cut.
    InvalidSpan(&'static str),
    /// No statement line with this id in the company (another company's looks the same).
    StatementLineNotFound,
    /// Another statement line is already matched to the settlement or the transfer.
    AlreadyMatched,
    /// The statement line was moved by someone else while this was being done.
    LineChanged,
    /// The settlement or the transfer is not one that fits the line: another account, reversed, not the
    /// same amount or direction, or too far from its day.
    MatchDoesNotFit,
    /// A file has more lines than one import takes.
    TooManyImportRows,
    /// No recurrence with this id in the company (another company's looks the same).
    RecurrenceNotFound,
    /// No transfer with this id in the company, or it was reversed already.
    TransferNotFound,
    /// No settlement with this id in the entry (another company's looks the same), or it was
    /// reversed already.
    SettlementNotFound,
    /// What is applied would settle more of the installment than is left of it.
    SettlementExceedsBalance,
    /// The entry has settlements that are not reversed: its installments and kind are fixed until
    /// they are reversed ([ADR 0030]).
    EntryIsSettled,
    /// A category, cost center or party an entry points at is not usable in the company: it does
    /// not exist there, or is inactive, or is the transfers category. Names the field.
    UnusableReference(&'static str),
    /// No payment method with this id in the company (another company's looks the same).
    PaymentMethodNotFound,
    /// No cost center with this id in the company (another company's looks the same).
    CostCenterNotFound,
    /// No invitation behind this token or id (another company's id looks the same).
    InvitationNotFound,
    /// The user is not a member of the company (or no longer is).
    MemberNotFound,
    /// A company has one owner, who is not changed, removed or given to another ([ADR 0028]).
    OwnerIsFixed,
    /// The invitation was accepted, withdrawn or has expired.
    InvitationNoLongerValid,
    /// The signed-in user is not the address the invitation was sent to.
    InvitationForAnotherEmail,
    /// The operator switched the CNPJ lookup off, so there is nothing to ask.
    LookupDisabled,
    /// The party's CNPJ was re-checked a moment ago; the registry has a shared rate limit.
    RecheckTooSoon,
    /// The postal code service has no address for it, or could not tell (its 404 covers both).
    PostalCodeNotFound,
    /// The postal code service did not give a usable answer: network, timeout, rate limit, error.
    PostalCodeLookupUnavailable,
    /// The operator switched the postal code lookup off, so there is nothing to ask.
    PostalCodeLookupDisabled,
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
            Self::DuplicateSku => f.write_str("a product with this SKU already exists"),
            Self::DuplicateName => f.write_str("this name is already in use"),
            Self::DuplicateMembership => {
                f.write_str("this user is already a member of the company")
            }
            Self::DuplicateInvitation => {
                f.write_str("this e-mail address already has an open invitation to the company")
            }
            Self::DuplicateBankAccount => {
                f.write_str("a bank account with this bank, branch and number already exists")
            }
            Self::DuplicateTransferCategory => {
                f.write_str("the company already has a category for transfers between accounts")
            }
            Self::NotFound => f.write_str("no matching record"),
            Self::Overpaid => {
                f.write_str("this settles more of the installment than is left of it")
            }
            Self::EntrySettled => f.write_str("the entry has settlements that are not reversed"),
            Self::DuplicateLine => f.write_str("the account already has this statement line"),
            Self::AlreadyMatched => f.write_str("another statement line is already matched to it"),
            Self::LineChanged => f.write_str("the statement line was changed meanwhile"),
            Self::DuplicateOccurrence => {
                f.write_str("the recurrence already has the entry of this occurrence")
            }
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
            Self::NotAllowed => "your role in this company does not allow this",
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
            Self::PartyNotFound => f.write_str("party not found"),
            Self::ProductNotFound => f.write_str("product not found"),
            Self::CostCenterNotFound => f.write_str("cost center not found"),
            Self::BankAccountNotFound => f.write_str("bank account not found"),
            Self::PaymentMethodNotFound => f.write_str("payment method not found"),
            Self::PaymentTermNotFound => f.write_str("payment term not found"),
            Self::EntryNotFound => f.write_str("entry not found"),
            Self::AttachmentNotFound => f.write_str("attachment not found"),
            Self::TooManyAttachments => {
                f.write_str("an entry holds at most 20 attachments; remove one to attach another")
            }
            Self::SettlementNotFound => f.write_str("settlement not found"),
            Self::TransferNotFound => f.write_str("transfer not found"),
            Self::RecurrenceNotFound => f.write_str("recurrence not found"),
            Self::StatementLineNotFound => f.write_str("statement line not found"),
            Self::AlreadyMatched => f.write_str("another statement line is already matched to it"),
            Self::LineChanged => f.write_str("the statement line was changed meanwhile: read it again"),
            Self::MatchDoesNotFit => f.write_str(
                "that settlement or transfer does not fit the line: it has to be in the same account, not reversed, for the same amount in the same direction, and within five days",
            ),
            Self::TooManyImportRows => f.write_str("the file has more lines than one import takes"),
            Self::InvalidSpan(reason) => write!(f, "invalid span: {reason}"),
            Self::SettlementExceedsBalance => {
                f.write_str("this settles more of the installment than is left of it")
            }
            Self::EntryIsSettled => f.write_str(
                "the installments and the kind of an entry with settlements cannot change: reverse them first",
            ),
            Self::UnusableReference(field) => {
                write!(
                    f,
                    "the {field} does not exist in the company, or cannot be used here"
                )
            }
            Self::CategoryNotFound => f.write_str("category not found"),
            Self::ParentCategoryNotFound => f.write_str("the parent category does not exist"),
            Self::CategoryParentInactive => {
                f.write_str("the parent category is inactive; reactivate it first")
            }
            Self::CategoryHasActiveChildren => {
                f.write_str("deactivate the subcategories of this category first")
            }
            Self::CategoriesAlreadyExist => f.write_str(
                "the default categories can only be applied to a company without categories",
            ),
            Self::CategoryLimitReached => f.write_str("a company can have at most 1000 categories"),
            Self::InvitationNotFound => f.write_str("invitation not found"),
            Self::MemberNotFound => f.write_str("member not found"),
            Self::OwnerIsFixed => f.write_str(
                "a company has one owner, who cannot be changed, removed or given to another",
            ),
            Self::InvitationNoLongerValid => f.write_str(
                "this invitation was already used or withdrawn, or has expired: ask for a new one",
            ),
            Self::InvitationForAnotherEmail => f.write_str(
                "this invitation was sent to another e-mail address: sign in with that one",
            ),
            Self::LookupDisabled => f.write_str("CNPJ lookup is switched off on this installation"),
            Self::RecheckTooSoon => {
                f.write_str("this CNPJ was checked a moment ago, try again in a few seconds")
            }
            Self::PostalCodeNotFound => f.write_str(
                "no address found for this postal code, or the service could not tell: \
                 check the number or fill the address in by hand",
            ),
            Self::PostalCodeLookupUnavailable => f.write_str(
                "the postal code service is not answering right now: fill the address in by hand",
            ),
            Self::PostalCodeLookupDisabled => {
                f.write_str("postal code lookup is switched off on this installation")
            }
        }
    }
}

impl std::error::Error for RepositoryError {}
impl std::error::Error for AuthError {}
impl std::error::Error for ApplicationError {}
