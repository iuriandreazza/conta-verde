use conta_verde_domain::{CompanyId, Party, PartyDraft, PartyId};

use crate::{ApplicationError, PartyRepository};

pub struct PartyService<R> {
    repository: R,
}

impl<R: PartyRepository> PartyService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn register(
        &self,
        company_id: CompanyId,
        draft: PartyDraft,
    ) -> Result<Party, ApplicationError> {
        let party = Party::new(PartyId::generate(), company_id, draft)?;
        self.repository.save(&party).await?;
        Ok(party)
    }

    pub async fn list(&self, company_id: CompanyId) -> Result<Vec<Party>, ApplicationError> {
        Ok(self.repository.list_by_company(company_id).await?)
    }
}
