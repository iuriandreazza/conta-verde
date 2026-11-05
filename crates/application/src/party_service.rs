use conta_verde_domain::{Cnpj, CompanyId, Document, DocumentCheck, Party, PartyDraft, PartyId};

use crate::{
    ApplicationError, CnpjLookupError, CnpjRegistry, PartyCursor, PartyRepository, RegistryEntry,
};

const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 200;

#[derive(Debug, Clone)]
pub struct PartyPage {
    pub items: Vec<Party>,
    /// Present when more parties exist after this page.
    pub next: Option<PartyCursor>,
}

pub struct PartyService<R, C> {
    repository: R,
    registry: C,
}

impl<R: PartyRepository, C: CnpjRegistry> PartyService<R, C> {
    pub fn new(repository: R, registry: C) -> Self {
        Self {
            repository,
            registry,
        }
    }

    /// Validates the draft, checks a CNPJ against the federal registry and stores the party.
    ///
    /// A CPF is validated by its check digits only. What the registry says about a CNPJ is
    /// recorded and never blocks: unknown to the registry (its open data lags, so a new company
    /// can be missing) is stored as not found, and an unreachable registry as unverified, so a
    /// third-party limitation never stops data entry. With the lookup switched off nothing is
    /// asked and nothing is claimed about the CNPJ.
    pub async fn register(
        &self,
        company_id: CompanyId,
        draft: PartyDraft,
    ) -> Result<Party, ApplicationError> {
        let mut party = Party::new(PartyId::generate(), company_id, draft)?;
        if let Some(cnpj) = party.document().and_then(Document::cnpj) {
            let check = self.check_cnpj(cnpj).await;
            party = party.with_document_check(check);
        }
        self.repository.save(&party).await?;
        Ok(party)
    }

    async fn check_cnpj(&self, cnpj: Cnpj<'_>) -> DocumentCheck {
        match self.registry.lookup(cnpj).await {
            Ok(RegistryEntry {
                legal_name,
                situation,
            }) => DocumentCheck::Verified {
                legal_name,
                situation,
            },
            Err(CnpjLookupError::NotFound) => DocumentCheck::NotFound,
            Err(CnpjLookupError::Unavailable) => DocumentCheck::Unverified,
            Err(CnpjLookupError::Disabled) => DocumentCheck::NotApplicable,
        }
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
        let next = if has_more {
            items.last().map(PartyCursor::from)
        } else {
            None
        };
        Ok(PartyPage { items, next })
    }
}
