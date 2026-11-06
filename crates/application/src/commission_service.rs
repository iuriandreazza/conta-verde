use std::collections::BTreeMap;

use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CalendarDate, CommissionRule, CommissionRuleDraft,
    CommissionRuleId, CompanyId, Decimal, PartyId, Sale, SaleStatus, commission_of_sale,
    commission_rule_changes, commission_rule_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, CommissionRuleQuery, CommissionRuleRepository, NumberCursor, RepositoryError,
    RuleCursor, SaleQuery, SaleRepository, SalesReferences,
};

/// The most sales one statement adds up: a statement is a page a person reads, and a longer span is cut.
const MAX_STATEMENT_SALES: usize = 2000;
const PAGE: usize = 200;

#[derive(Debug, Clone)]
pub struct CommissionRulePage {
    pub items: Vec<CommissionRule>,
    /// Present when more rules exist after this page.
    pub next: Option<RuleCursor>,
}

/// One sale on a statement: what it was worth net of returns, and what its seller earned on it.
#[derive(Debug, Clone)]
pub struct StatementSale {
    pub sale: Sale,
    pub base: Decimal,
    pub commission: Decimal,
}

/// What one seller earned over the span, sale by sale.
#[derive(Debug, Clone)]
pub struct SellerStatement {
    pub seller_id: PartyId,
    pub sales: Vec<StatementSale>,
    pub base: Decimal,
    pub commission: Decimal,
}

/// What the sellers earned over a span, by the day of the sale ([ADR 0041]).
#[derive(Debug, Clone)]
pub struct CommissionStatement {
    pub from: CalendarDate,
    pub to: CalendarDate,
    pub sellers: Vec<SellerStatement>,
    pub base: Decimal,
    pub commission: Decimal,
}

/// Commission rules, and the statement they make of the completed sales ([ADR 0041]). A statement is added up
/// when it is read, from the sales and the rules in force on the day of each: nothing is stored, so a return
/// that comes later is in it.
pub struct CommissionService<R, S, F> {
    rules: R,
    sales: S,
    references: F,
}

impl<R, S, F> CommissionService<R, S, F>
where
    R: CommissionRuleRepository,
    S: SaleRepository,
    F: SalesReferences,
{
    pub fn new(rules: R, sales: S, references: F) -> Self {
        Self {
            rules,
            sales,
            references,
        }
    }

    /// Validates the draft and what it points at, and stores the rule with its creation in the audit trail.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: CommissionRuleDraft,
    ) -> Result<CommissionRule, ApplicationError> {
        let rule = CommissionRule::new(CommissionRuleId::generate(), company_id, draft)?;
        self.check_references(&rule, None).await?;
        let audit = audit_entry(
            &rule,
            actor,
            AuditAction::Created,
            commission_rule_creation_changes(&rule),
        );
        self.rules.save(&rule, &audit).await?;
        Ok(rule)
    }

    /// Replaces the editable fields. A draft that changes nothing writes nothing and records nothing.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: CommissionRuleId,
        draft: CommissionRuleDraft,
    ) -> Result<CommissionRule, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        let changes = commission_rule_changes(&before, &after);
        if changes.is_empty() {
            return Ok(before);
        }
        self.check_references(&after, Some(&before)).await?;
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.rules
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::CommissionRuleNotFound,
                other => other.into(),
            })?;
        Ok(after)
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> Result<CommissionRule, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the rule, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's rule answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.rules.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &CommissionRuleQuery,
        after: Option<&RuleCursor>,
        limit: Option<usize>,
    ) -> Result<CommissionRulePage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .rules
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(CommissionRulePage { items, next })
    }

    /// What the sellers (or one of them) earned on the completed sales sold from `from` through `to`, net of
    /// what came back, by the rules in force on each sale's day. A sale without a seller earns nothing and is
    /// not listed; one that is cancelled is not a sale. More than 2000 sales are refused: narrow the span.
    pub async fn statement(
        &self,
        company_id: CompanyId,
        seller: Option<PartyId>,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<CommissionStatement, ApplicationError> {
        if from > to {
            return Err(ApplicationError::InvalidSpan("from must not be after to"));
        }
        let mut query = SaleQuery::default()
            .with_status(SaleStatus::Completed)
            .sold_between(from, to)
            .sorted(crate::NumberSort::NumberAsc);
        if let Some(seller) = seller {
            query = query.with_seller(seller);
        }
        let mut sales: Vec<Sale> = Vec::new();
        let mut after: Option<NumberCursor> = None;
        loop {
            let page = self
                .sales
                .list_by_company(company_id, &query, after.as_ref(), PAGE + 1)
                .await?;
            let has_more = page.len() > PAGE;
            let kept = page.into_iter().take(PAGE).collect::<Vec<_>>();
            after = kept.last().map(NumberCursor::from);
            sales.extend(kept);
            if sales.len() > MAX_STATEMENT_SALES {
                return Err(ApplicationError::InvalidSpan(
                    "more than 2000 sales: narrow the period",
                ));
            }
            if !has_more {
                break;
            }
        }
        sales.retain(|sale| sale.seller_id().is_some());
        let ids: Vec<_> = sales.iter().map(Sale::id).collect();
        let returns = self.sales.returns_of_sales(company_id, &ids).await?;
        let rules = self.all_rules(company_id).await?;
        let mut by_seller: BTreeMap<uuid::Uuid, SellerStatement> = BTreeMap::new();
        for sale in sales {
            let made: Vec<_> = returns
                .iter()
                .filter(|made| made.sale_id() == sale.id())
                .cloned()
                .collect();
            let (Some(seller_id), Some(earned)) =
                (sale.seller_id(), commission_of_sale(&sale, &made, &rules))
            else {
                continue;
            };
            let statement =
                by_seller
                    .entry(seller_id.as_uuid())
                    .or_insert_with(|| SellerStatement {
                        seller_id,
                        sales: Vec::new(),
                        base: Decimal::ZERO,
                        commission: Decimal::ZERO,
                    });
            statement.base += earned.base;
            statement.commission += earned.commission;
            statement.sales.push(StatementSale {
                sale,
                base: earned.base,
                commission: earned.commission,
            });
        }
        let sellers: Vec<SellerStatement> = by_seller
            .into_values()
            .map(|mut statement| {
                statement.base = statement.base.normalize();
                statement.commission = statement.commission.normalize();
                statement
            })
            .collect();
        Ok(CommissionStatement {
            from,
            to,
            base: sellers.iter().map(|s| s.base).sum::<Decimal>().normalize(),
            commission: sellers
                .iter()
                .map(|s| s.commission)
                .sum::<Decimal>()
                .normalize(),
            sellers,
        })
    }

    /// Every rule of the company, read page by page.
    async fn all_rules(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<CommissionRule>, ApplicationError> {
        let query = CommissionRuleQuery::default().with_active(true);
        let mut rules = Vec::new();
        let mut after: Option<RuleCursor> = None;
        loop {
            let page = self
                .rules
                .list_by_company(company_id, &query, after.as_ref(), PAGE + 1)
                .await?;
            let has_more = page.len() > PAGE;
            let kept = page.into_iter().take(PAGE).collect::<Vec<_>>();
            after = kept.last().map(RuleCursor::from);
            rules.extend(kept);
            if !has_more {
                return Ok(rules);
            }
        }
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> Result<CommissionRule, ApplicationError> {
        self.rules
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::CommissionRuleNotFound)
    }

    /// The seller and the product the rule names must be usable, only when the edit changes them.
    async fn check_references(
        &self,
        rule: &CommissionRule,
        before: Option<&CommissionRule>,
    ) -> Result<(), ApplicationError> {
        let company = rule.company_id();
        if let Some(seller) = rule.seller_id()
            && before.is_none_or(|stored| stored.seller_id() != rule.seller_id())
            && !self.references.seller_is_usable(company, seller).await?
        {
            return Err(ApplicationError::UnusableReference("seller"));
        }
        if let Some(product) = rule.product_id()
            && before.is_none_or(|stored| stored.product_id() != rule.product_id())
            && !self.references.product_is_usable(company, product).await?
        {
            return Err(ApplicationError::UnusableReference("product"));
        }
        Ok(())
    }
}
