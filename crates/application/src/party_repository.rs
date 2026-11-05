use std::future::Future;

use conta_verde_domain::{CompanyId, Party, PartyId};

use crate::RepositoryError;

/// Position of the last party of a page. Parties are ordered by name ignoring case, then by id,
/// so the name and id of the last item are enough to resume right after it.
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

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
pub trait PartyRepository: Send + Sync {
    fn save(&self, party: &Party) -> impl Future<Output = Result<(), RepositoryError>> + Send;

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
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// At most `limit` parties of the company, ordered by name ignoring case, then by id,
    /// starting right after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        after: Option<&PartyCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Party>, RepositoryError>> + Send;
}
