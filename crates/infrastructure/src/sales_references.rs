use conta_verde_application::{
    PartyRepository, PaymentTermRepository, ProductRepository, RepositoryError, SalesReferences,
};
use conta_verde_domain::{CompanyId, PartyId, PaymentTermId, ProductId};

/// Answers what a quote, a sale or a contract may point at by asking the registers themselves, so one
/// implementation serves every adapter, like `RegisterReferences` does for entries. A record of
/// another company is not found, so it answers like one that does not exist.
pub struct RegisterSalesReferences<P, R, T> {
    parties: P,
    products: R,
    payment_terms: T,
}

impl<P, R, T> RegisterSalesReferences<P, R, T> {
    pub fn new(parties: P, products: R, payment_terms: T) -> Self {
        Self {
            parties,
            products,
            payment_terms,
        }
    }
}

impl<P, R, T> SalesReferences for RegisterSalesReferences<P, R, T>
where
    P: PartyRepository,
    R: ProductRepository,
    T: PaymentTermRepository,
{
    async fn customer_exists(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<bool, RepositoryError> {
        Ok(self.parties.find(company_id, id).await?.is_some())
    }

    async fn product_is_usable(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .products
            .find(company_id, id)
            .await?
            .is_some_and(|product| product.is_active()))
    }

    async fn payment_term_is_usable(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .payment_terms
            .find(company_id, id)
            .await?
            .is_some_and(|term| term.is_active()))
    }
}
