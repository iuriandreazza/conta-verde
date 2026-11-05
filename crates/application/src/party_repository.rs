use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, Party, PartyId};

use crate::RepositoryError;

/// Position of the last party of a page. Parties are ordered by name ignoring case, then by id
/// (both reversed for [`PartySort::NameDesc`]), so the name and id of the last item are enough to
/// resume right after it in either direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyCursor {
    pub name: String,
    pub id: PartyId,
}

impl From<&Party> for PartyCursor {
    fn from(party: &Party) -> Self {
        Self {
            name: party.name().to_owned(),
            id: party.id(),
        }
    }
}

/// Which side of a party a list is narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyKind {
    Customer,
    Supplier,
}

/// Every sort is by a column that is never empty, so a cursor can always be built from the last row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PartySort {
    #[default]
    NameAsc,
    NameDesc,
}

/// How a list of parties is narrowed and ordered, the convention other lists follow: a free-text
/// search, filters on single attributes, and one sort. Built through the methods below so that
/// adapters can trust it is normalized.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartyQuery {
    search: Option<String>,
    /// The search written the way documents are stored (no punctuation, upper case), kept apart
    /// because it can be empty when the search is not: a search for "." must not match every
    /// document.
    document_needle: Option<String>,
    kind: Option<PartyKind>,
    sort: PartySort,
}

impl PartyQuery {
    /// Longest search text accepted; a person types a name or a number, not a paragraph.
    pub const MAX_SEARCH_CHARS: usize = 100;

    /// Matches a name or an e-mail anywhere in it, ignoring case, and a document ignoring its
    /// punctuation. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        let text = text.trim();
        self.search = (!text.is_empty()).then(|| text.to_owned());
        let needle: String = text
            .chars()
            .filter(|c| !matches!(c, '.' | '-' | '/') && !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        self.document_needle = (!needle.is_empty()).then_some(needle);
        self
    }

    pub fn of_kind(mut self, kind: PartyKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn sorted(mut self, sort: PartySort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn kind(&self) -> Option<PartyKind> {
        self.kind
    }

    pub fn sort(&self) -> PartySort {
        self.sort
    }

    /// What to look for in documents, or `None` when the search has nothing a document could hold.
    pub fn document_search(&self) -> Option<&str> {
        self.document_needle.as_deref()
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, party: &Party) -> bool {
        let kind_matches = match self.kind {
            None => true,
            Some(PartyKind::Customer) => party.is_customer(),
            Some(PartyKind::Supplier) => party.is_supplier(),
        };
        kind_matches
            && self
                .search()
                .is_none_or(|text| self.search_matches(text, party))
    }

    fn search_matches(&self, text: &str, party: &Party) -> bool {
        let text = text.to_lowercase();
        party.name().to_lowercase().contains(&text)
            || party
                .email()
                .is_some_and(|email| email.as_str().to_lowercase().contains(&text))
            || self.document_search().is_some_and(|needle| {
                party
                    .document()
                    .is_some_and(|document| document.as_str().contains(needle))
            })
    }
}

/// Adapters call this before a write: an entry that is not about the party being written (another
/// company, another id) would put a row into the wrong trail, which is a caller's bug, so it is
/// refused instead of stored.
pub fn ensure_audit_describes(audit: &AuditEntry, party: &Party) -> Result<(), RepositoryError> {
    if audit.describes(party) {
        Ok(())
    } else {
        Err(RepositoryError::Storage(
            "audit entry does not describe the party being written".to_owned(),
        ))
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write.
pub trait PartyRepository: Send + Sync {
    fn save(
        &self,
        party: &Party,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The party, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> impl Future<Output = Result<Option<Party>, RepositoryError>> + Send;

    /// Replaces what is stored about the party's document check, and nothing else.
    fn update_document_check(
        &self,
        party: &Party,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the party, oldest first. Empty for a party of another company.
    /// Reading the trail is not itself recorded.
    fn history(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` parties of the company that match `query`, in its order, starting right
    /// after `after` when given. A cursor only means something for the query that produced it.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PartyQuery,
        after: Option<&PartyCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Party>, RepositoryError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_search_is_no_search() {
        assert_eq!(PartyQuery::default().searching("  \t ").search(), None);
        assert_eq!(
            PartyQuery::default().searching("  Ana ").search(),
            Some("Ana")
        );
    }

    #[test]
    fn document_search_drops_punctuation_and_never_matches_everything() {
        let document_search = |text: &str| {
            PartyQuery::default()
                .searching(text)
                .document_search()
                .map(str::to_owned)
        };
        assert_eq!(
            document_search("11.222.333/0001-81").as_deref(),
            Some("11222333000181")
        );
        assert_eq!(document_search("12.abc").as_deref(), Some("12ABC"));
        assert_eq!(document_search(" . - / "), None);
        assert_eq!(PartyQuery::default().document_search(), None);
    }
}
