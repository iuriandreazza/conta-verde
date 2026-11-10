use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use conta_verde_application::{
    ApplicationError, AuthError, ProviderError, RepositoryError, WebhookError,
};
use conta_verde_domain::DomainError;
use serde::Serialize;

#[derive(Debug)]
pub(crate) enum ApiError {
    Application(ApplicationError),
    InvalidCursor,
    /// A request body that is not the JSON this endpoint takes.
    InvalidBody(axum::extract::rejection::JsonRejection),
    /// A list parameter this API does not understand; the message names which one.
    InvalidQuery(&'static str),
    /// Query parameters that are not even the right shape, like `limit=abc`.
    InvalidParameters(axum::extract::rejection::QueryRejection),
    /// A body field this endpoint needs that the request left out.
    InvalidField(&'static str),
    /// An import file that cannot be read as a whole; the message says what to change.
    InvalidFile(String),
    /// A file past the size this API reads; the message says how large that is.
    FileTooLarge(&'static str),
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::InvalidCursor => (StatusCode::BAD_REQUEST, "invalid cursor".to_owned()),
            Self::InvalidBody(rejection) => crate::json::describe(rejection),
            Self::InvalidParameters(rejection) => (rejection.status(), rejection.body_text()),
            Self::InvalidField(message) => {
                (StatusCode::UNPROCESSABLE_ENTITY, (*message).to_owned())
            }
            Self::InvalidQuery(message) => (StatusCode::BAD_REQUEST, (*message).to_owned()),
            Self::InvalidFile(message) => (StatusCode::UNPROCESSABLE_ENTITY, message.clone()),
            Self::FileTooLarge(message) => (StatusCode::PAYLOAD_TOO_LARGE, (*message).to_owned()),
            Self::Application(error) => describe(error),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

fn describe(error: &ApplicationError) -> (StatusCode, String) {
    match error {
        // The quote is not in a status that allows it: not the request's fault, but the state's.
        ApplicationError::IdempotencyMismatch => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::Domain(
            error @ (DomainError::QuoteIsLocked
            | DomainError::QuoteIsNotSent
            | DomainError::SaleIsLocked
            | DomainError::SaleIsNotCompleted
            | DomainError::PurchaseIsLocked
            | DomainError::OrderFollowsAnInvoice
            | DomainError::CountIsLocked
            | DomainError::StockMovementIsAReversal
            | DomainError::PurchaseIsNotReceivable
            | DomainError::ReceiptExceedsOrdered
            | DomainError::EntryIsVoided
            | DomainError::ReturnExceedsSold
            | DomainError::EntryFollowsASale
            | DomainError::ApiKeyIsNotActive
            | DomainError::ChargeIsNotOpen
            | DomainError::ContractIsTerminated),
        ) => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::Domain(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
        ApplicationError::Repository(
            error @ (RepositoryError::DuplicateDocument
            | RepositoryError::DuplicateEmail
            | RepositoryError::DuplicateSku
            | RepositoryError::DuplicateName
            | RepositoryError::DuplicateTransferCategory
            | RepositoryError::DuplicateBankAccount
            | RepositoryError::DuplicateMembership
            | RepositoryError::Overpaid
            | RepositoryError::EntrySettled
            | RepositoryError::DuplicateOrigin
            | RepositoryError::DuplicateLine
            | RepositoryError::AlreadyMatched
            | RepositoryError::LineChanged
            | RepositoryError::DuplicateInvitation
            | RepositoryError::QuoteChanged
            | RepositoryError::EntryVoided
            | RepositoryError::SaleChanged
            | RepositoryError::PurchaseChanged
            | RepositoryError::CountChanged
            | RepositoryError::DuplicateInvoice
            | RepositoryError::StockChanged
            | RepositoryError::StockInsufficient
            | RepositoryError::DuplicateReversal
            | RepositoryError::ReceiptTooMuch
            | RepositoryError::DuplicateQuoteSale
            | RepositoryError::DuplicateCycle
            | RepositoryError::ContractChanged
            | RepositoryError::ReturnTooMuch
            | RepositoryError::SaleHasReturns
            | RepositoryError::ChargeAlreadyOpen
            | RepositoryError::FiscalDocumentAlreadyLive
            | RepositoryError::FiscalDocumentChanged
            | RepositoryError::GatewaySettingsChanged
            | RepositoryError::DuplicateFiscalFile
            | RepositoryError::DuplicateFiscalDps),
        ) => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::Auth(error) => {
            let status = match error {
                AuthError::InvalidCredentials | AuthError::Unauthenticated => {
                    StatusCode::UNAUTHORIZED
                }
                // Same answer for "not yours" and "does not exist", so ids cannot be probed.
                AuthError::Forbidden => StatusCode::NOT_FOUND,
                // A member, but not of a role that reaches this: the company is theirs to see.
                AuthError::NotAllowed => StatusCode::FORBIDDEN,
                // Like an expired quote link: the link existed or never did, and either way it no longer works.
                AuthError::InvalidResetLink | AuthError::InvalidConfirmationLink => {
                    StatusCode::GONE
                }
            };
            (status, error.to_string())
        }
        ApplicationError::PartyNotFound
        | ApplicationError::Repository(RepositoryError::NotFound) => {
            (StatusCode::NOT_FOUND, "party not found".to_owned())
        }
        ApplicationError::CategoryParentInactive
        | ApplicationError::CategoryHasActiveChildren
        | ApplicationError::CategoriesAlreadyExist
        | ApplicationError::CategoryLimitReached => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::ParentCategoryNotFound
        | ApplicationError::SupplierNotRegistered
        | ApplicationError::InvoiceItemNeedsAProduct(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
        }
        ApplicationError::ProductNotFound
        | ApplicationError::ApiKeyNotFound
        | ApplicationError::CostCenterNotFound
        | ApplicationError::BankAccountNotFound
        | ApplicationError::PaymentMethodNotFound
        | ApplicationError::PaymentTermNotFound
        | ApplicationError::PriceListNotFound
        | ApplicationError::ChargeNotFound
        | ApplicationError::FiscalDocumentNotFound
        | ApplicationError::CategoryNotFound => (StatusCode::NOT_FOUND, error.to_string()),
        ApplicationError::LookupDisabled | ApplicationError::PostalCodeLookupDisabled => {
            (StatusCode::CONFLICT, error.to_string())
        }
        ApplicationError::QuoteChanged
        | ApplicationError::ContractChanged
        | ApplicationError::SaleChanged
        | ApplicationError::PurchaseChanged
        | ApplicationError::CountChanged
        | ApplicationError::StockChanged
        | ApplicationError::StockMovementFollowsADocument
        | ApplicationError::StockInsufficient { .. }
        | ApplicationError::PeriodLocked { .. }
        | ApplicationError::EntryIsVoided
        | ApplicationError::SaleHasSettlements
        | ApplicationError::SaleHasReturns
        | ApplicationError::ChargeFromAnotherProvider
        | ApplicationError::QuoteAlreadyConverted => (StatusCode::CONFLICT, error.to_string()),
        // It existed and cannot be approved any more: its validity has run out.
        ApplicationError::QuoteExpired => (StatusCode::GONE, error.to_string()),
        ApplicationError::QuoteNotFound
        | ApplicationError::SaleNotFound
        | ApplicationError::PurchaseNotFound
        | ApplicationError::CountNotFound
        | ApplicationError::StockMovementNotFound
        | ApplicationError::PurchaseReceiptNotFound
        | ApplicationError::ContractNotFound
        | ApplicationError::CommissionRuleNotFound
        | ApplicationError::EntryNotFound
        | ApplicationError::StatementLineNotFound
        | ApplicationError::RecurrenceNotFound
        | ApplicationError::SettlementNotFound
        | ApplicationError::AttachmentNotFound
        | ApplicationError::TransferNotFound => (StatusCode::NOT_FOUND, error.to_string()),
        ApplicationError::SettlementExceedsBalance
        | ApplicationError::AddressNeverConfirmed
        | ApplicationError::CannotMail(_)
        | ApplicationError::EntryIsSettled
        | ApplicationError::TooManyAttachments => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::AlreadyMatched | ApplicationError::LineChanged => {
            (StatusCode::CONFLICT, error.to_string())
        }
        ApplicationError::MatchDoesNotFit
        | ApplicationError::TooManyImportRows
        | ApplicationError::ExportTooLarge(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
        }
        ApplicationError::InvalidSpan(_) => (StatusCode::BAD_REQUEST, error.to_string()),
        ApplicationError::UnusableReference(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
        }
        ApplicationError::InvitationNotFound | ApplicationError::MemberNotFound => {
            (StatusCode::NOT_FOUND, error.to_string())
        }
        ApplicationError::OwnerIsFixed => (StatusCode::CONFLICT, error.to_string()),
        // It existed and cannot be used any more: the link was used, withdrawn or has expired.
        ApplicationError::InvitationNoLongerValid => (StatusCode::GONE, error.to_string()),
        ApplicationError::InvitationForAnotherEmail => (StatusCode::FORBIDDEN, error.to_string()),
        ApplicationError::NoEmailAddress => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
        ApplicationError::Webhook(WebhookError::InvalidSignature) => {
            (StatusCode::UNAUTHORIZED, error.to_string())
        }
        ApplicationError::Webhook(WebhookError::Malformed(detail)) => {
            tracing::warn!(%detail, "a signed webhook was not a notice");
            (StatusCode::BAD_REQUEST, error.to_string())
        }
        ApplicationError::CollectionNotConfigured => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::ChargeNeedsReceivable
        | ApplicationError::ChargeNeedsParty
        | ApplicationError::NothingToCharge => {
            (StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
        }
        // The operator has configured none: nothing to ask, like a lookup that is switched off.
        ApplicationError::Provider(ProviderError::Disabled) => (
            StatusCode::CONFLICT,
            "payments are not configured on this installation".to_owned(),
        ),
        // What the provider said stays in the log: it can carry what a client should not see.
        ApplicationError::Provider(ProviderError::Rejected(detail)) => {
            tracing::warn!(%detail, "the payment provider refused a request");
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "the payment provider refused the charge".to_owned(),
            )
        }
        ApplicationError::Provider(ProviderError::Unavailable(detail)) => {
            tracing::warn!(%detail, "the payment provider is unavailable");
            (
                StatusCode::BAD_GATEWAY,
                "the payment provider is not reachable".to_owned(),
            )
        }
        ApplicationError::FiscalRule(_) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
        // The operator has configured none, or another CNPJ: nothing to ask, like a payment provider that is off.
        ApplicationError::FiscalGateway(
            conta_verde_application::GatewayError::Disabled
            | conta_verde_application::GatewayError::WrongIssuer,
        ) => (StatusCode::CONFLICT, error.to_string()),
        ApplicationError::FiscalGateway(conta_verde_application::GatewayError::Unsupported(_)) => {
            (StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
        }
        // The text can name the gateway's answer: it stays in the log.
        ApplicationError::FiscalGateway(conta_verde_application::GatewayError::Unavailable(
            detail,
        )) => {
            tracing::warn!(%detail, "the fiscal gateway is unavailable");
            (
                StatusCode::BAD_GATEWAY,
                "the fiscal gateway is not reachable".to_owned(),
            )
        }
        ApplicationError::PostalCodeNotFound => (StatusCode::NOT_FOUND, error.to_string()),
        ApplicationError::PostalCodeLookupUnavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, error.to_string())
        }
        ApplicationError::RecheckTooSoon | ApplicationError::MailRefused(_) => {
            (StatusCode::TOO_MANY_REQUESTS, error.to_string())
        }
        // Internal details can leak: log them, but show clients a generic message.
        ApplicationError::Repository(RepositoryError::Storage(detail)) => internal(detail),
        ApplicationError::Secret(error) => internal(&error.to_string()),
        ApplicationError::SecretUnreadable(_) => (StatusCode::CONFLICT, error.to_string()),
    }
}

fn internal(detail: &str) -> (StatusCode, String) {
    tracing::error!(%detail, "internal failure");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal error".to_owned(),
    )
}
