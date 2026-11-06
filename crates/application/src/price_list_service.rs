use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CompanyId, PriceList, PriceListDraft, PriceListId, ProductId,
    price_list_changes, price_list_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, NameCursor, PriceListQuery, PriceListRepository, RepositoryError,
    SalesReferences,
};

#[derive(Debug, Clone)]
pub struct PriceListPage {
    pub items: Vec<PriceList>,
    /// Present when more price lists exist after this page.
    pub next: Option<NameCursor>,
}

/// Price lists ([ADR 0042]). The products they name must be usable when a price is put in a list; one that is
/// kept as it was is not asked again, so a list can still be edited after a product was deactivated.
pub struct PriceListService<R, F> {
    repository: R,
    references: F,
}

impl<R, F> PriceListService<R, F>
where
    R: PriceListRepository,
    F: SalesReferences,
{
    pub fn new(repository: R, references: F) -> Self {
        Self {
            repository,
            references,
        }
    }

    /// Validates the draft and stores the price list; the creation is recorded in the audit trail
    /// under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: PriceListDraft,
    ) -> Result<PriceList, ApplicationError> {
        let record = PriceList::new(PriceListId::generate(), company_id, draft)?;
        self.check_products(&record, None).await?;
        let audit = audit_entry(
            &record,
            actor,
            AuditAction::Created,
            price_list_creation_changes(&record),
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
        id: PriceListId,
        draft: PriceListDraft,
    ) -> Result<PriceList, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        let changes = price_list_changes(&before, &after);
        if changes.is_empty() {
            return Ok(after);
        }
        self.check_products(&after, Some(&before)).await?;
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // It vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::PriceListNotFound,
                other => other.into(),
            })?;
        Ok(after)
    }

    /// The price list, only if it belongs to the company.
    pub async fn get(
        &self,
        company_id: CompanyId,
        id: PriceListId,
    ) -> Result<PriceList, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the price list, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: PriceListId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's price list answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &PriceListQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<PriceListPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(PriceListPage { items, next })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: PriceListId,
    ) -> Result<PriceList, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::PriceListNotFound)
    }

    /// Every product the list names, but those the stored list already named, must be usable.
    async fn check_products(
        &self,
        list: &PriceList,
        before: Option<&PriceList>,
    ) -> Result<(), ApplicationError> {
        for item in list.items() {
            let kept = before.is_some_and(|stored| {
                stored
                    .items()
                    .iter()
                    .any(|old| old.product_id() == item.product_id())
            });
            if !kept && !self.is_usable(list, item.product_id()).await? {
                return Err(ApplicationError::UnusableReference("product"));
            }
        }
        Ok(())
    }

    async fn is_usable(
        &self,
        list: &PriceList,
        product: ProductId,
    ) -> Result<bool, ApplicationError> {
        Ok(self
            .references
            .product_is_usable(list.company_id(), product)
            .await?)
    }
}
