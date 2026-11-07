use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

use conta_verde_application::{
    CnpjLookupError, CnpjRegistry, CompanyMembership, IdentityRepository, Member, NameCursor,
    NewAccount, NewUser, PartyQuery, PartyRepository, RegistryEntry, RepositoryError,
    StoredCredentials, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, Cnpj, Company, CompanyId, Email, Party, PartyId, Role,
    User, UserId,
};

/// Keeps parties and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened: a refused or missed write leaves nothing in the trail.
#[derive(Default, Clone)]
pub struct InMemoryPartyRepository {
    state: Arc<Mutex<PartyState>>,
}

#[derive(Default)]
struct PartyState {
    parties: Vec<Party>,
    audit: AuditLog,
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
        state.audit.record(audit);
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

    async fn update(&self, party: &Party, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, party)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let belongs = |stored: &Party| stored.company_id() == party.company_id();
        let position = state
            .parties
            .iter()
            .position(|stored| belongs(stored) && stored.id() == party.id())
            .ok_or(RepositoryError::NotFound)?;
        let duplicate = party.document().is_some()
            && state.parties.iter().any(|other| {
                belongs(other) && other.id() != party.id() && other.document() == party.document()
            });
        if duplicate {
            return Err(RepositoryError::DuplicateDocument);
        }
        state.parties[position] = party.clone();
        state.audit.record(audit);
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
            .history(company_id, AuditEntity::Party, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PartyQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<Party>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .parties
            .iter()
            .filter(|party| party.company_id() == company_id && query.matches(party))
            .cloned()
            .collect();
        Ok(keyset_page(matching, query.sort(), after, limit, |party| {
            name_key(party.name(), party.id().as_uuid())
        }))
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

/// Keeps users, companies, memberships and sessions in process memory. A clone shares the same
/// state, so a test can keep a handle to what a service was given.
#[derive(Default, Clone)]
pub struct InMemoryIdentityRepository {
    state: Arc<Mutex<IdentityState>>,
}

#[derive(Default)]
struct IdentityState {
    users: Vec<StoredCredentials>,
    companies: Vec<Company>,
    memberships: Vec<(UserId, CompanyId, Role)>,
    sessions: HashMap<String, (UserId, Instant)>,
    /// A user's reset: the digest, when it ends and when it was made.
    resets: HashMap<UserId, (String, Instant, Instant)>,
}

impl InMemoryIdentityRepository {
    /// The name of a company, for the repositories that join it to what they hold.
    pub(crate) fn company_name(&self, company_id: CompanyId) -> Option<String> {
        let state = self.state.lock().expect("repository lock poisoned");
        state
            .companies
            .iter()
            .find(|company| company.id() == company_id)
            .map(|company| company.name().to_owned())
    }
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
            .push((account.user.id(), account.company.id(), Role::Owner));
        Ok(())
    }

    async fn register_user(&self, new_user: &NewUser) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state
            .users
            .iter()
            .any(|existing| existing.user.email() == new_user.user.email())
        {
            return Err(RepositoryError::DuplicateEmail);
        }
        state.users.push(StoredCredentials {
            user: new_user.user.clone(),
            password_hash: new_user.password_hash.clone(),
        });
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

    async fn company_name(&self, company_id: CompanyId) -> Result<Option<String>, RepositoryError> {
        Ok(InMemoryIdentityRepository::company_name(self, company_id))
    }

    async fn memberships_of(
        &self,
        user_id: UserId,
    ) -> Result<Vec<CompanyMembership>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut memberships: Vec<CompanyMembership> = state
            .memberships
            .iter()
            .filter(|(member, _, _)| *member == user_id)
            .filter_map(|(_, company_id, role)| {
                let company = state
                    .companies
                    .iter()
                    .find(|company| company.id() == *company_id)?;
                Some(CompanyMembership {
                    company: company.clone(),
                    role: *role,
                })
            })
            .collect();
        memberships.sort_by_cached_key(|membership| {
            (
                membership.company.name().to_lowercase(),
                membership.company.id().as_uuid(),
            )
        });
        Ok(memberships)
    }

    async fn members_of(&self, company_id: CompanyId) -> Result<Vec<Member>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut members: Vec<Member> = state
            .memberships
            .iter()
            .filter(|(_, company, _)| *company == company_id)
            .filter_map(|(user_id, _, role)| {
                state.user(*user_id).map(|user| Member {
                    user: user.clone(),
                    role: *role,
                })
            })
            .collect();
        members.sort_by_cached_key(|member| {
            (
                member.user.name().to_lowercase(),
                member.user.id().as_uuid(),
            )
        });
        Ok(members)
    }

    async fn role_in(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> Result<Option<Role>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .memberships
            .iter()
            .find(|(member, company, _)| *member == user_id && *company == company_id)
            .map(|(_, _, role)| *role))
    }

    async fn add_membership(
        &self,
        user_id: UserId,
        company_id: CompanyId,
        role: Role,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state
            .memberships
            .iter()
            .any(|(member, company, _)| *member == user_id && *company == company_id)
        {
            return Err(RepositoryError::DuplicateMembership);
        }
        state.memberships.push((user_id, company_id, role));
        Ok(())
    }

    async fn set_role(
        &self,
        user_id: UserId,
        company_id: CompanyId,
        role: Role,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let membership = state
            .memberships
            .iter_mut()
            .find(|(member, company, current)| {
                *member == user_id && *company == company_id && *current != Role::Owner
            })
            .ok_or(RepositoryError::NotFound)?;
        membership.2 = role;
        Ok(())
    }

    async fn remove_membership(
        &self,
        user_id: UserId,
        company_id: CompanyId,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let before = state.memberships.len();
        state.memberships.retain(|(member, company, role)| {
            !(*member == user_id && *company == company_id && *role != Role::Owner)
        });
        if state.memberships.len() == before {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
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

    async fn create_password_reset(
        &self,
        user_id: UserId,
        token_digest: &str,
        lifetime: Duration,
        cooldown: Duration,
    ) -> Result<bool, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let now = Instant::now();
        if state
            .resets
            .get(&user_id)
            .is_some_and(|(_, _, made)| now.duration_since(*made) < cooldown)
        {
            return Ok(false);
        }
        state
            .resets
            .insert(user_id, (token_digest.to_owned(), now + lifetime, now));
        Ok(true)
    }

    async fn reset_password(
        &self,
        token_digest: &str,
        new_hash: &str,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let now = Instant::now();
        let user_id = state
            .resets
            .iter()
            .find(|(_, (digest, ends, _))| digest == token_digest && *ends > now)
            .map(|(user_id, _)| *user_id)
            .ok_or(RepositoryError::NotFound)?;
        state.resets.remove(&user_id);
        if let Some(credentials) = state.users.iter_mut().find(|c| c.user.id() == user_id) {
            credentials.password_hash = new_hash.to_owned();
        }
        state.sessions.retain(|_, (owner, _)| *owner != user_id);
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
            ..PartyDraft::default()
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
    async fn imports_parties_and_records_each_creation() {
        let service = PartyService::new(InMemoryPartyRepository::default(), contract::registry());
        contract::imports_parties_and_records_each_creation(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn an_import_with_the_lookup_off_claims_nothing_about_a_cnpj() {
        let registry: Option<InMemoryCnpjRegistry> = None;
        let service = PartyService::new(InMemoryPartyRepository::default(), registry);
        let company = CompanyId::generate();
        let draft = PartyDraft {
            name: "Padaria".into(),
            document: Some(contract::ACTIVE_CNPJ.into()),
            is_customer: true,
            ..PartyDraft::default()
        };
        let report = service
            .import(
                company,
                &contract::actor(),
                vec![conta_verde_application::ImportRow { row: 2, draft }],
                false,
            )
            .await
            .unwrap();
        assert_eq!(report.created, 1);
        let stored = service
            .list(company, &PartyQuery::default(), None, None)
            .await
            .unwrap()
            .items;
        assert_eq!(stored[0].document_check(), &DocumentCheck::NotApplicable);
    }

    #[tokio::test]
    async fn keeps_party_contacts_and_tags() {
        let service = PartyService::new(InMemoryPartyRepository::default(), contract::registry());
        contract::keeps_party_contacts_and_tags(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn edits_parties_and_records_only_changes() {
        let service = PartyService::new(InMemoryPartyRepository::default(), contract::registry());
        contract::edits_parties_and_records_only_changes(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
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

    /// A registry whose `refresh` answers only once told to, so a test can change the party while
    /// the registry is being asked, the way a slow network would let a person do.
    struct Gated {
        gate: Arc<tokio::sync::Notify>,
        looked_up: Result<RegistryEntry, CnpjLookupError>,
        refreshed: Result<RegistryEntry, CnpjLookupError>,
    }

    impl CnpjRegistry for Gated {
        async fn lookup(
            &self,
            _cnpj: conta_verde_domain::Cnpj<'_>,
        ) -> Result<RegistryEntry, CnpjLookupError> {
            self.looked_up.clone()
        }

        async fn refresh(
            &self,
            _cnpj: conta_verde_domain::Cnpj<'_>,
        ) -> Result<RegistryEntry, CnpjLookupError> {
            self.gate.notified().await;
            self.refreshed.clone()
        }
    }

    fn entry(legal_name: &str) -> RegistryEntry {
        RegistryEntry {
            legal_name: legal_name.into(),
            situation: "ATIVA".into(),
        }
    }

    fn gated(
        looked_up: Result<RegistryEntry, CnpjLookupError>,
        refreshed: Result<RegistryEntry, CnpjLookupError>,
    ) -> (
        PartyService<InMemoryPartyRepository, Gated>,
        Arc<tokio::sync::Notify>,
    ) {
        let gate = Arc::new(tokio::sync::Notify::new());
        let registry = Gated {
            gate: gate.clone(),
            looked_up,
            refreshed,
        };
        (
            PartyService::new(InMemoryPartyRepository::default(), registry),
            gate,
        )
    }

    #[tokio::test]
    async fn a_recheck_does_not_undo_an_edit_made_while_the_registry_was_asked() {
        let (service, gate) = gated(Err(CnpjLookupError::Unavailable), Ok(entry("FROM REFRESH")));
        let company = CompanyId::generate();
        let actor = bia();
        let party = service
            .register(
                company,
                &actor,
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();
        assert_eq!(party.document_check(), &DocumentCheck::Unverified);

        let (rechecked, edited) = tokio::join!(
            service.verify_document(company, &actor, party.id()),
            async {
                let edited = service
                    .update(
                        company,
                        &actor,
                        party.id(),
                        draft("Padaria do Centro", Some(contract::ACTIVE_CNPJ)),
                    )
                    .await;
                gate.notify_one();
                edited
            }
        );
        edited.unwrap();
        let rechecked = rechecked.unwrap();
        assert_eq!(rechecked.name(), "Padaria do Centro");
        let stored = service
            .list(company, &PartyQuery::default(), None, None)
            .await
            .unwrap()
            .items;
        assert_eq!(stored[0].name(), "Padaria do Centro");
        assert!(
            matches!(stored[0].document_check(), DocumentCheck::Verified { legal_name, .. } if legal_name == "FROM REFRESH")
        );
        // The trail has the registration, the edit and the re-check, each saying only its own change.
        assert_eq!(service.history(company, party.id()).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn an_answer_about_a_document_that_was_replaced_meanwhile_is_dropped() {
        let (service, gate) = gated(Ok(entry("FROM LOOKUP")), Ok(entry("FROM REFRESH")));
        let company = CompanyId::generate();
        let actor = bia();
        let party = service
            .register(
                company,
                &actor,
                draft("Padaria", Some(contract::ACTIVE_CNPJ)),
            )
            .await
            .unwrap();

        let (rechecked, edited) = tokio::join!(
            service.verify_document(company, &actor, party.id()),
            async {
                let edited = service
                    .update(
                        company,
                        &actor,
                        party.id(),
                        draft("Padaria", Some(contract::CLOSED_ALPHANUMERIC_CNPJ)),
                    )
                    .await;
                gate.notify_one();
                edited
            }
        );
        edited.unwrap();
        let rechecked = rechecked.unwrap();
        // The party keeps the new document, with what the lookup said about it, not the refresh of
        // the old one.
        assert_eq!(rechecked.document().unwrap().as_str(), "12ABC34501DE35");
        assert!(
            matches!(rechecked.document_check(), DocumentCheck::Verified { legal_name, .. } if legal_name == "FROM LOOKUP")
        );
        // The registration and the edit; the dropped answer wrote nothing.
        assert_eq!(service.history(company, party.id()).await.unwrap().len(), 2);
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
                "is_carrier",
                "is_seller",
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
    async fn resets_a_password_once() {
        let identities = InMemoryIdentityRepository::default();
        let auth = AuthService::new(
            identities.clone(),
            Argon2SecretService::new().unwrap(),
            Duration::from_secs(3600),
        );
        contract::resets_a_password_once(&auth, &identities, &Argon2SecretService::new().unwrap())
            .await;
    }

    #[tokio::test]
    async fn registers_a_user_with_no_company() {
        contract::registers_a_user_with_no_company(&InMemoryIdentityRepository::default()).await;
    }

    #[tokio::test]
    async fn memberships_carry_a_role() {
        contract::memberships_carry_a_role(&InMemoryIdentityRepository::default()).await;
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
