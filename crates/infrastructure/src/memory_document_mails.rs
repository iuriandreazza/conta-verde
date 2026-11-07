use std::sync::{Arc, Mutex};

use conta_verde_application::{DocumentMailRepository, RepositoryError, Reservation};
use conta_verde_domain::{CompanyId, DocumentMail, DocumentMailId, DocumentMailKind};
use uuid::Uuid;

/// Keeps the log of e-mailed documents in process memory, for tests and for running the API without a database. Clones
/// share the same log.
#[derive(Default, Clone)]
pub struct InMemoryDocumentMailRepository {
    mails: Arc<Mutex<Vec<DocumentMail>>>,
}

impl DocumentMailRepository for InMemoryDocumentMailRepository {
    async fn reserve(
        &self,
        mail: &DocumentMail,
        cooldown_millis: i64,
        day_millis: i64,
        cap: u32,
    ) -> Result<Reservation, RepositoryError> {
        let mut mails = self.mails.lock().expect("log lock poisoned");
        let of_company = || {
            mails
                .iter()
                .filter(|stored| stored.company_id == mail.company_id)
        };
        let too_soon = of_company().any(|stored| {
            stored.kind == mail.kind
                && stored.document_id == mail.document_id
                && stored.to == mail.to
                && stored.sent_at_millis > mail.sent_at_millis - cooldown_millis
        });
        if too_soon {
            return Ok(Reservation::TooSoon);
        }
        let today = of_company()
            .filter(|stored| stored.sent_at_millis > mail.sent_at_millis - day_millis)
            .count();
        if today >= cap as usize {
            return Ok(Reservation::CapReached);
        }
        mails.push(mail.clone());
        Ok(Reservation::Reserved)
    }

    async fn release(
        &self,
        company_id: CompanyId,
        id: DocumentMailId,
    ) -> Result<(), RepositoryError> {
        let mut mails = self.mails.lock().expect("log lock poisoned");
        mails.retain(|stored| !(stored.company_id == company_id && stored.id == id));
        Ok(())
    }

    async fn of_document(
        &self,
        company_id: CompanyId,
        kind: DocumentMailKind,
        document_id: Uuid,
        limit: usize,
    ) -> Result<Vec<DocumentMail>, RepositoryError> {
        let mails = self.mails.lock().expect("log lock poisoned");
        let mut found: Vec<DocumentMail> = mails
            .iter()
            .filter(|stored| {
                stored.company_id == company_id
                    && stored.kind == kind
                    && stored.document_id == document_id
            })
            .cloned()
            .collect();
        found.sort_by_key(|stored| std::cmp::Reverse((stored.sent_at_millis, stored.id.as_uuid())));
        found.truncate(limit);
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn limits_and_logs_the_documents_that_are_mailed() {
        contract::limits_and_logs_the_documents_that_are_mailed(
            &InMemoryDocumentMailRepository::default(),
            CompanyId::generate(),
            CompanyId::generate(),
            conta_verde_domain::UserId::generate(),
        )
        .await;
    }
}
