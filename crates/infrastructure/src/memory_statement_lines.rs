use std::sync::{Arc, Mutex};

use conta_verde_application::{
    DateCursor, MatchedTargets, RepositoryError, StatementLineQuery, StatementLineRepository,
    ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, BankAccountId, CompanyId, LineMatch, LineStatus,
    SettlementId, StatementLine, StatementLineId, TransferId,
};

use crate::memory_audit::AuditLog;

/// Keeps statement lines and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it keeps one line per fingerprint in an account and one
/// line per settlement or transfer, and records an audit entry only for a write that happened. Clones
/// share the same lines.
#[derive(Default, Clone)]
pub struct InMemoryStatementLineRepository {
    state: Arc<Mutex<LineState>>,
}

#[derive(Default)]
struct LineState {
    lines: Vec<StatementLine>,
    audit: AuditLog,
}

impl LineState {
    /// Whether another line is matched to what this one is matched to.
    fn match_taken(&self, line: &StatementLine) -> bool {
        // A settlement is matched once; a transfer once in each account, since it is a line in each.
        line.matched().is_some_and(|target| {
            self.lines.iter().any(|other| {
                other.id() != line.id()
                    && other.matched() == Some(target)
                    && (matches!(target, LineMatch::Settlement(_))
                        || other.account_id() == line.account_id())
            })
        })
    }
}

impl StatementLineRepository for InMemoryStatementLineRepository {
    async fn save(&self, line: &StatementLine, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, line)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if state.lines.iter().any(|other| {
            other.account_id() == line.account_id() && other.fingerprint() == line.fingerprint()
        }) {
            return Err(RepositoryError::DuplicateLine);
        }
        if state.match_taken(line) {
            return Err(RepositoryError::AlreadyMatched);
        }
        state.lines.push(line.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> Result<Option<StatementLine>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .lines
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        line: &StatementLine,
        was: LineStatus,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, line)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .lines
            .iter()
            .position(|stored| stored.company_id() == line.company_id() && stored.id() == line.id())
            .ok_or(RepositoryError::NotFound)?;
        if state.lines[position].status() != was {
            return Err(RepositoryError::LineChanged);
        }
        if state.match_taken(line) {
            return Err(RepositoryError::AlreadyMatched);
        }
        state.lines[position] = line.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: StatementLineId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::StatementLine, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &StatementLineQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> Result<Vec<StatementLine>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let key = |line: &StatementLine| (line.posted_on(), line.id().as_uuid());
        let mut rows: Vec<StatementLine> = state
            .lines
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .filter(|stored| after.is_none_or(|cursor| key(stored) < (cursor.date, cursor.id)))
            .cloned()
            .collect();
        rows.sort_by_key(key);
        rows.reverse();
        rows.truncate(limit);
        Ok(rows)
    }

    async fn matched_among(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        settlements: &[SettlementId],
        transfers: &[TransferId],
    ) -> Result<MatchedTargets, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found = MatchedTargets::default();
        for line in state
            .lines
            .iter()
            .filter(|line| line.company_id() == company_id)
        {
            match line.matched() {
                Some(LineMatch::Settlement(id)) if settlements.contains(&id) => {
                    found.settlements.push(id)
                }
                Some(LineMatch::Transfer(id))
                    if transfers.contains(&id) && line.account_id() == account =>
                {
                    found.transfers.push(id)
                }
                _ => {}
            }
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{FinancialEntryService, ReconciliationService, TransferService};

    use super::*;
    use crate::contract;
    use crate::memory_entries::tests::Registers;
    use crate::{InMemoryFinancialEntryRepository, InMemoryTransferRepository};

    #[tokio::test]
    async fn reconciles_statement_lines() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let store = InMemoryStatementLineRepository::default();
        let entry_store = InMemoryFinancialEntryRepository::default();
        let transfer_store = InMemoryTransferRepository::default();
        let service = ReconciliationService::new(
            store.clone(),
            FinancialEntryService::new(entry_store.clone(), registers.references()),
            TransferService::new(transfer_store.clone(), registers.references()),
            registers.references(),
        );
        contract::reconciles_statement_lines(
            &service,
            &store,
            &FinancialEntryService::new(entry_store, registers.references()),
            &TransferService::new(transfer_store, registers.references()),
            company,
            other_company,
            &seed,
        )
        .await;
    }
}
