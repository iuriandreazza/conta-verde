use conta_verde_domain::{
    Actor, AuditAction, AuditEntity, AuditEntry, AuditRecord, CompanyId, Product, ProductDraft,
    ProductId, product_changes, product_creation_changes,
};

use crate::{
    ApplicationError, NameCursor, ProductQuery, ProductRepository, RepositoryError,
    list_query::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE},
};

#[derive(Debug, Clone)]
pub struct ProductPage {
    pub items: Vec<Product>,
    /// Present when more products exist after this page.
    pub next: Option<NameCursor>,
}

pub struct ProductService<R> {
    repository: R,
}

impl<R: ProductRepository> ProductService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Validates the draft and stores the product; the creation is recorded in the audit trail
    /// under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: ProductDraft,
    ) -> Result<Product, ApplicationError> {
        let product = Product::new(ProductId::generate(), company_id, draft)?;
        let audit = product_audit(
            &product,
            actor,
            AuditAction::Created,
            product_creation_changes(&product),
        );
        self.repository.save(&product, &audit).await?;
        Ok(product)
    }

    /// Replaces every editable field with the draft's. A draft that changes nothing writes nothing
    /// and records nothing: the trail lists changes, not attempts.
    ///
    /// Two people editing the same product at once is last write wins, and the "from" of the
    /// second entry is what that person saw, not necessarily what the first one left.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ProductId,
        draft: ProductDraft,
    ) -> Result<Product, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        let changes = product_changes(&before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        let audit = product_audit(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // The product vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::ProductNotFound,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// What was recorded about the product, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // A product of another company answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &ProductQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<ProductPage, ApplicationError> {
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
        Ok(ProductPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> Result<Product, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::ProductNotFound)
    }
}

fn product_audit(
    product: &Product,
    actor: &Actor,
    action: AuditAction,
    changes: Vec<conta_verde_domain::FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: product.company_id(),
        actor: actor.clone(),
        entity: AuditEntity::Product,
        entity_id: product.id().as_uuid(),
        action,
        changes,
    }
}
