use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, PaymentMethod, PaymentMethodDraft, PaymentMethodId,
    payment_method_changes, payment_method_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, NameCursor, PaymentMethodQuery, PaymentMethodRepository, RepositoryError,
};

#[derive(Debug, Clone)]
pub struct PaymentMethodPage {
    pub items: Vec<PaymentMethod>,
    /// Present when more payment methods exist after this page.
    pub next: Option<NameCursor>,
}

pub struct PaymentMethodService<R> {
    repository: R,
}

impl<R: PaymentMethodRepository> PaymentMethodService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Validates the draft and stores the payment method; the creation is recorded in the audit trail
    /// under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: PaymentMethodDraft,
    ) -> Result<PaymentMethod, ApplicationError> {
        let record = PaymentMethod::new(PaymentMethodId::generate(), company_id, draft)?;
        let audit = audit_entry(
            &record,
            actor,
            AuditAction::Created,
            payment_method_creation_changes(&record),
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
        id: PaymentMethodId,
        draft: PaymentMethodDraft,
    ) -> Result<PaymentMethod, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        let changes = payment_method_changes(&before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // It vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::PaymentMethodNotFound,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// What was recorded about the payment method, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's payment method answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &PaymentMethodQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<PaymentMethodPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(PaymentMethodPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> Result<PaymentMethod, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::PaymentMethodNotFound)
    }
}
