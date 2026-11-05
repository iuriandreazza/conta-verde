use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, Document, DocumentCheck, DomainError, Party,
    PartyDraft, PartyId, creation_changes, document_check_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, CnpjLookupError, CnpjRegistry, NameCursor, PartyQuery, PartyRepository,
    RegistryEntry, RepositoryError,
};

const DEFAULT_RECHECK_INTERVAL: Duration = Duration::from_secs(10);

fn check_from(answer: Result<RegistryEntry, CnpjLookupError>) -> DocumentCheck {
    match answer {
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

#[derive(Debug, Clone)]
pub struct PartyPage {
    pub items: Vec<Party>,
    /// Present when more parties exist after this page.
    pub next: Option<NameCursor>,
}

pub struct PartyService<R, C> {
    repository: R,
    registry: C,
    recheck_interval: Duration,
    last_rechecks: Mutex<HashMap<String, Instant>>,
}

impl<R: PartyRepository, C: CnpjRegistry> PartyService<R, C> {
    pub fn new(repository: R, registry: C) -> Self {
        Self {
            repository,
            registry,
            recheck_interval: DEFAULT_RECHECK_INTERVAL,
            last_rechecks: Mutex::default(),
        }
    }

    /// How long must pass between two re-checks of the same CNPJ.
    pub fn with_recheck_interval(mut self, interval: Duration) -> Self {
        self.recheck_interval = interval;
        self
    }

    /// Validates the draft, checks a CNPJ against the federal registry and stores the party.
    ///
    /// A CPF is validated by its check digits only. What the registry says about a CNPJ is
    /// recorded and never blocks: unknown to the registry (its open data lags, so a new company
    /// can be missing) is stored as not found, and an unreachable registry as unverified, so a
    /// third-party limitation never stops data entry. With the lookup switched off nothing is
    /// asked and nothing is claimed about the CNPJ.
    ///
    /// The creation is recorded in the audit trail under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: PartyDraft,
    ) -> Result<Party, ApplicationError> {
        let mut party = Party::new(PartyId::generate(), company_id, draft)?;
        if let Some(cnpj) = party.document().and_then(Document::cnpj) {
            let check = check_from(self.registry.lookup(cnpj).await);
            party = party.with_document_check(check);
        }
        let audit = audit_entry(
            &party,
            actor,
            AuditAction::Created,
            creation_changes(&party),
        );
        self.repository.save(&party, &audit).await?;
        Ok(party)
    }

    /// Asks the registry again about the party's CNPJ and records the answer, so a party stored
    /// as unverified or not found (an outage, a lagging registry) can be re-checked later. The
    /// answer replaces the old one, whatever it was; like at registration, nothing blocks.
    ///
    /// A re-check that gets the same answer as before changes nothing, so nothing is written and
    /// no audit entry is added: the trail lists changes, not attempts.
    pub async fn verify_document(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        party_id: PartyId,
    ) -> Result<Party, ApplicationError> {
        let party = self
            .repository
            .find(company_id, party_id)
            .await?
            .ok_or(ApplicationError::PartyNotFound)?;
        let cnpj = party
            .document()
            .and_then(Document::cnpj)
            .ok_or(DomainError::NoCnpjToVerify)?;
        let cnpj_key = cnpj.as_str().to_owned();
        self.claim_recheck(&cnpj_key)?;
        let answer = self.registry.refresh(cnpj).await;
        if answer == Err(CnpjLookupError::Disabled) {
            // Nothing was asked, so nothing should hold the next attempt back.
            self.release_recheck(&cnpj_key);
            return Err(ApplicationError::LookupDisabled);
        }
        let new_check = check_from(answer);
        let changes = document_check_changes(party.document_check(), &new_check);
        let party = party.with_document_check(new_check);
        if changes.is_empty() {
            return Ok(party);
        }
        let audit = audit_entry(&party, actor, AuditAction::Updated, changes);
        self.repository
            .update_document_check(&party, &audit)
            .await
            .map_err(|error| {
                // Nothing was stored, so the user may try again right away.
                self.release_recheck(&cnpj_key);
                match error {
                    // The party vanished or changed company between the read and the write.
                    RepositoryError::NotFound => ApplicationError::PartyNotFound,
                    other => other.into(),
                }
            })?;
        Ok(party)
    }

    /// What was recorded about the party, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        party_id: PartyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // A party of another company answers like an unknown one, never with its trail.
        self.repository
            .find(company_id, party_id)
            .await?
            .ok_or(ApplicationError::PartyNotFound)?;
        Ok(self.repository.history(company_id, party_id).await?)
    }

    /// Lets one re-check per CNPJ through each interval, whichever party or company asks: the
    /// registry's rate limit is shared by the whole installation, and parties that share a CNPJ
    /// would otherwise each cost a call. An answer that says the registry is unreachable counts
    /// too, since the registry was asked.
    fn claim_recheck(&self, cnpj: &str) -> Result<(), ApplicationError> {
        let mut last = self.last_rechecks.lock().expect("recheck lock poisoned");
        let interval = self.recheck_interval;
        last.retain(|_, at| at.elapsed() < interval);
        if last.contains_key(cnpj) {
            return Err(ApplicationError::RecheckTooSoon);
        }
        last.insert(cnpj.to_owned(), Instant::now());
        Ok(())
    }

    fn release_recheck(&self, cnpj: &str) {
        self.last_rechecks
            .lock()
            .expect("recheck lock poisoned")
            .remove(cnpj);
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &PartyQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<PartyPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(PartyPage { items, next })
    }
}
