use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, Document, DocumentCheck, DomainError, Party,
    PartyDraft, PartyId, creation_changes, document_check_changes, party_changes,
};

use crate::audit::audit_entry;
use crate::import::import_rows;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, CnpjLookupError, CnpjRegistry, ImportReport, ImportRow, NameCursor,
    PartyQuery, PartyRepository, RegistryEntry, RepositoryError,
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

    /// Registers the parties of an import file, or with `dry_run` only says what would happen.
    ///
    /// Every row is checked first, as when registering one, and a document that repeats within the
    /// file is refused; if any row is refused nothing is written (the report says which and why).
    /// Then the rows are written one by one, each with its own audit entry under `actor`: a
    /// document that is already registered is skipped, never overwritten.
    ///
    /// The registry is **not** asked about each CNPJ: a file of hundreds would spend its shared rate
    /// limit and leave most parties unverified anyway. A CNPJ is stored as unverified, so the list
    /// offers "check again" for it, unless the operator switched the lookup off, which claims
    /// nothing about it, as for any registration.
    pub async fn import(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        rows: Vec<ImportRow<PartyDraft>>,
        dry_run: bool,
    ) -> Result<ImportReport, ApplicationError> {
        let unchecked = if self.registry.is_enabled() {
            DocumentCheck::Unverified
        } else {
            DocumentCheck::NotApplicable
        };
        let unchecked = &unchecked;
        import_rows(
            rows,
            dry_run,
            "document",
            RepositoryError::DuplicateDocument,
            |draft| Party::new(PartyId::generate(), company_id, draft),
            |party| {
                party
                    .document()
                    .map(|document| document.as_str().to_owned())
            },
            |party| async move {
                let party = if party.document().and_then(Document::cnpj).is_some() {
                    party.with_document_check(unchecked.clone())
                } else {
                    party
                };
                let audit = audit_entry(
                    &party,
                    actor,
                    AuditAction::Created,
                    creation_changes(&party),
                );
                self.repository.save(&party, &audit).await
            },
        )
        .await
    }

    /// Replaces every editable field of the party with the draft's. A document that changed is
    /// checked against the registry like a new one (and the old answer goes with the old
    /// document); the same document keeps what the registry said, since nothing is asked again.
    ///
    /// Only the fields that differ are recorded, and a change that changes nothing writes nothing,
    /// so no audit entry is added for it.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        party_id: PartyId,
        draft: PartyDraft,
    ) -> Result<Party, ApplicationError> {
        let before = self
            .repository
            .find(company_id, party_id)
            .await?
            .ok_or(ApplicationError::PartyNotFound)?;
        let mut party = before.replaced_with(draft)?;
        if party.document() != before.document()
            && let Some(cnpj) = party.document().and_then(Document::cnpj)
        {
            let check = check_from(self.registry.lookup(cnpj).await);
            party = party.with_document_check(check);
        }
        let changes = party_changes(&before, &party);
        if changes.is_empty() {
            return Ok(party);
        }
        let audit = audit_entry(&party, actor, AuditAction::Updated, changes);
        self.repository
            .update(&party, &audit)
            .await
            .map_err(|error| match error {
                // The party vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::PartyNotFound,
                other => other.into(),
            })?;
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
        // The registry can take seconds, and the write replaces the whole party: read it again, so
        // an edit made meanwhile is not undone, and drop an answer about a document that is no
        // longer the party's.
        let party = self
            .repository
            .find(company_id, party_id)
            .await?
            .ok_or(ApplicationError::PartyNotFound)?;
        if party
            .document()
            .and_then(Document::cnpj)
            .map(|c| c.as_str())
            != Some(cnpj_key.as_str())
        {
            self.release_recheck(&cnpj_key);
            return Ok(party);
        }
        let new_check = check_from(answer);
        let changes = document_check_changes(party.document_check(), &new_check);
        let party = party.with_document_check(new_check);
        if changes.is_empty() {
            return Ok(party);
        }
        let audit = audit_entry(&party, actor, AuditAction::Updated, changes);
        self.repository
            .update(&party, &audit)
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
