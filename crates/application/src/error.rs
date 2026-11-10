use std::fmt;

use conta_verde_domain::{CalendarDate, DomainError};

use crate::{ProviderError, SecretError, WebhookError};

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
    /// The origin (an occurrence of a recurrence, a sale, a return) already has its entry.
    DuplicateOrigin,
    /// The entry is cancelled: it takes no more settlements.
    EntryVoided,
    /// The sale is no longer in the status the write was made from.
    SaleChanged,
    /// The purchase order is no longer in the status the write was made from.
    PurchaseChanged,
    /// The count is no longer in the status the write was made from.
    CountChanged,
    /// The NF-e already has its purchase order.
    DuplicateInvoice,
    /// A receipt would take in more of a line than was ordered.
    ReceiptTooMuch,
    /// A product has another balance than the one the movements were valued against.
    StockChanged,
    /// A write would take a product's stock below zero.
    StockInsufficient,
    /// The movement was already undone.
    DuplicateReversal,
    /// The quote already became a sale.
    DuplicateQuoteSale,
    /// The contract already has the sale of that cycle.
    DuplicateCycle,
    /// The contract was terminated while the write was being made.
    ContractChanged,
    /// A return would take back more of a line than was sold.
    ReturnTooMuch,
    /// The sale has returns, which stand.
    SaleHasReturns,
    /// The quote is no longer in the status the write was made from.
    QuoteChanged,
    /// The installment already has an open charge.
    ChargeAlreadyOpen,
    /// The sale already has a live fiscal document of this kind.
    FiscalDocumentAlreadyLive,
    /// The fiscal document is no longer in the status the write was made from.
    FiscalDocumentChanged,
    /// The fiscal document already has a file of this kind.
    DuplicateFiscalFile,
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
    /// A password reset link that does not exist, has expired or was already used.
    InvalidResetLink,
    /// An address confirmation link that does not exist, has expired or was already used.
    InvalidConfirmationLink,
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
    /// No price list with this id in the company (another company's looks the same).
    PriceListNotFound,
    /// No charge with this id in the entry (another company's looks the same).
    ChargeNotFound,
    /// Only a receivable is charged: a payable is what the company owes.
    ChargeNeedsReceivable,
    /// A charge is made out to someone: the entry has no customer.
    ChargeNeedsParty,
    /// The installment is settled: there is nothing left to charge.
    NothingToCharge,
    /// Charges land in an account the company has not chosen yet (the collection settings).
    CollectionNotConfigured,
    /// The provider's webhook was refused: the signature is not its, or the body is not a notice.
    Webhook(WebhookError),
    /// The charge was made at another provider than the one configured now, which cannot call it off.
    ChargeFromAnotherProvider,
    /// The payment provider could not make or call off the charge.
    Provider(ProviderError),
    /// No financial entry with this id in the company (another company's looks the same).
    EntryNotFound,
    /// No attachment with this id in the entry (another company's looks the same), or it was removed
    /// already.
    AttachmentNotFound,
    /// An entry holds a limited number of files at once.
    TooManyAttachments,
    /// A cash flow was asked for a span that is not one or is too long to cut.
    InvalidSpan(&'static str),
    /// A package for the accountant would be bigger than one request holds; narrow the span ([ADR 0071]).
    ExportTooLarge(&'static str),
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
    /// No sale with this id in the company (another company's looks the same).
    SaleNotFound,
    /// No contract with this id in the company (another company's looks the same).
    ContractNotFound,
    /// No commission rule with this id in the company (another company's looks the same).
    CommissionRuleNotFound,
    /// The contract was changed while this was being done.
    ContractChanged,
    /// The sale was changed while this was being done.
    SaleChanged,
    /// No stock movement with this id in the company (another company's looks the same).
    StockMovementNotFound,
    /// The stock changed under the write and could not be valued again in a few tries.
    StockChanged,
    /// The day is in a closed period ([ADR 0069]): the last day that is closed.
    PeriodLocked {
        through: CalendarDate,
    },
    /// A product would be left with less than nothing: the name and what is on hand.
    StockInsufficient {
        product: String,
        available: String,
    },
    /// A movement a document made is undone through the document.
    StockMovementFollowsADocument,
    /// No supplier of the company has the registry number the invoice names: it is registered first.
    SupplierNotRegistered,
    /// An item of the invoice was not given the product it becomes.
    InvoiceItemNeedsAProduct(u16),
    /// No inventory count with this id in the company (another company's looks the same).
    CountNotFound,
    /// The count was changed while this was being done.
    CountChanged,
    /// No purchase order with this id in the company (another company's looks the same).
    PurchaseNotFound,
    /// No receipt with this id in the purchase order.
    PurchaseReceiptNotFound,
    /// The purchase order was changed while this was being done.
    PurchaseChanged,
    /// The entry is cancelled: it cannot change or be settled.
    EntryIsVoided,
    /// The sale's receivable has settlements that are not reversed: reverse them before cancelling.
    SaleHasSettlements,
    /// The quote already became a sale.
    QuoteAlreadyConverted,
    /// The sale has returns: they stand, so the sale cannot be cancelled.
    SaleHasReturns,
    /// No quote with this id in the company, or no link that leads to one.
    QuoteNotFound,
    /// The quote was changed or answered while this was being done.
    QuoteChanged,
    /// The quote's validity has run out: it can no longer be approved.
    QuoteExpired,
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
    /// A document that cannot be e-mailed in the state it is in; says why.
    CannotMail(&'static str),
    /// A sign-up for an address held by an account that was never confirmed: a link to recover it went to that address.
    AddressNeverConfirmed,
    /// The e-mail was not sent because of a limit; says which.
    MailRefused(&'static str),
    /// Nobody to send the e-mail to: the party has no address and none was given.
    NoEmailAddress,
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

impl From<ProviderError> for ApplicationError {
    fn from(error: ProviderError) -> Self {
        Self::Provider(error)
    }
}

impl From<WebhookError> for ApplicationError {
    fn from(error: WebhookError) -> Self {
        Self::Webhook(error)
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
            Self::DuplicateOrigin => f.write_str("this origin already has its entry"),
            Self::EntryVoided => f.write_str("the entry was cancelled"),
            Self::SaleChanged => f.write_str("the sale was changed meanwhile"),
            Self::PurchaseChanged => f.write_str("the purchase order was changed meanwhile"),
            Self::CountChanged => f.write_str("the count was changed meanwhile"),
            Self::DuplicateInvoice => f.write_str("this invoice was already entered"),
            Self::ReceiptTooMuch => f.write_str("a receipt cannot be of more than was ordered"),
            Self::StockChanged => f.write_str("the stock changed meanwhile"),
            Self::StockInsufficient => f.write_str("not enough stock"),
            Self::DuplicateReversal => f.write_str("the movement was already undone"),
            Self::DuplicateQuoteSale => f.write_str("the quote already became a sale"),
            Self::DuplicateCycle => f.write_str("the contract already has the sale of this cycle"),
            Self::ContractChanged => f.write_str("the contract was changed meanwhile"),
            Self::ReturnTooMuch => f.write_str("a return cannot be of more than was sold"),
            Self::SaleHasReturns => f.write_str("the sale has returns"),
            Self::QuoteChanged => f.write_str("the quote was changed meanwhile"),
            Self::ChargeAlreadyOpen => f.write_str("the installment already has an open charge"),
            Self::FiscalDocumentAlreadyLive => {
                f.write_str("the sale already has a fiscal document in progress or authorized")
            }
            Self::FiscalDocumentChanged => f.write_str("the fiscal document was changed meanwhile"),
            Self::DuplicateFiscalFile => {
                f.write_str("the fiscal document already has a file of this kind")
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
            Self::InvalidResetLink | Self::InvalidConfirmationLink => {
                "this link has expired or was already used"
            }
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
            Self::PriceListNotFound => f.write_str("price list not found"),
            Self::ChargeNotFound => f.write_str("charge not found"),
            Self::ChargeNeedsReceivable => f.write_str("only a receivable can be charged"),
            Self::ChargeNeedsParty => f.write_str("the entry has no customer to charge"),
            Self::NothingToCharge => f.write_str("the installment has nothing left to charge"),
            Self::CollectionNotConfigured => {
                f.write_str("choose the account charges are paid into before issuing one")
            }
            Self::Webhook(error) => match error {
                WebhookError::InvalidSignature => f.write_str("invalid signature"),
                WebhookError::Malformed(_) => f.write_str("the notice is not valid"),
            },
            Self::ChargeFromAnotherProvider => {
                f.write_str("the charge was made at another payment provider than the configured one")
            }
            Self::Provider(error) => error.fmt(f),
            Self::EntryNotFound => f.write_str("entry not found"),
            Self::AttachmentNotFound => f.write_str("attachment not found"),
            Self::TooManyAttachments => {
                f.write_str("an entry holds at most 20 attachments; remove one to attach another")
            }
            Self::SettlementNotFound => f.write_str("settlement not found"),
            Self::TransferNotFound => f.write_str("transfer not found"),
            Self::RecurrenceNotFound => f.write_str("recurrence not found"),
            Self::QuoteNotFound => f.write_str("quote not found"),
            Self::SaleNotFound => f.write_str("sale not found"),
            Self::ContractNotFound => f.write_str("contract not found"),
            Self::CommissionRuleNotFound => f.write_str("commission rule not found"),
            Self::ContractChanged => f.write_str("the contract was changed meanwhile: read it again"),
            Self::SaleChanged => f.write_str("the sale was changed meanwhile: read it again"),
            Self::PurchaseNotFound => f.write_str("purchase order not found"),
            Self::CountNotFound => f.write_str("count not found"),
            Self::SupplierNotRegistered => f.write_str(
                "no supplier is registered with the issuer's CNPJ or CPF of the invoice: register it first",
            ),
            Self::InvoiceItemNeedsAProduct(number) => {
                write!(f, "item {number} of the invoice needs the product it becomes")
            }
            Self::CountChanged => f.write_str("the count was changed meanwhile: read it again"),
            Self::StockMovementNotFound => f.write_str("stock movement not found"),
            Self::StockChanged => f.write_str("the stock changed meanwhile: try again"),
            Self::PeriodLocked { through } => write!(
                f,
                "the period through {through} is closed: reopen it to change what is dated on or before it"
            ),
            Self::StockInsufficient { product, available } => {
                write!(f, "not enough stock of {product}: {available} on hand")
            }
            Self::StockMovementFollowsADocument => f.write_str(
                "a movement made by a sale, a receipt or a count is undone through that document",
            ),
            Self::PurchaseReceiptNotFound => f.write_str("receipt not found"),
            Self::PurchaseChanged => {
                f.write_str("the purchase order was changed meanwhile: read it again")
            }
            Self::EntryIsVoided => f.write_str("the entry was cancelled: it cannot change or be settled"),
            Self::SaleHasSettlements => f.write_str(
                "the receivable of this sale has settlements: reverse them before cancelling it",
            ),
            Self::QuoteAlreadyConverted => f.write_str("the quote already became a sale"),
            Self::SaleHasReturns => f.write_str("the sale has returns, which stand: it cannot be cancelled"),
            Self::QuoteChanged => f.write_str("the quote was changed meanwhile: read it again"),
            Self::QuoteExpired => f.write_str("the quote is past its validity"),
            Self::StatementLineNotFound => f.write_str("statement line not found"),
            Self::AlreadyMatched => f.write_str("another statement line is already matched to it"),
            Self::LineChanged => f.write_str("the statement line was changed meanwhile: read it again"),
            Self::MatchDoesNotFit => f.write_str(
                "that settlement or transfer does not fit the line: it has to be in the same account, not reversed, for the same amount in the same direction, and within five days",
            ),
            Self::TooManyImportRows => f.write_str("the file has more lines than one import takes"),
            Self::InvalidSpan(reason) => write!(f, "invalid span: {reason}"),
            Self::ExportTooLarge(what) => {
                write!(f, "too much for one export ({what}): choose a shorter period")
            }
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
            Self::AddressNeverConfirmed => f.write_str(
                "this address has an account that was never confirmed: a link to recover it was sent to the address",
            ),
            Self::CannotMail(why) | Self::MailRefused(why) => f.write_str(why),
            Self::NoEmailAddress => {
                f.write_str("there is no e-mail address to send to: register one for the party or give one")
            }
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
