use conta_verde_application::{
    DocumentReferences, NameCursor, PartyQuery, PartyRepository, PaymentTermRepository,
    ProductQuery, ProductRepository, RepositoryError,
};
use conta_verde_domain::{CompanyId, Document, PartyId, PaymentTerm, PaymentTermId, ProductId};

/// How many registers a lookup reads at a time.
const PAGE: usize = 200;

/// Answers what a quote, a sale, a contract or a purchase order may point at by asking the registers themselves, so one
/// implementation serves every adapter, like `RegisterReferences` does for entries. A record of
/// another company is not found, so it answers like one that does not exist.
pub struct RegisterDocumentReferences<P, R, T> {
    parties: P,
    products: R,
    payment_terms: T,
}

impl<P, R, T> RegisterDocumentReferences<P, R, T> {
    pub fn new(parties: P, products: R, payment_terms: T) -> Self {
        Self {
            parties,
            products,
            payment_terms,
        }
    }
}

impl<P, R, T> DocumentReferences for RegisterDocumentReferences<P, R, T>
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

    async fn supplier_is_usable(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .parties
            .find(company_id, id)
            .await?
            .is_some_and(|party| party.is_supplier()))
    }

    async fn party_with_document(
        &self,
        company_id: CompanyId,
        document: &Document,
    ) -> Result<Option<PartyId>, RepositoryError> {
        let query = PartyQuery::default().searching(document.as_str());
        let mut after: Option<NameCursor> = None;
        loop {
            let page = self
                .parties
                .list_by_company(company_id, &query, after.as_ref(), PAGE + 1)
                .await?;
            if let Some(party) = page
                .iter()
                .take(PAGE)
                .find(|party| party.document() == Some(document))
            {
                return Ok(Some(party.id()));
            }
            match (page.len() > PAGE).then(|| page.get(PAGE - 1)).flatten() {
                Some(last) => after = Some(NameCursor::from(last)),
                None => return Ok(None),
            }
        }
    }

    async fn product_with_code(
        &self,
        company_id: CompanyId,
        code: &str,
    ) -> Result<Option<ProductId>, RepositoryError> {
        let query = ProductQuery::default().searching(code);
        let wanted = code.trim();
        let mut after: Option<NameCursor> = None;
        loop {
            let page = self
                .products
                .list_by_company(company_id, &query, after.as_ref(), PAGE + 1)
                .await?;
            if let Some(product) = page.iter().take(PAGE).find(|product| {
                product
                    .sku()
                    .is_some_and(|sku| sku.eq_ignore_ascii_case(wanted))
            }) {
                return Ok(Some(product.id()));
            }
            match (page.len() > PAGE).then(|| page.get(PAGE - 1)).flatten() {
                Some(last) => after = Some(NameCursor::from(last)),
                None => return Ok(None),
            }
        }
    }

    async fn seller_is_usable(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .parties
            .find(company_id, id)
            .await?
            .is_some_and(|party| party.is_seller()))
    }

    async fn payment_term(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<Option<PaymentTerm>, RepositoryError> {
        self.payment_terms.find(company_id, id).await
    }
}
