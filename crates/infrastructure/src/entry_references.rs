use conta_verde_application::{
    CategoryRepository, CostCenterRepository, EntryReferences, PartyRepository, RepositoryError,
};
use conta_verde_domain::{CategoryId, CompanyId, CostCenterId, DreGroup, PartyId};

/// Answers what an entry may point at by asking the registers themselves, so one implementation
/// serves every adapter: Postgres, and the in-memory ones that tests share between the services.
/// A record of another company is not found, so it answers like one that does not exist.
pub struct RegisterReferences<C, K, P> {
    categories: C,
    cost_centers: K,
    parties: P,
}

impl<C, K, P> RegisterReferences<C, K, P> {
    pub fn new(categories: C, cost_centers: K, parties: P) -> Self {
        Self {
            categories,
            cost_centers,
            parties,
        }
    }
}

impl<C, K, P> EntryReferences for RegisterReferences<C, K, P>
where
    C: CategoryRepository,
    K: CostCenterRepository,
    P: PartyRepository,
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
}
