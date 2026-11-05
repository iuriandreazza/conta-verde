use conta_verde_domain::{
    Actor, AuditAction, AuditEntity, AuditEntry, AuditRecord, CashFlowGroup, Category, CategoryId,
    CompanyId, DomainError, DreGroup, FieldChange, category_changes, category_creation_changes,
    default_template,
};

use crate::category_repository::MAX_CATEGORIES;
use crate::{ApplicationError, CategoryRepository, RepositoryError};

/// What is asked to create a category. A top-level one says where it lands; a subcategory names
/// its parent and says nothing else, since it takes the groups from it.
#[derive(Debug, Clone)]
pub struct NewCategory {
    pub parent_id: Option<CategoryId>,
    pub name: String,
    pub dre_group: Option<DreGroup>,
    pub cash_flow_group: Option<CashFlowGroup>,
}

pub struct CategoryService<R> {
    repository: R,
}

impl<R: CategoryRepository> CategoryService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// The whole tree as a flat list in a fixed order; the parent id says where each one goes.
    pub async fn list(
        &self,
        company_id: CompanyId,
        active: Option<bool>,
    ) -> Result<Vec<Category>, ApplicationError> {
        Ok(self.repository.list_by_company(company_id, active).await?)
    }

    /// Creates a top-level category or a subcategory; the creation is recorded in the audit trail
    /// under `actor`, in the same write.
    pub async fn create(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        request: NewCategory,
    ) -> Result<Category, ApplicationError> {
        let all = self.repository.list_by_company(company_id, None).await?;
        if all.len() >= MAX_CATEGORIES {
            return Err(ApplicationError::CategoryLimitReached);
        }
        let category = match request.parent_id {
            None => {
                let (Some(dre_group), Some(cash_flow_group)) =
                    (request.dre_group, request.cash_flow_group)
                else {
                    return Err(DomainError::MissingGroups.into());
                };
                Category::new_root(
                    CategoryId::generate(),
                    company_id,
                    &request.name,
                    dre_group,
                    cash_flow_group,
                )?
            }
            Some(parent_id) => {
                if request.dre_group.is_some() || request.cash_flow_group.is_some() {
                    return Err(DomainError::GroupsAreInherited.into());
                }
                let parent = all
                    .iter()
                    .find(|category| category.id() == parent_id)
                    .ok_or(ApplicationError::ParentCategoryNotFound)?;
                if !parent.is_active() {
                    return Err(ApplicationError::CategoryParentInactive);
                }
                Category::new_child(CategoryId::generate(), parent, &request.name)?
            }
        };
        let audit = category_audit(
            &category,
            actor,
            AuditAction::Created,
            category_creation_changes(&category),
        );
        self.repository.save(&category, &audit).await?;
        Ok(category)
    }

    /// Renames a category or switches it on or off; the parent and the groups never change. A
    /// change that changes nothing writes nothing and records nothing. An active category is never
    /// left under an inactive one, whichever way round it is asked for. Concurrent changes are
    /// last write wins, and the use case does not see a subcategory added at the same moment.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: CategoryId,
        name: &str,
        active: bool,
    ) -> Result<Category, ApplicationError> {
        let all = self.repository.list_by_company(company_id, None).await?;
        let before = all
            .iter()
            .find(|category| category.id() == id)
            .ok_or(ApplicationError::CategoryNotFound)?;
        let after = before.changed(name, active)?;
        let changes = category_changes(before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        if before.is_active() && !active {
            let has_active_child = all
                .iter()
                .any(|category| category.parent_id() == Some(id) && category.is_active());
            if has_active_child {
                return Err(ApplicationError::CategoryHasActiveChildren);
            }
        }
        if !before.is_active() && active {
            let parent_is_inactive = before.parent_id().is_some_and(|parent_id| {
                all.iter()
                    .any(|category| category.id() == parent_id && !category.is_active())
            });
            if parent_is_inactive {
                return Err(ApplicationError::CategoryParentInactive);
            }
        }
        let audit = category_audit(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // It vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::CategoryNotFound,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// Gives a company without categories the usual ones (`default_template`), each with its entry
    /// in the audit trail, all in one write.
    pub async fn apply_default_template(
        &self,
        company_id: CompanyId,
        actor: &Actor,
    ) -> Result<Vec<Category>, ApplicationError> {
        if !self
            .repository
            .list_by_company(company_id, None)
            .await?
            .is_empty()
        {
            return Err(ApplicationError::CategoriesAlreadyExist);
        }
        let categories = default_template(company_id);
        let entries: Vec<(Category, AuditEntry)> = categories
            .iter()
            .map(|category| {
                let audit = category_audit(
                    category,
                    actor,
                    AuditAction::Created,
                    category_creation_changes(category),
                );
                (category.clone(), audit)
            })
            .collect();
        self.repository.save_all(&entries).await?;
        Ok(categories)
    }

    /// What was recorded about the category, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's category answers like an unknown one, never with its trail.
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::CategoryNotFound)?;
        Ok(self.repository.history(company_id, id).await?)
    }
}

fn category_audit(
    category: &Category,
    actor: &Actor,
    action: AuditAction,
    changes: Vec<FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: category.company_id(),
        actor: actor.clone(),
        entity: AuditEntity::Category,
        entity_id: category.id().as_uuid(),
        action,
        changes,
    }
}
