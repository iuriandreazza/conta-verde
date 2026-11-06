use std::future::Future;

use conta_verde_domain::{ChargeId, CompanyId};

use crate::RepositoryError;

/// What a customer's link leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkTarget {
    pub company_id: CompanyId,
    pub charge_id: ChargeId,
}

/// Port for the links that lead a customer to the payment page of a charge ([ADR 0047]). Only the digest of
/// the token is kept, like an invitation's: a leaked database gives no usable link. A link is a
/// capability and not a register, so it is not audited. A charge may have several, since each reminder
/// makes its own.
pub trait PaymentLinkRepository: Send + Sync {
    /// Stores a link to the company's charge, under the digest of its token. The database refuses a charge
    /// of another company.
    fn save(
        &self,
        company_id: CompanyId,
        charge_id: ChargeId,
        digest: &str,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The charge behind a digest, if any link was made under it.
    fn find(
        &self,
        digest: &str,
    ) -> impl Future<Output = Result<Option<LinkTarget>, RepositoryError>> + Send;
}
