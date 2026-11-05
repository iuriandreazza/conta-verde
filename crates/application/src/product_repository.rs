use std::future::Future;

use conta_verde_domain::{AuditEntry, AuditRecord, CompanyId, Product, ProductId, ProductKind};

use crate::list_query::clean_search;
use crate::{NameCursor, NameSort, RepositoryError};

impl From<&Product> for NameCursor {
    fn from(product: &Product) -> Self {
        Self {
            name: product.name().to_owned(),
            id: product.id().as_uuid(),
        }
    }
}

/// How a list of products and services is narrowed and ordered, following the convention of
/// ADR 0018. Built through the methods below so that adapters can trust it is normalized.
///
/// Inactive items are listed unless asked otherwise: someone who deactivated a product has to be
/// able to find it again to bring it back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductQuery {
    search: Option<String>,
    kind: Option<ProductKind>,
    active: Option<bool>,
    sort: NameSort,
}

impl ProductQuery {
    /// Matches a name anywhere in it, ignoring case, or a SKU anywhere in it, ignoring case too
    /// (SKUs are stored in upper case). Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn of_kind(mut self, kind: ProductKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = Some(active);
        self
    }

    pub fn sorted(mut self, sort: NameSort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn kind(&self) -> Option<ProductKind> {
        self.kind
    }

    pub fn active(&self) -> Option<bool> {
        self.active
    }

    pub fn sort(&self) -> NameSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, product: &Product) -> bool {
        self.kind.is_none_or(|kind| product.kind() == kind)
            && self
                .active
                .is_none_or(|active| product.is_active() == active)
            && self.search().is_none_or(|text| {
                product.name().to_lowercase().contains(&text.to_lowercase())
                    || product
                        .sku()
                        .is_some_and(|sku| sku.contains(&text.to_ascii_uppercase()))
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write.
pub trait ProductRepository: Send + Sync {
    /// Fails with `DuplicateSku` when the company already has a product with that SKU.
    fn save(
        &self,
        product: &Product,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The product, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> impl Future<Output = Result<Option<Product>, RepositoryError>> + Send;

    /// Replaces every editable field of a stored product. `NotFound` when the company has no such
    /// product, `DuplicateSku` when another one of the company already has the SKU.
    fn update(
        &self,
        product: &Product,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the product, oldest first. Empty for a product of another company.
    /// Reading the trail is not itself recorded.
    fn history(
        &self,
        company_id: CompanyId,
        id: ProductId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` products of the company that match `query`, in its order, starting right
    /// after `after` when given. A cursor only means something for the query that produced it.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &ProductQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Product>, RepositoryError>> + Send;
}
