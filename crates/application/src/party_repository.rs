use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, Party, PartyId};

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

    /// At most `limit` parties of the company, ordered by name ignoring case, then by id,
    /// starting right after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        after: Option<&PartyCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Party>, RepositoryError>> + Send;
}
