use std::future::Future;

use conta_verde_domain::{CompanyId, Document, PartyId, PaymentTerm, PaymentTermId, ProductId};

use crate::RepositoryError;

/// Whether what a quote, a sale, a contract or a purchase order points at can be used, asked before it is written
/// ([ADR 0037]). A reference to another company's record answers like one that does not exist.
pub trait DocumentReferences: Send + Sync {
    /// The customer exists in the company.
    fn customer_exists(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The product or service exists in the company and is active.
    fn product_is_usable(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The payment term exists in the company and is active.
    fn payment_term_is_usable(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The party exists in the company and is a supplier.
    fn supplier_is_usable(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The party of the company with this registry number, if one is registered ([ADR 0057]).
    fn party_with_document(
        &self,
        company_id: CompanyId,
        document: &Document,
    ) -> impl Future<Output = Result<Option<PartyId>, RepositoryError>> + Send;

    /// The product of the company whose SKU is this code (compared without regard to case), if one is.
    fn product_with_code(
        &self,
        company_id: CompanyId,
        code: &str,
    ) -> impl Future<Output = Result<Option<ProductId>, RepositoryError>> + Send;

    /// The party exists in the company and is a seller.
    fn seller_is_usable(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The payment term, active or not: a sale that was agreed on one is billed by it.
    fn payment_term(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> impl Future<Output = Result<Option<PaymentTerm>, RepositoryError>> + Send;
}
