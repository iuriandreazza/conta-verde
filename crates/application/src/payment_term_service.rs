use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, PaymentTerm, PaymentTermDraft, PaymentTermId,
    payment_term_changes, payment_term_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, NameCursor, PaymentTermQuery, PaymentTermRepository, RepositoryError,
};

#[derive(Debug, Clone)]
pub struct PaymentTermPage {
    pub items: Vec<PaymentTerm>,
    /// Present when more payment terms exist after this page.
    pub next: Option<NameCursor>,
}

pub struct PaymentTermService<R> {
    repository: R,
}

impl<R: PaymentTermRepository> PaymentTermService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Validates the draft and stores the payment term; the creation is recorded in the audit trail
    /// under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: PaymentTermDraft,
    ) -> Result<PaymentTerm, ApplicationError> {
        let record = PaymentTerm::new(PaymentTermId::generate(), company_id, draft)?;
        let audit = audit_entry(
            &record,
            actor,
            AuditAction::Created,
            payment_term_creation_changes(&record),
        );
        self.repository.save(&record, &audit).await?;
        Ok(record)
    }

    /// Replaces the editable fields. A draft that changes nothing writes nothing and records
    /// nothing: the trail lists changes, not attempts. Concurrent edits are last write wins.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: PaymentTermId,
        draft: PaymentTermDraft,
    ) -> Result<PaymentTerm, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        let changes = payment_term_changes(&before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // It vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::PaymentTermNotFound,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// The payment term, only if it belongs to the company.
    pub async fn get(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<PaymentTerm, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the payment term, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's payment term answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &PaymentTermQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<PaymentTermPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(PaymentTermPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PaymentTermId,
    ) -> Result<PaymentTerm, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::PaymentTermNotFound)
    }
}
