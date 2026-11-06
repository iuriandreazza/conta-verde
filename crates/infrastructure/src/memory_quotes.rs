use std::sync::{Arc, Mutex};

use conta_verde_application::{
    LinkChange, NumberCursor, NumberSort, QuoteQuery, QuoteRepository, RepositoryError,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CompanyId, Quote, QuoteId, QuoteStatus,
};

use crate::memory_audit::AuditLog;

/// Keeps quotes and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened. Clones share the same quotes.
#[derive(Default, Clone)]
pub struct InMemoryQuoteRepository {
    state: Arc<Mutex<QuoteState>>,
}

#[derive(Default)]
struct QuoteState {
    /// Each with the digest of the token of its link.
    quotes: Vec<(Quote, Option<String>)>,
    audit: AuditLog,
}

impl QuoteRepository for InMemoryQuoteRepository {
    async fn save(&self, quote: &Quote, audit: &AuditEntry) -> Result<i64, RepositoryError> {
        ensure_audit_describes(audit, quote)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let number = state
            .quotes
            .iter()
            .filter(|(stored, _)| stored.company_id() == quote.company_id())
            .map(|(stored, _)| stored.number())
            .max()
            .unwrap_or(0)
            + 1;
        state.quotes.push((quote.clone().with_number(number), None));
        state.audit.record(audit);
        Ok(number)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: QuoteId,
    ) -> Result<Option<Quote>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .quotes
            .iter()
            .find(|(stored, _)| stored.company_id() == company_id && stored.id() == id)
            .map(|(stored, _)| stored.clone()))
    }

    async fn find_by_link(&self, digest: &str) -> Result<Option<Quote>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .quotes
            .iter()
            .find(|(_, link)| link.as_deref() == Some(digest))
            .map(|(stored, _)| stored.clone()))
    }

    async fn update(
        &self,
        quote: &Quote,
        expected: QuoteStatus,
        link: LinkChange,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, quote)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let (stored, stored_link) = state
            .quotes
            .iter_mut()
            .find(|(stored, _)| {
                stored.company_id() == quote.company_id() && stored.id() == quote.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if stored.status() != expected {
            return Err(RepositoryError::QuoteChanged);
        }
        // The number is the store's, not the draft's.
        *stored = quote.clone().with_number(stored.number());
        match link {
            LinkChange::Revoke => *stored_link = None,
            LinkChange::Set(digest) => *stored_link = Some(digest),
        }
        state.audit.record(audit);
        Ok(())
    }

    async fn answer(
        &self,
        quote: &Quote,
        link_digest: &str,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, quote)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let (stored, _) = state
            .quotes
            .iter_mut()
            .find(|(stored, link)| {
                stored.company_id() == quote.company_id()
                    && stored.id() == quote.id()
                    && stored.status() == QuoteStatus::Sent
                    && link.as_deref() == Some(link_digest)
            })
            .ok_or(RepositoryError::QuoteChanged)?;
        // Only the status moves: what the customer saw stays as it is.
        *stored = stored
            .answered(quote.status() == QuoteStatus::Approved)
            .map_err(|_| RepositoryError::QuoteChanged)?;
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: QuoteId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Quote, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &QuoteQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> Result<Vec<Quote>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let descending = query.sort() == NumberSort::NumberDesc;
        let key = |quote: &Quote| (quote.number(), quote.id().as_uuid());
        let mut rows: Vec<Quote> = state
            .quotes
            .iter()
            .map(|(stored, _)| stored)
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .filter(|stored| {
                after.is_none_or(|cursor| {
                    let position = (cursor.number, cursor.id);
                    if descending {
                        key(stored) < position
                    } else {
                        key(stored) > position
                    }
                })
            })
            .cloned()
            .collect();
        rows.sort_by_key(key);
        if descending {
            rows.reverse();
        }
        rows.truncate(limit);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::QuoteService;

    use super::*;
    use crate::{
        Argon2SecretService, InMemoryEmailSender, InMemoryPartyRepository,
        InMemoryPaymentTermRepository, InMemoryProductRepository, RegisterSalesReferences,
        contract,
    };

    #[tokio::test]
    async fn runs_the_quote_lifecycle() {
        let (parties, products, terms) = (
            InMemoryPartyRepository::default(),
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = contract::seed_sales_records(&parties, &products, &terms, company).await;
        let references =
            || RegisterSalesReferences::new(parties.clone(), products.clone(), terms.clone());
        contract::sales_references_say_what_is_usable(&references(), company, other_company, &seed)
            .await;
        let emails = InMemoryEmailSender::default();
        let store = InMemoryQuoteRepository::default();
        let service = QuoteService::new(
            store.clone(),
            references(),
            emails.clone(),
            Argon2SecretService::new().unwrap(),
            "https://erp.example",
        );
        contract::runs_the_quote_lifecycle(
            &service,
            &store,
            &emails,
            company,
            other_company,
            &seed,
        )
        .await;
    }
}
