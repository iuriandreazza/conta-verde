use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use conta_verde_application::{ApplicationError, AuthError, RepositoryError};
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
    /// An import file past the size this API reads.
    FileTooLarge,
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
            Self::FileTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "the file is larger than 1 MiB: split it".to_owned(),
            ),
            Self::Application(error) => describe(error),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

fn describe(error: &ApplicationError) -> (StatusCode, String) {
    match error {
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
            | RepositoryError::DuplicateOccurrence
            | RepositoryError::DuplicateLine
            | RepositoryError::AlreadyMatched
            | RepositoryError::LineChanged
            | RepositoryError::DuplicateInvitation),
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
        ApplicationError::ParentCategoryNotFound => {
            (StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
        }
        ApplicationError::ProductNotFound
        | ApplicationError::CostCenterNotFound
        | ApplicationError::BankAccountNotFound
        | ApplicationError::PaymentMethodNotFound
        | ApplicationError::PaymentTermNotFound
        | ApplicationError::CategoryNotFound => (StatusCode::NOT_FOUND, error.to_string()),
        ApplicationError::LookupDisabled | ApplicationError::PostalCodeLookupDisabled => {
            (StatusCode::CONFLICT, error.to_string())
        }
        ApplicationError::EntryNotFound
        | ApplicationError::StatementLineNotFound
        | ApplicationError::RecurrenceNotFound
        | ApplicationError::SettlementNotFound
        | ApplicationError::TransferNotFound => (StatusCode::NOT_FOUND, error.to_string()),
        ApplicationError::SettlementExceedsBalance | ApplicationError::EntryIsSettled => {
            (StatusCode::CONFLICT, error.to_string())
        }
        ApplicationError::AlreadyMatched | ApplicationError::LineChanged => {
            (StatusCode::CONFLICT, error.to_string())
        }
        ApplicationError::MatchDoesNotFit | ApplicationError::TooManyImportRows => {
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
        ApplicationError::PostalCodeNotFound => (StatusCode::NOT_FOUND, error.to_string()),
        ApplicationError::PostalCodeLookupUnavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, error.to_string())
        }
        ApplicationError::RecheckTooSoon => (StatusCode::TOO_MANY_REQUESTS, error.to_string()),
        // Internal details can leak: log them, but show clients a generic message.
        ApplicationError::Repository(RepositoryError::Storage(detail)) => internal(detail),
        ApplicationError::Secret(error) => internal(&error.to_string()),
    }
}

fn internal(detail: &str) -> (StatusCode, String) {
    tracing::error!(%detail, "internal failure");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal error".to_owned(),
    )
}
