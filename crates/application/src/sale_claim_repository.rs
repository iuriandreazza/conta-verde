use std::future::Future;

use conta_verde_domain::{CompanyId, SaleId};

use crate::RepositoryError;

/// What was first said about an external reference: the id its sale has (or will have) and the request that asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleClaim {
    pub sale_id: SaleId,
    pub request: String,
}

/// Port to the references a program names its orders with ([ADR 0087]): the **claim** on a reference is made before the sale
/// is, by a unique key, so a retry after a crash and a race between two requests both end in one sale.
///
/// A claim is operational, not a register of the business: it is never edited or deleted and has no audit entry (the sale it
/// names has its own).
pub trait SaleClaimRepository: Send + Sync {
    /// Claims `reference` for `sale_id` in the company, written as `request`, unless it was claimed already, and gives back
    /// the claim that holds: the new one for the first caller, the first one for every later caller.
    fn claim(
        &self,
        company_id: CompanyId,
        reference: &str,
        sale_id: SaleId,
        request: &str,
    ) -> impl Future<Output = Result<SaleClaim, RepositoryError>> + Send;

    /// The reference a sale was claimed with, if it was made through one.
    fn reference_of(
        &self,
        company_id: CompanyId,
        sale_id: SaleId,
    ) -> impl Future<Output = Result<Option<String>, RepositoryError>> + Send;

    /// The claim on a reference, if there is one.
    fn find(
        &self,
        company_id: CompanyId,
        reference: &str,
    ) -> impl Future<Output = Result<Option<SaleClaim>, RepositoryError>> + Send;
}
