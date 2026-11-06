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
    NoPriceListItems,
    TooManyPriceListItems,
    DuplicatePriceListProduct,
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
    InvalidTransferAmount,
    TransferToTheSameAccount,
    InvalidNote,
    InvalidAttachmentName,
    InvalidChargeAmount,
    InvalidChargeProvider,
    InvalidChargePayment,
    ChargeIsNotOpen,
    ChargeIsPaid,
    InvalidChargeNote,
    InvalidPaymentEvent,
    TooManyDunningRules,
    InvalidDunningDays,
    DuplicateDunningRule,
    UnsupportedAttachmentType,
    InvalidAttachmentSize,
    InvalidOpeningBalance,
    InvalidStatementAmount,
    InvalidStatementDescription,
    LineIsNotPending,
    LineIsNotMatched,
    LineIsNotIgnored,
    InvalidRecurrenceAmount,
    RecurrenceEndsBeforeItStarts,
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
    InvalidLineQuantity,
    NoLines,
    TooManyLines,
    DiscountExceedsLine,
    DocumentTooLarge,
    InvalidQuoteNotes,
    QuoteValidityBeforeIssue,
    QuoteIsLocked,
    QuoteIsNotSent,
    EntryIsVoided,
    SaleIsLocked,
    SaleIsNotCompleted,
    SaleIsEmpty,
    InvalidReturnQuantity,
    NoReturnItems,
    ReturnLineUnknown,
    ReturnRefundIsNothing,
    InstallmentTooSmall,
    ReturnExceedsSold,
    EntryFollowsASale,
    ContractIsTerminated,
    InvalidAdjustment,
    ExpectedBeforeOrdered,
    PurchaseIsLocked,
    PurchaseIsEmpty,
    PurchaseIsNotReceivable,
    NoReceiptItems,
    ReceiptLineUnknown,
    InvalidReceiptQuantity,
    ReceiptExceedsOrdered,
    InvalidStockQuantity,
    StockCostRequired,
    StockCostNotAllowed,
    StockMovementIsAReversal,
    NotStocked,
    CountIsLocked,
    NoCountLines,
    TooManyCountLines,
    DuplicateCountLine,
    InvalidCountQuantity,
    InvalidAccessKey,
    InvoiceKeyMismatch(&'static str),
    InvoiceTotalsMismatch(&'static str),
    InvalidInvoiceAmount(&'static str),
    InvoiceHasNoItems,
    TooManyInvoiceItems,
    InvoiceQuantityTooPrecise,
    InvoiceInstallmentsMismatch,
    ScheduleDoesNotMatchReceipt,
    OrderFollowsAnInvoice,
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
            Self::InvalidTransferAmount => {
                f.write_str("amount must be more than 0, in cents (at most two decimals)")
            }
            Self::TransferToTheSameAccount => {
                f.write_str("a transfer moves money between two different accounts")
            }
            Self::InvalidNote => {
                f.write_str("note must be one line of at most 200 characters, without control characters")
            }
            Self::InvalidRecurrenceAmount => {
                f.write_str("amount must be more than 0, in cents (at most two decimals)")
            }
            Self::RecurrenceEndsBeforeItStarts => {
                f.write_str("ends_on must not be before starts_on")
            }
            Self::InvalidStatementAmount => f.write_str(
                "amount must be more than 0 or less than 0, in cents (at most two decimals)",
            ),
            Self::InvalidStatementDescription => f.write_str(
                "a statement line has a description of one line and at most 200 characters, and a reference of at most 100",
            ),
            Self::LineIsNotPending => f.write_str("the statement line is not pending"),
            Self::LineIsNotMatched => f.write_str("the statement line is not matched"),
            Self::LineIsNotIgnored => f.write_str("the statement line is not ignored"),
            Self::InvalidChargeAmount => {
                f.write_str("a charge is for an amount above 0, in cents")
            }
            Self::InvalidChargeProvider => {
                f.write_str("the charge's provider key or id is not valid")
            }
            Self::InvalidChargePayment => {
                f.write_str("the charge's payment details do not fit its method")
            }
            Self::ChargeIsNotOpen => f.write_str("the charge is not open"),
            Self::ChargeIsPaid => f.write_str("the charge is paid already"),
            Self::InvalidChargeNote => f.write_str("a charge's note is one short line"),
            Self::TooManyDunningRules => f.write_str("a company has at most 20 reminder rules"),
            Self::InvalidDunningDays => {
                f.write_str("a reminder goes out from 60 days before to 180 days after the due date")
            }
            Self::DuplicateDunningRule => {
                f.write_str("a reminder rule is for one day and channel once")
            }
            Self::InvalidPaymentEvent => f.write_str("the provider's notice is not valid"),
            Self::InvalidAttachmentName => f.write_str(
                "file name must be one line of at most 200 characters, with no slash or control characters",
            ),
            Self::UnsupportedAttachmentType => f.write_str(
                "file type must be PDF, PNG, JPEG, WebP, plain text, CSV or XML",
            ),
            Self::InvalidAttachmentSize => f.write_str("file must have from 1 byte to 10 MB"),
            Self::InvalidOpeningBalance => f.write_str(
                "balance must be a number in cents (at most two decimals), negative if the account starts in debt",
            ),
            Self::SettlementMovesNothing => f.write_str(
                "a settlement must move some money: what is applied, with interest and fine, less the discount, must be more than 0",
            ),
            Self::TooManyEntryInstallments => f.write_str("an entry has at most 120 installments"),
            Self::NoInstallments => f.write_str("a payment term needs at least one installment"),
            Self::TooManyInstallments => f.write_str("a payment term has at most 24 installments"),
            Self::NoPriceListItems => f.write_str("a price list needs at least one price"),
            Self::TooManyPriceListItems => f.write_str("a price list has at most 1000 prices"),
            Self::DuplicatePriceListProduct => {
                f.write_str("a price list names each product once")
            }
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
            Self::InvalidLineQuantity => f.write_str(
                "quantity must be more than 0, with at most 3 decimals, written like 1.5",
            ),
            Self::NoLines => f.write_str("a document needs at least one line"),
            Self::TooManyLines => f.write_str("a document has at most 100 lines"),
            Self::DiscountExceedsLine => {
                f.write_str("a discount cannot be more than the value of its line")
            }
            Self::DocumentTooLarge => f.write_str("the total is more than the system holds"),
            Self::InvalidQuoteNotes => f.write_str(
                "notes must have at most 2000 characters and no null characters",
            ),
            Self::QuoteValidityBeforeIssue => {
                f.write_str("valid_until must not be before issued_on")
            }
            Self::QuoteIsLocked => {
                f.write_str("a quote that was approved, declined or turned into a sale cannot change")
            }
            Self::QuoteIsNotSent => f.write_str("only a quote that was sent can be answered"),
            Self::EntryIsVoided => f.write_str("the entry was cancelled: it cannot change, and it counts nowhere"),
            Self::SaleIsLocked => f.write_str("only an order that is still open can change"),
            Self::SaleIsNotCompleted => f.write_str("only a completed sale has returns"),
            Self::SaleIsEmpty => f.write_str("a sale to be completed must be worth more than 0"),
            Self::InvalidReturnQuantity => f.write_str(
                "a returned quantity must be more than 0, with at most 3 decimals, written like 1.5",
            ),
            Self::NoReturnItems => f.write_str("a return needs at least one item"),
            Self::ReturnLineUnknown => f.write_str("a return is of the items the sale has, each once"),
            Self::ReturnRefundIsNothing => f.write_str("what is returned is worth less than one cent"),
            Self::InstallmentTooSmall => f.write_str(
                "the amount is too small to be split by this payment term: an installment would be less than one cent",
            ),
            Self::ReturnExceedsSold => f.write_str("a return cannot be of more than was sold and not returned yet"),
            Self::EntryFollowsASale => f.write_str(
                "an entry made by a sale, a return or a purchase receipt changes through that document, not on its own",
            ),
            Self::ExpectedBeforeOrdered => {
                f.write_str("the expected day cannot be before the day of the order")
            }
            Self::PurchaseIsLocked => f.write_str(
                "a purchase order can change only while it is a draft, and be cancelled only before anything arrives",
            ),
            Self::PurchaseIsEmpty => {
                f.write_str("a purchase order to be approved must be worth more than 0")
            }
            Self::PurchaseIsNotReceivable => {
                f.write_str("only an approved or partly received purchase order can receive goods")
            }
            Self::NoReceiptItems => f.write_str("a receipt needs at least one item"),
            Self::ReceiptLineUnknown => {
                f.write_str("a receipt is of the items the order has, each once")
            }
            Self::InvalidReceiptQuantity => f.write_str(
                "a received quantity must be more than 0, with at most 3 decimals, written like 1.5",
            ),
            Self::InvalidStockQuantity => f.write_str(
                "a quantity of stock must be more than 0 (an adjustment may be negative), with at most 3 decimals, written like 1.5",
            ),
            Self::StockCostRequired => f.write_str(
                "stock that comes in needs its cost: the total in cents, or the cost of a unit, not both",
            ),
            Self::StockCostNotAllowed => f.write_str(
                "only stock that comes in is given a cost: the rest is valued at the average cost",
            ),
            Self::StockMovementIsAReversal => {
                f.write_str("a movement that undoes another is not itself undone")
            }
            Self::NotStocked => f.write_str("a service has no stock"),
            Self::CountIsLocked => {
                f.write_str("only a count that is open can change, be applied or be cancelled")
            }
            Self::InvalidAccessKey => f.write_str(
                "the access key must be the 44 digits of the invoice, with a right check digit",
            ),
            Self::InvoiceKeyMismatch(what) => {
                write!(f, "the invoice's {what} is not the one its access key says")
            }
            Self::InvoiceTotalsMismatch(what) => {
                write!(f, "the invoice's {what} does not add up to what it declares")
            }
            Self::InvalidInvoiceAmount(what) => {
                write!(f, "the invoice has an invalid {what}")
            }
            Self::InvoiceHasNoItems => f.write_str("the invoice has no items"),
            Self::TooManyInvoiceItems => f.write_str("a purchase has at most 100 items"),
            Self::InvoiceQuantityTooPrecise => f.write_str(
                "an item's quantity has more than 3 decimals, which stock does not keep",
            ),
            Self::InvoiceInstallmentsMismatch => f.write_str(
                "the invoice's installments must add up to its value and not go back in time",
            ),
            Self::ScheduleDoesNotMatchReceipt => f.write_str(
                "when a receipt is paid on given days, the parts must add up to its value, each above 0, in days that never go back",
            ),
            Self::OrderFollowsAnInvoice => f.write_str(
                "an order entered from an NF-e is received by entering the invoice, not on its own",
            ),
            Self::NoCountLines => f.write_str("a count needs at least one product"),
            Self::TooManyCountLines => f.write_str("a count has at most 500 products"),
            Self::DuplicateCountLine => f.write_str("a product is counted once in a count"),
            Self::InvalidCountQuantity => f.write_str(
                "a counted quantity is 0 or more, with at most 3 decimals, written like 1.5",
            ),
            Self::ReceiptExceedsOrdered => {
                f.write_str("a receipt cannot be of more than was ordered and not received yet")
            }
            Self::ContractIsTerminated => f.write_str("a terminated contract is fixed"),
            Self::InvalidAdjustment => f.write_str(
                "an adjustment is a percentage between -99.99 and 1000, with at most two decimals",
            ),
            Self::KindCannotChange => f.write_str("the kind cannot change once registered"),
        }
    }
}

impl std::error::Error for DomainError {}
