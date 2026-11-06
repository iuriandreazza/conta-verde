use std::sync::{Arc, Mutex};

use conta_verde_application::{CategoryRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{AuditEntity, AuditEntry, AuditRecord, Category, CategoryId, CompanyId};

use crate::memory_audit::AuditLog;

/// Keeps categories and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened: a refused or missed write leaves nothing in the trail.
#[derive(Default, Clone)]
pub struct InMemoryCategoryRepository {
    state: Arc<Mutex<CategoryState>>,
}

#[derive(Default)]
struct CategoryState {
    categories: Vec<Category>,
    audit: AuditLog,
}

/// What the database's constraints say about adding or renaming `category` among `existing`:
/// its parent fits (see below), siblings (the roots among them) have distinct names ignoring case,
/// and there is one transfer category.
fn check_fits(existing: &[Category], category: &Category) -> Result<(), RepositoryError> {
    if let Some(parent_id) = category.parent_id() {
        // As the database's foreign key: a category of the same company, one level up, whose groups
        // this one carries.
        let has_parent = existing.iter().any(|other| {
            other.id() == parent_id
                && other.company_id() == category.company_id()
                && other.dre_group() == category.dre_group()
                && other.cash_flow_group() == category.cash_flow_group()
                && other.level() + 1 == category.level()
        });
        if !has_parent {
            return Err(RepositoryError::NotFound);
        }
    }
    let others = || {
        existing.iter().filter(|other| {
            other.company_id() == category.company_id() && other.id() != category.id()
        })
    };
    if others().any(|other| {
        other.parent_id() == category.parent_id()
            && other.name().to_lowercase() == category.name().to_lowercase()
    }) {
        return Err(RepositoryError::DuplicateName);
    }
    if category.is_transfer() && others().any(Category::is_transfer) {
        return Err(RepositoryError::DuplicateTransferCategory);
    }
    Ok(())
}

impl CategoryRepository for InMemoryCategoryRepository {
    async fn save(&self, category: &Category, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, category)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        check_fits(&state.categories, category)?;
        state.categories.push(category.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn save_all(&self, categories: &[(Category, AuditEntry)]) -> Result<(), RepositoryError> {
        for (category, audit) in categories {
            ensure_audit_describes(audit, category)?;
        }
        let mut state = self.state.lock().expect("repository lock poisoned");
        // Checked on a copy, so that one refused category leaves the real state untouched.
        let mut working = state.categories.clone();
        for (category, _) in categories {
            check_fits(&working, category)?;
            working.push(category.clone());
        }
        state.categories = working;
        for (_, audit) in categories {
            state.audit.record(audit);
        }
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> Result<Option<Category>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .categories
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(&self, category: &Category, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, category)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .categories
            .iter()
            .position(|stored| {
                stored.company_id() == category.company_id() && stored.id() == category.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        check_fits(&state.categories, category)?;
        // Only the name and the active flag change, as in the database's UPDATE.
        let changed = state.categories[position]
            .changed(category.name(), category.is_active())
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        state.categories[position] = changed;
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Category, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        active: Option<bool>,
    ) -> Result<Vec<Category>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<Category> = state
            .categories
            .iter()
            .filter(|stored| {
                stored.company_id() == company_id
                    && active.is_none_or(|active| stored.is_active() == active)
            })
            .cloned()
            .collect();
        found.sort_by_cached_key(|stored| (stored.name().to_lowercase(), stored.id().as_uuid()));
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::CategoryService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn stores_categories_with_their_trail() {
        let repository = InMemoryCategoryRepository::default();
        contract::stores_categories_with_their_trail(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn saves_a_batch_all_or_nothing() {
        let repository = InMemoryCategoryRepository::default();
        contract::saves_a_batch_all_or_nothing(&repository, CompanyId::generate()).await;
    }

    #[tokio::test]
    async fn applies_the_default_template_once() {
        let service = CategoryService::new(InMemoryCategoryRepository::default());
        contract::applies_the_default_template_once(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn keeps_the_tree_consistent() {
        let service = CategoryService::new(InMemoryCategoryRepository::default());
        contract::keeps_the_tree_consistent(&service, CompanyId::generate()).await;
    }

    #[tokio::test]
    async fn a_company_has_at_most_a_thousand_categories() {
        use conta_verde_application::{MAX_CATEGORIES, NewCategory};
        use conta_verde_domain::{
            AuditAction, AuditEntity, AuditEntry, CashFlowGroup, CategoryId, DreGroup,
            category_creation_changes,
        };

        let company = CompanyId::generate();
        let who = contract::actor();
        let batch: Vec<_> = (0..MAX_CATEGORIES)
            .map(|index| {
                let category = Category::new_root(
                    CategoryId::generate(),
                    company,
                    &format!("Categoria {index}"),
                    DreGroup::Costs,
                    CashFlowGroup::Operating,
                )
                .unwrap();
                let entry = AuditEntry {
                    company_id: company,
                    actor: who.clone(),
                    entity: AuditEntity::Category,
                    entity_id: category.id().as_uuid(),
                    action: AuditAction::Created,
                    changes: category_creation_changes(&category),
                };
                (category, entry)
            })
            .collect();
        let repository = InMemoryCategoryRepository::default();
        repository.save_all(&batch).await.unwrap();
        let service = CategoryService::new(repository);
        let one_more = NewCategory {
            parent_id: None,
            name: "Uma a mais".into(),
            dre_group: Some(DreGroup::Costs),
            cash_flow_group: Some(CashFlowGroup::Operating),
        };
        assert_eq!(
            service.create(company, &who, one_more).await,
            Err(conta_verde_application::ApplicationError::CategoryLimitReached)
        );
    }
}
