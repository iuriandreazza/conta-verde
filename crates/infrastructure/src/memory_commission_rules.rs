use std::sync::{Arc, Mutex};

use conta_verde_application::{
    CommissionRuleQuery, CommissionRuleRepository, RepositoryError, RuleCursor,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, CommissionRule, CommissionRuleId, CompanyId,
};

use crate::memory_audit::AuditLog;

/// Keeps commission rules and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that happened.
/// Clones share the same rules.
#[derive(Default, Clone)]
pub struct InMemoryCommissionRuleRepository {
    state: Arc<Mutex<RuleState>>,
}

#[derive(Default)]
struct RuleState {
    rules: Vec<CommissionRule>,
    audit: AuditLog,
}

impl CommissionRuleRepository for InMemoryCommissionRuleRepository {
    async fn save(&self, rule: &CommissionRule, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, rule)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.rules.push(rule.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> Result<Option<CommissionRule>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .rules
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        rule: &CommissionRule,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, rule)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .rules
            .iter_mut()
            .find(|stored| stored.company_id() == rule.company_id() && stored.id() == rule.id())
            .ok_or(RepositoryError::NotFound)?;
        *stored = rule.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: CommissionRuleId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::CommissionRule, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &CommissionRuleQuery,
        after: Option<&RuleCursor>,
        limit: usize,
    ) -> Result<Vec<CommissionRule>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let key = |rule: &CommissionRule| (rule.starts_on(), rule.id().as_uuid());
        let mut rows: Vec<CommissionRule> = state
            .rules
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .filter(|stored| after.is_none_or(|cursor| key(stored) < (cursor.starts_on, cursor.id)))
            .cloned()
            .collect();
        rows.sort_by_key(key);
        rows.reverse();
        rows.truncate(limit);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{CommissionService, FinancialEntryService, SaleService};

    use super::*;
    use crate::memory_entries::tests::Registers;
    use crate::{
        InMemoryFinancialEntryRepository, InMemoryPaymentTermRepository, InMemoryProductRepository,
        InMemoryQuoteRepository, InMemorySaleRepository, RegisterSalesReferences, contract,
    };

    #[tokio::test]
    async fn computes_commission_statements() {
        let registers = Registers::new();
        let (products, terms) = (
            InMemoryProductRepository::default(),
            InMemoryPaymentTermRepository::default(),
        );
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let entry_seed = registers.seed(company).await;
        let seed =
            contract::seed_sales_records(&registers.parties, &products, &terms, company).await;
        let references = || {
            RegisterSalesReferences::new(registers.parties.clone(), products.clone(), terms.clone())
        };
        let sale_store = InMemorySaleRepository::default();
        let sales = SaleService::new(
            sale_store.clone(),
            InMemoryQuoteRepository::default(),
            FinancialEntryService::new(
                InMemoryFinancialEntryRepository::default(),
                registers.references(),
            ),
            registers.references(),
            references(),
        );
        let commissions = CommissionService::new(
            InMemoryCommissionRuleRepository::default(),
            sale_store,
            references(),
        );
        contract::computes_commission_statements(
            &commissions,
            &sales,
            company,
            other_company,
            &entry_seed,
            &seed,
        )
        .await;
    }
}
