use conta_verde_application::{
    BankAccountRepository, CategoryRepository, CostCenterRepository, EntryReferences,
    PartyRepository, PaymentMethodRepository, RepositoryError,
};
use conta_verde_domain::{
    BankAccountId, CategoryId, CompanyId, CostCenterId, DreGroup, PartyId, PaymentMethodId,
};

/// Answers what an entry and its settlements may point at by asking the registers themselves, so one
/// implementation serves every adapter: Postgres, and the in-memory ones that tests share between the
/// services. A record of another company is not found, so it answers like one that does not exist.
pub struct RegisterReferences<C, K, P, B, M> {
    categories: C,
    cost_centers: K,
    parties: P,
    bank_accounts: B,
    payment_methods: M,
}

impl<C, K, P, B, M> RegisterReferences<C, K, P, B, M> {
    pub fn new(
        categories: C,
        cost_centers: K,
        parties: P,
        bank_accounts: B,
        payment_methods: M,
    ) -> Self {
        Self {
            categories,
            cost_centers,
            parties,
            bank_accounts,
            payment_methods,
        }
    }
}

impl<C, K, P, B, M> EntryReferences for RegisterReferences<C, K, P, B, M>
where
    C: CategoryRepository,
    K: CostCenterRepository,
    P: PartyRepository,
    B: BankAccountRepository,
    M: PaymentMethodRepository,
{
    async fn category_is_usable(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .categories
            .find(company_id, id)
            .await?
            .is_some_and(|category| {
                category.is_active() && category.dre_group() != DreGroup::Transfers
            }))
    }

    async fn cost_center_is_usable(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .cost_centers
            .find(company_id, id)
            .await?
            .is_some_and(|cost_center| cost_center.is_active()))
    }

    async fn party_exists(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<bool, RepositoryError> {
        Ok(self.parties.find(company_id, id).await?.is_some())
    }

    async fn bank_account_is_usable(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .bank_accounts
            .find(company_id, id)
            .await?
            .is_some_and(|account| account.is_active()))
    }

    async fn payment_method_is_usable(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .payment_methods
            .find(company_id, id)
            .await?
            .is_some_and(|method| method.is_active()))
    }
}
