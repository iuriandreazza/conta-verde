use conta_verde_domain::PostalCode;

use crate::{ApplicationError, PostalCodeDirectory, PostalCodeEntry, PostalCodeLookupError};

/// Finds the address of a postal code so a form can be filled in. Nothing is stored and nothing is
/// recorded: a lookup is not a change to any record.
pub struct PostalCodeService<D> {
    directory: D,
}

impl<D: PostalCodeDirectory> PostalCodeService<D> {
    pub fn new(directory: D) -> Self {
        Self { directory }
    }

    /// The address for a postal code written with or without the hyphen. A code that is not eight
    /// digits is refused without asking the directory, which is a third party.
    pub async fn lookup(&self, raw: &str) -> Result<PostalCodeEntry, ApplicationError> {
        let code = PostalCode::parse(raw)?;
        self.directory
            .lookup(&code)
            .await
            .map_err(|error| match error {
                PostalCodeLookupError::NotFound => ApplicationError::PostalCodeNotFound,
                PostalCodeLookupError::Unavailable => ApplicationError::PostalCodeLookupUnavailable,
                PostalCodeLookupError::Disabled => ApplicationError::PostalCodeLookupDisabled,
            })
    }
}
