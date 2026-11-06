use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CommissionRule, CommissionRuleId, CompanyId, PartyId, ProductId,
};
use uuid::Uuid;

use crate::RepositoryError;

/// Position of the last row of a list of commission rules: newest first, then by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleCursor {
    pub starts_on: conta_verde_domain::CalendarDate,
    pub id: Uuid,
}

impl From<&CommissionRule> for RuleCursor {
    fn from(rule: &CommissionRule) -> Self {
        Self {
            starts_on: rule.starts_on(),
            id: rule.id().as_uuid(),
        }
    }
}

/// How a list of commission rules is narrowed, following ADR 0018. They are listed from the one that
/// started last, then by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommissionRuleQuery {
    seller: Option<PartyId>,
    product: Option<ProductId>,
    active: Option<bool>,
}

impl CommissionRuleQuery {
    pub fn with_seller(mut self, seller: PartyId) -> Self {
        self.seller = Some(seller);
        self
    }

    pub fn with_product(mut self, product: ProductId) -> Self {
        self.product = Some(product);
        self
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = Some(active);
        self
    }

    pub fn seller(&self) -> Option<PartyId> {
        self.seller
    }

    pub fn product(&self) -> Option<ProductId> {
        self.product
    }

    pub fn active(&self) -> Option<bool> {
        self.active
    }

    /// The same rules the SQL adapter applies in the database: a rule for a seller (or a product) is the one
    /// that names them.
    pub fn matches(&self, rule: &CommissionRule) -> bool {
        self.seller
            .is_none_or(|seller| rule.seller_id() == Some(seller))
            && self
                .product
                .is_none_or(|product| rule.product_id() == Some(product))
            && self.active.is_none_or(|active| rule.is_active() == active)
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait CommissionRuleRepository: Send + Sync {
    fn save(
        &self,
        rule: &CommissionRule,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The rule, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> impl Future<Output = Result<Option<CommissionRule>, RepositoryError>> + Send;

    /// Replaces the editable fields of a stored rule: `NotFound` when the company has no such one.
    fn update(
        &self,
        rule: &CommissionRule,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the rule, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` rules of the company that match `query`, newest start first, starting right after `after`.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &CommissionRuleQuery,
        after: Option<&RuleCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<CommissionRule>, RepositoryError>> + Send;
}
