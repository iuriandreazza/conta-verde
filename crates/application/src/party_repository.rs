use std::future::Future;

use conta_verde_domain::{CompanyId, Party};

use crate::RepositoryError;

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
pub trait PartyRepository: Send + Sync {
    fn save(&self, party: &Party) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn list_by_company(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Vec<Party>, RepositoryError>> + Send;
}
