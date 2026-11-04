use conta_verde_domain::{CompanyId, Party, PartyDraft, PartyId};

use crate::{ApplicationError, PartyCursor, PartyRepository};

const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 200;

#[derive(Debug, Clone)]
pub struct PartyPage {
    pub items: Vec<Party>,
    /// Present when more parties exist after this page.
    pub next: Option<PartyCursor>,
}

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

    pub async fn list(
        &self,
        company_id: CompanyId,
        after: Option<&PartyCursor>,
        limit: Option<usize>,
    ) -> Result<PartyPage, ApplicationError> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
        // One extra row tells whether another page exists without a second query.
        let mut items = self
            .repository
            .list_by_company(company_id, after, limit + 1)
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next = has_more
            .then(|| items.last().map(PartyCursor::from))
            .flatten();
        Ok(PartyPage { items, next })
    }
}
