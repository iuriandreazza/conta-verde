use conta_verde_domain::{
    Actor, AuditAction, AuditEntity, AuditEntry, AuditRecord, CompanyId, CostCenter,
    CostCenterDraft, CostCenterId, FieldChange, cost_center_changes, cost_center_creation_changes,
};

use crate::list_query::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
use crate::{ApplicationError, CostCenterQuery, CostCenterRepository, NameCursor, RepositoryError};

#[derive(Debug, Clone)]
pub struct CostCenterPage {
    pub items: Vec<CostCenter>,
    /// Present when more cost centers exist after this page.
    pub next: Option<NameCursor>,
}

pub struct CostCenterService<R> {
    repository: R,
}

impl<R: CostCenterRepository> CostCenterService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Validates the draft and stores the cost center; the creation is recorded in the audit
    /// trail under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: CostCenterDraft,
    ) -> Result<CostCenter, ApplicationError> {
        let cost_center = CostCenter::new(CostCenterId::generate(), company_id, draft)?;
        let audit = cost_center_audit(
            &cost_center,
            actor,
            AuditAction::Created,
            cost_center_creation_changes(&cost_center),
        );
        self.repository.save(&cost_center, &audit).await?;
        Ok(cost_center)
    }

    /// Replaces the editable fields. A draft that changes nothing writes nothing and records
    /// nothing: the trail lists changes, not attempts. Concurrent edits are last write wins.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: CostCenterId,
        draft: CostCenterDraft,
    ) -> Result<CostCenter, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        let changes = cost_center_changes(&before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        let audit = cost_center_audit(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // It vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::CostCenterNotFound,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// What was recorded about the cost center, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's cost center answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &CostCenterQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<CostCenterPage, ApplicationError> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
        // One extra row tells whether another page exists without a second query.
        let mut items = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next = if has_more {
            items.last().map(NameCursor::from)
        } else {
            None
        };
        Ok(CostCenterPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> Result<CostCenter, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::CostCenterNotFound)
    }
}

fn cost_center_audit(
    cost_center: &CostCenter,
    actor: &Actor,
    action: AuditAction,
    changes: Vec<FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: cost_center.company_id(),
        actor: actor.clone(),
        entity: AuditEntity::CostCenter,
        entity_id: cost_center.id().as_uuid(),
        action,
        changes,
    }
}
