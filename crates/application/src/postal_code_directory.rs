use std::future::Future;

use conta_verde_domain::PostalCode;

/// The address a postal code stands for, as far as the directory knows it. It is only ever used to
/// fill a form in: whoever submits it goes through the same checks as an address typed by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostalCodeEntry {
    /// A city with one postal code for all of it has no street.
    pub street: Option<String>,
    pub district: Option<String>,
    pub city: String,
    /// The two-letter abbreviation.
    pub state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostalCodeLookupError {
    /// The directory has no address for it. The service this was built on answers the same way
    /// when every one of its own sources failed, so this does not prove the code does not exist.
    NotFound,
    /// No trustworthy answer: network failure, timeout, rate limit or server error. Adapters log
    /// the cause.
    Unavailable,
    /// The operator turned the lookup off, so nothing was asked.
    Disabled,
}

/// Port for finding the address of a postal code. Futures are `Send` so use cases can run on a
/// multi-threaded runtime.
pub trait PostalCodeDirectory: Send + Sync {
    fn lookup(
        &self,
        code: &PostalCode,
    ) -> impl Future<Output = Result<PostalCodeEntry, PostalCodeLookupError>> + Send;
}

/// `None` means the operator switched the lookup off.
impl<T: PostalCodeDirectory> PostalCodeDirectory for Option<T> {
    async fn lookup(&self, code: &PostalCode) -> Result<PostalCodeEntry, PostalCodeLookupError> {
        match self {
            Some(directory) => directory.lookup(code).await,
            None => Err(PostalCodeLookupError::Disabled),
        }
    }
}
