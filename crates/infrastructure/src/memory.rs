use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use conta_verde_application::{
    CnpjLookupError, CnpjRegistry, IdentityRepository, NewAccount, PartyCursor, PartyRepository,
    RegistryEntry, RepositoryError, StoredCredentials,
};
use conta_verde_domain::{Cnpj, Company, CompanyId, Email, Party, User, UserId};
use uuid::Uuid;

/// Keeps parties in process memory. Used by tests and for running the API without a database.
#[derive(Default)]
pub struct InMemoryPartyRepository {
    parties: Mutex<Vec<Party>>,
}

impl PartyRepository for InMemoryPartyRepository {
    async fn save(&self, party: &Party) -> Result<(), RepositoryError> {
        let mut parties = self.parties.lock().expect("repository lock poisoned");
        let duplicate = party.document().is_some()
            && parties.iter().any(|existing| {
                existing.company_id() == party.company_id()
                    && existing.document() == party.document()
            });
        if duplicate {
            return Err(RepositoryError::DuplicateDocument);
        }
        parties.push(party.clone());
        Ok(())
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        after: Option<&PartyCursor>,
        limit: usize,
    ) -> Result<Vec<Party>, RepositoryError> {
        let parties = self.parties.lock().expect("repository lock poisoned");
        let sort_key = |name: &str, id: Uuid| (name.to_lowercase(), id);
        let after_key = after.map(|cursor| sort_key(&cursor.name, cursor.id.as_uuid()));

        let mut owned: Vec<Party> = parties
            .iter()
            .filter(|party| party.company_id() == company_id)
            .filter(|party| {
                after_key
                    .as_ref()
                    .is_none_or(|key| sort_key(party.name(), party.id().as_uuid()) > *key)
            })
            .cloned()
            .collect();
        owned.sort_by_cached_key(|party| sort_key(party.name(), party.id().as_uuid()));
        owned.truncate(limit);
        Ok(owned)
    }
}

/// A test double: a registry that knows only the companies it was given, or none at all.
#[derive(Default)]
pub struct InMemoryCnpjRegistry {
    companies: HashMap<String, RegistryEntry>,
    unreachable: bool,
}

impl InMemoryCnpjRegistry {
    pub fn with_company(mut self, cnpj: &str, legal_name: &str, situation: &str) -> Self {
        self.companies.insert(
            cnpj.to_owned(),
            RegistryEntry {
                legal_name: legal_name.to_owned(),
                situation: situation.to_owned(),
            },
        );
        self
    }

    /// Every lookup fails as if the service were down.
    pub fn unreachable() -> Self {
        Self {
            unreachable: true,
            ..Self::default()
        }
    }
}

impl CnpjRegistry for InMemoryCnpjRegistry {
    async fn lookup(&self, cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
        if self.unreachable {
            return Err(CnpjLookupError::Unavailable);
        }
        self.companies
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
    use conta_verde_domain::{DocumentCheck, DomainError, PartyDraft};

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
            .register(company, draft("Zeca", None))
            .await
            .unwrap();
        service.register(company, draft("ana", None)).await.unwrap();
        service
            .register(other_company, draft("Outra", None))
            .await
            .unwrap();

        let names: Vec<String> = service
            .list(company, None, None)
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
            .register(company, draft("Ana", document))
            .await
            .unwrap();
        let duplicate = service.register(company, draft("Ana 2", document)).await;
        assert_eq!(
            duplicate.unwrap_err(),
            ApplicationError::Repository(RepositoryError::DuplicateDocument)
        );

        let other = service
            .register(CompanyId::generate(), draft("Ana 3", document))
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
    async fn rejects_a_cnpj_the_registry_does_not_know() {
        let service = PartyService::new(InMemoryPartyRepository::default(), contract::registry());
        // Valid check digits, but no such company in the registry.
        let result = service
            .register(
                CompanyId::generate(),
                draft("Fantasma", Some("00.000.000/0001-91")),
            )
            .await;
        assert_eq!(
            result.unwrap_err(),
            ApplicationError::Domain(DomainError::DocumentNotRegistered)
        );
    }

    #[tokio::test]
    async fn never_looks_a_cpf_up() {
        // If the CPF were sent to the registry it would come back unverified, not "not applicable".
        let service = PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::unreachable(),
        );
        let party = service
            .register(CompanyId::generate(), draft("Ana", Some("529.982.247-25")))
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
            service.register(company, draft(name, None)).await.unwrap();
        }

        let smallest = service.list(company, None, Some(0)).await.unwrap();
        assert_eq!(smallest.items.len(), 1);
        assert!(smallest.next.is_some());

        let largest = service.list(company, None, Some(1_000_000)).await.unwrap();
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
