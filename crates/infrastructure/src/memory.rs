use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use conta_verde_application::{
    CnpjLookupError, CnpjRegistry, IdentityRepository, NewAccount, PartyCursor, PartyQuery,
    PartyRepository, PartySort, RegistryEntry, RepositoryError, StoredCredentials,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, Cnpj, Company, CompanyId, Email, Party, PartyId, User,
    UserId,
};
use uuid::Uuid;

/// Keeps parties and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened: a refused or missed write leaves nothing in the trail.
#[derive(Default)]
pub struct InMemoryPartyRepository {
    state: Mutex<PartyState>,
}

#[derive(Default)]
struct PartyState {
    parties: Vec<Party>,
    audit: Vec<AuditRecord>,
    next_sequence: i64,
}

impl PartyState {
    fn record(&mut self, audit: &AuditEntry) {
        self.next_sequence += 1;
        let at_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
            });
        self.audit.push(AuditRecord {
            sequence: self.next_sequence,
            at_millis,
            entry: audit.clone(),
        });
    }
}

impl PartyRepository for InMemoryPartyRepository {
    async fn save(&self, party: &Party, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, party)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let duplicate = party.document().is_some()
            && state.parties.iter().any(|existing| {
                existing.company_id() == party.company_id()
                    && existing.document() == party.document()
            });
        if duplicate {
            return Err(RepositoryError::DuplicateDocument);
        }
        state.parties.push(party.clone());
        state.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<Option<Party>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .parties
            .iter()
            .find(|party| party.company_id() == company_id && party.id() == id)
            .cloned())
    }

    async fn update_document_check(
        &self,
        party: &Party,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, party)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .parties
            .iter_mut()
            .find(|stored| stored.company_id() == party.company_id() && stored.id() == party.id());
        let stored = stored.ok_or(RepositoryError::NotFound)?;
        *stored = stored
            .clone()
            .with_document_check(party.document_check().clone());
        state.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .iter()
            .filter(|record| {
                record.entry.company_id == company_id
                    && record.entry.entity == AuditEntity::Party
                    && record.entry.entity_id == id.as_uuid()
            })
            .cloned()
            .collect())
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PartyQuery,
        after: Option<&PartyCursor>,
        limit: usize,
    ) -> Result<Vec<Party>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let sort_key = |name: &str, id: Uuid| (name.to_lowercase(), id);
        let descending = query.sort() == PartySort::NameDesc;
        let after_key = after.map(|cursor| sort_key(&cursor.name, cursor.id.as_uuid()));

        let mut found: Vec<Party> = state
            .parties
            .iter()
            .filter(|party| party.company_id() == company_id && query.matches(party))
            .filter(|party| {
                after_key.as_ref().is_none_or(|key| {
                    let key_of_party = sort_key(party.name(), party.id().as_uuid());
                    if descending {
                        key_of_party < *key
                    } else {
                        key_of_party > *key
                    }
                })
            })
            .cloned()
            .collect();
        found.sort_by_cached_key(|party| sort_key(party.name(), party.id().as_uuid()));
        if descending {
            found.reverse();
        }
        found.truncate(limit);
        Ok(found)
    }
}

/// A test double: a registry that knows only the companies it was given, or none at all.
/// Clones share their state, so a test can keep a handle and change what the registry answers
/// after the service under test has taken ownership of another clone.
#[derive(Clone, Default)]
pub struct InMemoryCnpjRegistry {
    state: Arc<Mutex<RegistryState>>,
}

#[derive(Default)]
struct RegistryState {
    companies: HashMap<String, RegistryEntry>,
    unreachable: bool,
}

impl InMemoryCnpjRegistry {
    pub fn with_company(self, cnpj: &str, legal_name: &str, situation: &str) -> Self {
        self.add_company(cnpj, legal_name, situation);
        self
    }

    pub fn add_company(&self, cnpj: &str, legal_name: &str, situation: &str) {
        self.state
            .lock()
            .expect("registry lock poisoned")
            .companies
            .insert(
                cnpj.to_owned(),
                RegistryEntry {
                    legal_name: legal_name.to_owned(),
                    situation: situation.to_owned(),
                },
            );
    }

    /// Every lookup fails as if the service were down, until `recover` is called.
    pub fn unreachable() -> Self {
        let registry = Self::default();
        registry
            .state
            .lock()
            .expect("registry lock poisoned")
            .unreachable = true;
        registry
    }

    pub fn recover(&self) {
        self.state
            .lock()
            .expect("registry lock poisoned")
            .unreachable = false;
    }
}

impl CnpjRegistry for InMemoryCnpjRegistry {
    async fn lookup(&self, cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
        let state = self.state.lock().expect("registry lock poisoned");
        if state.unreachable {
            return Err(CnpjLookupError::Unavailable);
        }
        state
            .companies
            .get(cnpj.as_str())
            .cloned()
            .ok_or(CnpjLookupError::NotFound)
    }
}

/// Keeps users, companies, memberships and sessions in process memory.
#[derive(Default)]
pub struct InMemoryIdentityRepository {
    state: Mutex<IdentityState>,
}

#[derive(Default)]
struct IdentityState {
    users: Vec<StoredCredentials>,
    companies: Vec<Company>,
    memberships: Vec<(UserId, CompanyId)>,
    sessions: HashMap<String, (UserId, Instant)>,
}

impl IdentityState {
    fn user(&self, id: UserId) -> Option<&User> {
        self.users
            .iter()
            .map(|credentials| &credentials.user)
            .find(|user| user.id() == id)
    }
}

impl IdentityRepository for InMemoryIdentityRepository {
    async fn register_account(&self, new_account: &NewAccount) -> Result<(), RepositoryError> {
        let NewAccount {
            account,
            password_hash,
        } = new_account;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state
            .users
            .iter()
            .any(|existing| existing.user.email() == account.user.email())
        {
            return Err(RepositoryError::DuplicateEmail);
        }
        state.users.push(StoredCredentials {
            user: account.user.clone(),
            password_hash: password_hash.clone(),
        });
        state.companies.push(account.company.clone());
        state
            .memberships
            .push((account.user.id(), account.company.id()));
        Ok(())
    }

    async fn find_credentials(
        &self,
        email: &Email,
    ) -> Result<Option<StoredCredentials>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .users
            .iter()
            .find(|credentials| credentials.user.email() == email)
            .cloned())
    }

    async fn companies_of(&self, user_id: UserId) -> Result<Vec<Company>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut companies: Vec<Company> = state
            .companies
            .iter()
            .filter(|company| state.memberships.contains(&(user_id, company.id())))
            .cloned()
            .collect();
        companies
            .sort_by_cached_key(|company| (company.name().to_lowercase(), company.id().as_uuid()));
        Ok(companies)
    }

    async fn is_member(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> Result<bool, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state.memberships.contains(&(user_id, company_id)))
    }

    async fn create_session(
        &self,
        user_id: UserId,
        token_digest: &str,
        lifetime: Duration,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.sessions.insert(
            token_digest.to_owned(),
            (user_id, Instant::now() + lifetime),
        );
        Ok(())
    }

    async fn find_session_user(&self, token_digest: &str) -> Result<Option<User>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .sessions
            .get(token_digest)
            .filter(|(_, expires_at)| *expires_at > Instant::now())
            .and_then(|(user_id, _)| state.user(*user_id))
            .cloned())
    }

    async fn delete_session(&self, token_digest: &str) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.sessions.remove(token_digest);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{ApplicationError, AuthService, PartyService};
    use conta_verde_domain::{Actor, DocumentCheck, DomainError, PartyDraft, UserId};

    use super::*;
    use crate::{Argon2SecretService, contract};

    fn draft(name: &str, document: Option<&str>) -> PartyDraft {
        PartyDraft {
            name: name.into(),
            document: document.map(Into::into),
            email: None,
            is_customer: true,
            is_supplier: false,
        }
    }

    #[tokio::test]
    async fn registers_and_lists_only_the_companys_parties() {
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::default(),
        );
        let company = CompanyId::generate();
        let other_company = CompanyId::generate();

        service
            .register(company, &contract::actor(), draft("Zeca", None))
            .await
            .unwrap();
        service
            .register(company, &contract::actor(), draft("ana", None))
            .await
            .unwrap();
        service
            .register(other_company, &contract::actor(), draft("Outra", None))
            .await
            .unwrap();

        let names: Vec<String> = service
            .list(company, &PartyQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .iter()
            .map(|party| party.name().to_owned())
            .collect();
        assert_eq!(names, ["ana", "Zeca"]);
    }

    #[tokio::test]
    async fn rejects_duplicate_document_within_a_company_only() {
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::default(),
        );
        let company = CompanyId::generate();
        let document = Some("529.982.247-25");

        service
            .register(company, &contract::actor(), draft("Ana", document))
            .await
            .unwrap();
        let duplicate = service
            .register(company, &contract::actor(), draft("Ana 2", document))
            .await;
        assert_eq!(
            duplicate.unwrap_err(),
            ApplicationError::Repository(RepositoryError::DuplicateDocument)
        );

        let other = service
            .register(
                CompanyId::generate(),
                &contract::actor(),
                draft("Ana 3", document),
            )
            .await;
        assert!(other.is_ok());
    }

    #[tokio::test]
    async fn pages_through_parties_in_name_order() {
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::default(),
        );
        contract::pages_through_parties(&service, CompanyId::generate()).await;
    }

    #[tokio::test]
    async fn breaks_ties_between_equal_names() {
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::default(),
        );
        contract::breaks_ties_between_equal_names(&service, CompanyId::generate()).await;
    }

    #[tokio::test]
    async fn filters_and_sorts_parties() {
        let service = PartyService::new(InMemoryPartyRepository::default(), contract::registry());
        contract::filters_and_sorts_parties(&service, CompanyId::generate(), CompanyId::generate())
            .await;
    }

    #[tokio::test]
    async fn keeps_what_the_registry_said() {
        let service = PartyService::new(InMemoryPartyRepository::default(), contract::registry());
        contract::keeps_what_the_registry_said(&service, CompanyId::generate()).await;
    }

    #[tokio::test]
    async fn keeps_the_unverified_mark() {
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::unreachable(),
        );
        contract::keeps_the_unverified_mark(&service, CompanyId::generate()).await;
    }

    #[tokio::test]
    async fn records_what_was_written() {
        let repository = InMemoryPartyRepository::default();
        contract::records_what_was_written(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn leaves_no_trace_of_a_write_that_did_not_happen() {
        let repository = InMemoryPartyRepository::default();
        contract::leaves_no_trace_of_a_write_that_did_not_happen(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    fn bia() -> Actor {
        Actor::User {
            id: UserId::generate(),
            name: "Bia Lima".into(),
        }
    }

    #[tokio::test]
    async fn registering_records_who_did_it_and_what_was_set() {
        let service = PartyService::new(InMemoryPartyRepository::default(), contract::registry());
        let company = CompanyId::generate();
        let party = service
            .register(
                company,
                &bia(),
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();

        let history = service.history(company, party.id()).await.unwrap();
        assert_eq!(history.len(), 1);
        let entry = &history[0].entry;
        assert!(matches!(&entry.actor, Actor::User { name, .. } if name == "Bia Lima"));
        assert_eq!(entry.action, conta_verde_domain::AuditAction::Created);
        let set: Vec<&str> = entry.changes.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(
            set,
            [
                "name",
                "document",
                "is_customer",
                "is_supplier",
                "document_check",
                "registry_legal_name",
                "registry_situation"
            ]
        );
    }

    #[tokio::test]
    async fn a_recheck_that_changes_nothing_adds_no_entry() {
        let (service, _registry, company, party_id) = unverified_party().await;
        // Still down: the same answer as before, so there is nothing to record.
        service
            .verify_document(company, &contract::actor(), party_id)
            .await
            .unwrap();
        assert_eq!(service.history(company, party_id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_recheck_that_changes_something_is_recorded_under_whoever_asked() {
        let (service, registry, company, party_id) = unverified_party().await;
        registry.recover();
        registry.add_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA");
        service
            .verify_document(company, &bia(), party_id)
            .await
            .unwrap();

        let history = service.history(company, party_id).await.unwrap();
        assert_eq!(history.len(), 2);
        // The first entry is whoever registered it, the second whoever re-checked.
        assert!(matches!(&history[0].entry.actor, Actor::User { name, .. } if name == "Ana Souza"));
        assert!(matches!(&history[1].entry.actor, Actor::User { name, .. } if name == "Bia Lima"));
        let first = &history[1].entry.changes[0];
        assert_eq!(first.field, "document_check");
        assert_eq!(first.from.as_deref(), Some("unverified"));
        assert_eq!(first.to.as_deref(), Some("verified"));
    }

    #[tokio::test]
    async fn the_history_of_another_companys_party_is_not_found() {
        let (service, _registry, _company, party_id) = unverified_party().await;
        assert_eq!(
            service
                .history(CompanyId::generate(), party_id)
                .await
                .unwrap_err(),
            ApplicationError::PartyNotFound
        );
    }

    #[tokio::test]
    async fn finds_parties_and_replaces_their_document_check() {
        let repository = InMemoryPartyRepository::default();
        contract::finds_parties_and_replaces_their_document_check(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    /// A party registered while the registry was down, and the registry handle to bring it back.
    async fn unverified_party() -> (
        PartyService<InMemoryPartyRepository, InMemoryCnpjRegistry>,
        InMemoryCnpjRegistry,
        CompanyId,
        conta_verde_domain::PartyId,
    ) {
        let registry = InMemoryCnpjRegistry::unreachable();
        // No waiting between re-checks: the tests check one right after another.
        let service = PartyService::new(InMemoryPartyRepository::default(), registry.clone())
            .with_recheck_interval(Duration::ZERO);
        let company = CompanyId::generate();
        let party = service
            .register(
                company,
                &contract::actor(),
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();
        assert_eq!(party.document_check(), &DocumentCheck::Unverified);
        (service, registry, company, party.id())
    }

    #[tokio::test]
    async fn verifying_again_replaces_the_old_answer() {
        let (service, registry, company, party_id) = unverified_party().await;

        // Still down: it stays unverified instead of failing.
        let still_down = service
            .verify_document(company, &contract::actor(), party_id)
            .await
            .unwrap();
        assert_eq!(still_down.document_check(), &DocumentCheck::Unverified);

        registry.recover();
        registry.add_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA");
        let verified = service
            .verify_document(company, &contract::actor(), party_id)
            .await
            .unwrap();
        assert!(matches!(
            verified.document_check(),
            DocumentCheck::Verified { situation, .. } if situation == "ATIVA"
        ));
        // The answer is stored, not only returned.
        let listed = service
            .list(company, &PartyQuery::default(), None, None)
            .await
            .unwrap()
            .items;
        assert_eq!(listed[0].document_check(), verified.document_check());
    }

    #[tokio::test]
    async fn re_checks_of_one_party_are_spaced_out() {
        // The default interval applies: a loop of requests must not burn the shared rate limit.
        let registry = InMemoryCnpjRegistry::unreachable();
        let service = PartyService::new(InMemoryPartyRepository::default(), registry);
        let company = CompanyId::generate();
        let first = service
            .register(
                company,
                &contract::actor(),
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();
        let second = service
            .register(
                company,
                &contract::actor(),
                draft("Oficina", Some(contract::CLOSED_ALPHANUMERIC_CNPJ)),
            )
            .await
            .unwrap();

        service
            .verify_document(company, &contract::actor(), first.id())
            .await
            .unwrap();
        assert_eq!(
            service
                .verify_document(company, &contract::actor(), first.id())
                .await
                .unwrap_err(),
            ApplicationError::RecheckTooSoon
        );
        // Another CNPJ is not held back by the first one.
        service
            .verify_document(company, &contract::actor(), second.id())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn re_checks_are_spaced_out_per_cnpj_across_companies() {
        // The rate limit is shared by the installation, so the same CNPJ registered in two companies
        // must not cost two registry calls in a row.
        let registry = InMemoryCnpjRegistry::unreachable();
        let service = PartyService::new(InMemoryPartyRepository::default(), registry);
        let (company_a, company_b) = (CompanyId::generate(), CompanyId::generate());
        let in_a = service
            .register(
                company_a,
                &contract::actor(),
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();
        let in_b = service
            .register(
                company_b,
                &contract::actor(),
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();

        service
            .verify_document(company_a, &contract::actor(), in_a.id())
            .await
            .unwrap();
        assert_eq!(
            service
                .verify_document(company_b, &contract::actor(), in_b.id())
                .await
                .unwrap_err(),
            ApplicationError::RecheckTooSoon
        );
    }

    #[tokio::test]
    async fn a_party_can_turn_out_to_be_unknown_to_the_registry() {
        let (service, registry, company, party_id) = unverified_party().await;
        registry.recover();
        let party = service
            .verify_document(company, &contract::actor(), party_id)
            .await
            .unwrap();
        assert_eq!(party.document_check(), &DocumentCheck::NotFound);
    }

    #[tokio::test]
    async fn only_a_partys_own_company_can_verify_it() {
        let (service, _registry, _company, party_id) = unverified_party().await;
        let result = service
            .verify_document(CompanyId::generate(), &contract::actor(), party_id)
            .await;
        assert_eq!(result.unwrap_err(), ApplicationError::PartyNotFound);
        let unknown = service
            .verify_document(
                CompanyId::generate(),
                &contract::actor(),
                conta_verde_domain::PartyId::generate(),
            )
            .await;
        assert_eq!(unknown.unwrap_err(), ApplicationError::PartyNotFound);
    }

    #[tokio::test]
    async fn a_party_without_a_cnpj_cannot_be_verified() {
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::default(),
        );
        let company = CompanyId::generate();
        let cpf = service
            .register(
                company,
                &contract::actor(),
                draft("Ana", Some("529.982.247-25")),
            )
            .await
            .unwrap();
        let none = service
            .register(company, &contract::actor(), draft("Sem doc", None))
            .await
            .unwrap();
        for party in [cpf, none] {
            assert_eq!(
                service
                    .verify_document(company, &contract::actor(), party.id())
                    .await
                    .unwrap_err(),
                ApplicationError::Domain(DomainError::NoCnpjToVerify)
            );
        }
    }

    #[tokio::test]
    async fn verifying_with_the_lookup_off_is_refused_and_changes_nothing() {
        let registry: Option<InMemoryCnpjRegistry> = None;
        let service = PartyService::new(InMemoryPartyRepository::default(), registry);
        let company = CompanyId::generate();
        let party = service
            .register(
                company,
                &contract::actor(),
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();
        assert_eq!(
            service
                .verify_document(company, &contract::actor(), party.id())
                .await
                .unwrap_err(),
            ApplicationError::LookupDisabled
        );
        // Nothing was asked, so the attempt did not use up the interval: it is refused the same
        // way again, not with "too soon".
        assert_eq!(
            service
                .verify_document(company, &contract::actor(), party.id())
                .await
                .unwrap_err(),
            ApplicationError::LookupDisabled
        );
        let listed = service
            .list(company, &PartyQuery::default(), None, None)
            .await
            .unwrap()
            .items;
        assert_eq!(listed[0].document_check(), &DocumentCheck::NotApplicable);
    }

    #[tokio::test]
    async fn registers_a_cnpj_the_registry_does_not_know_with_a_warning() {
        let service = PartyService::new(InMemoryPartyRepository::default(), contract::registry());
        // Valid check digits, but no such company in the registry. Its open data lags, so a
        // company opened recently must still be registrable.
        let party = service
            .register(
                CompanyId::generate(),
                &contract::actor(),
                draft("Recém-aberta", Some(contract::UNKNOWN_CNPJ)),
            )
            .await
            .unwrap();
        assert_eq!(party.document_check(), &DocumentCheck::NotFound);
        assert!(party.document_check().needs_attention());
    }

    #[tokio::test]
    async fn never_looks_a_cpf_up() {
        // If the CPF were sent to the registry it would come back unverified, not "not applicable".
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::unreachable(),
        );
        let party = service
            .register(
                CompanyId::generate(),
                &contract::actor(),
                draft("Ana", Some("529.982.247-25")),
            )
            .await
            .unwrap();
        assert_eq!(party.document_check(), &DocumentCheck::NotApplicable);
    }

    #[tokio::test]
    async fn a_disabled_lookup_claims_nothing_about_the_cnpj() {
        // Switched off on purpose: not an outage, so the party is not flagged as unchecked.
        let registry: Option<InMemoryCnpjRegistry> = None;
        let service = PartyService::new(InMemoryPartyRepository::default(), registry);
        let party = service
            .register(
                CompanyId::generate(),
                &contract::actor(),
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();
        assert_eq!(party.document_check(), &DocumentCheck::NotApplicable);
        assert!(!party.document_check().needs_attention());
    }

    #[tokio::test]
    async fn clamps_the_page_size() {
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::default(),
        );
        let company = CompanyId::generate();
        for name in ["Ana", "Bruno"] {
            service
                .register(company, &contract::actor(), draft(name, None))
                .await
                .unwrap();
        }

        let smallest = service
            .list(company, &PartyQuery::default(), None, Some(0))
            .await
            .unwrap();
        assert_eq!(smallest.items.len(), 1);
        assert!(smallest.next.is_some());

        let largest = service
            .list(company, &PartyQuery::default(), None, Some(1_000_000))
            .await
            .unwrap();
        assert_eq!(largest.items.len(), 2);
        assert!(largest.next.is_none());
    }

    #[tokio::test]
    async fn runs_the_account_lifecycle() {
        let auth = AuthService::new(
            InMemoryIdentityRepository::default(),
            Argon2SecretService::new().unwrap(),
            Duration::from_secs(3600),
        );
        contract::runs_the_account_lifecycle(&auth).await;
    }

    #[tokio::test]
    async fn expired_sessions_are_not_found() {
        let auth = AuthService::new(
            InMemoryIdentityRepository::default(),
            Argon2SecretService::new().unwrap(),
            Duration::ZERO,
        );
        contract::expired_sessions_are_rejected(&auth).await;
    }
}
