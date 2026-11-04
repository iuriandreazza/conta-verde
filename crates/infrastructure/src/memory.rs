use std::sync::Mutex;

use conta_verde_application::{PartyRepository, RepositoryError};
use conta_verde_domain::{CompanyId, Party};

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

    async fn list_by_company(&self, company_id: CompanyId) -> Result<Vec<Party>, RepositoryError> {
        let parties = self.parties.lock().expect("repository lock poisoned");
        let mut owned: Vec<Party> = parties
            .iter()
            .filter(|party| party.company_id() == company_id)
            .cloned()
            .collect();
        owned.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(owned)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{ApplicationError, PartyService};
    use conta_verde_domain::PartyDraft;

    use super::*;

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
        let service = PartyService::new(InMemoryPartyRepository::default());
        let company = CompanyId::generate();
        let other_company = CompanyId::generate();

        service
            .register(company, draft("Zeca", None))
            .await
            .unwrap();
        service.register(company, draft("Ana", None)).await.unwrap();
        service
            .register(other_company, draft("Outra", None))
            .await
            .unwrap();

        let names: Vec<String> = service
            .list(company)
            .await
            .unwrap()
            .iter()
            .map(|party| party.name().to_owned())
            .collect();
        assert_eq!(names, ["Ana", "Zeca"]);
    }

    #[tokio::test]
    async fn rejects_duplicate_document_within_a_company_only() {
        let service = PartyService::new(InMemoryPartyRepository::default());
        let company = CompanyId::generate();
        let document = Some("529.982.247-25");

        service
            .register(company, draft("Ana", document))
            .await
            .unwrap();
        let duplicate = service.register(company, draft("Ana 2", document)).await;
        assert_eq!(duplicate.unwrap_err(), ApplicationError::DuplicateDocument);

        let other = service
            .register(CompanyId::generate(), draft("Ana 3", document))
            .await;
        assert!(other.is_ok());
    }
}
