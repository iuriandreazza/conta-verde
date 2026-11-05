use std::collections::HashMap;

use conta_verde_application::{PostalCodeDirectory, PostalCodeEntry, PostalCodeLookupError};
use conta_verde_domain::PostalCode;

/// A directory held in memory, for tests: it knows the postal codes it was given and answers
/// "not found" for the rest, or "unavailable" for all of them when built with `unreachable`.
#[derive(Default)]
pub struct InMemoryPostalCodeDirectory {
    entries: HashMap<String, PostalCodeEntry>,
    unreachable: bool,
}

impl InMemoryPostalCodeDirectory {
    /// A directory that never answers, like one whose service is down.
    pub fn unreachable() -> Self {
        Self {
            unreachable: true,
            ..Self::default()
        }
    }

    /// `code` is written as the domain keeps it: eight digits.
    pub fn with_entry(mut self, code: &str, entry: PostalCodeEntry) -> Self {
        self.entries.insert(code.to_owned(), entry);
        self
    }
}

impl PostalCodeDirectory for InMemoryPostalCodeDirectory {
    async fn lookup(&self, code: &PostalCode) -> Result<PostalCodeEntry, PostalCodeLookupError> {
        if self.unreachable {
            return Err(PostalCodeLookupError::Unavailable);
        }
        self.entries
            .get(code.as_str())
            .cloned()
            .ok_or(PostalCodeLookupError::NotFound)
    }
}
