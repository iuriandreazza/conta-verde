use std::sync::{Arc, Mutex};

use conta_verde_application::{
    BankAccountQuery, BankAccountRepository, NameCursor, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, BankAccount, BankAccountId, CompanyId,
};

use crate::memory_audit::AuditLog;
use crate::memory_list::{keyset_page, name_key};

/// Keeps bank accounts and their audit trail in process memory. Used by tests and for running the
/// API without a database. Like the Postgres adapter, it records an audit entry only for a write
/// that happened: a refused or missed write leaves nothing in the trail.
#[derive(Default, Clone)]
pub struct InMemoryBankAccountRepository {
    state: Arc<Mutex<BankAccountState>>,
}

#[derive(Default)]
struct BankAccountState {
    accounts: Vec<BankAccount>,
    audit: AuditLog,
}

/// The kind, bank, branch and number of an account, when the last three are known.
fn identity(account: &BankAccount) -> Option<(&'static str, &str, &str, &str)> {
    Some((
        account.kind().as_str(),
        account.bank_code()?,
        account.branch()?,
        account.number()?,
    ))
}

impl BankAccountState {
    /// What the database's unique indexes say about `account` among the others of its company:
    /// the name ignoring case, and the kind, bank, branch and number when the last three are known.
    fn conflict(&self, account: &BankAccount) -> Result<(), RepositoryError> {
        let others = || {
            self.accounts.iter().filter(|other| {
                other.company_id() == account.company_id() && other.id() != account.id()
            })
        };
        if others().any(|other| other.name().to_lowercase() == account.name().to_lowercase()) {
            return Err(RepositoryError::DuplicateName);
        }
        if identity(account).is_some() && others().any(|other| identity(other) == identity(account))
        {
            return Err(RepositoryError::DuplicateBankAccount);
        }
        Ok(())
    }
}

impl BankAccountRepository for InMemoryBankAccountRepository {
    async fn save(&self, account: &BankAccount, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, account)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.conflict(account)?;
        state.accounts.push(account.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> Result<Option<BankAccount>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .accounts
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        account: &BankAccount,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, account)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .accounts
            .iter()
            .position(|stored| {
                stored.company_id() == account.company_id()
                    && stored.id() == account.id()
                    && stored.kind() == account.kind()
            })
            .ok_or(RepositoryError::NotFound)?;
        state.conflict(account)?;
        state.accounts[position] = account.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::BankAccount, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &BankAccountQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<BankAccount>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let matching = state
            .accounts
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .cloned()
            .collect();
        Ok(keyset_page(
            matching,
            query.sort(),
            after,
            limit,
            |stored| name_key(stored.name(), stored.id().as_uuid()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::BankAccountService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract;

    #[tokio::test]
    async fn stores_bank_accounts_with_their_trail() {
        let repository = InMemoryBankAccountRepository::default();
        contract::stores_bank_accounts_with_their_trail(
            &repository,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn edits_bank_accounts_and_records_only_changes() {
        let service = BankAccountService::new(InMemoryBankAccountRepository::default());
        contract::edits_bank_accounts_and_records_only_changes(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }

    #[tokio::test]
    async fn filters_and_sorts_bank_accounts() {
        let service = BankAccountService::new(InMemoryBankAccountRepository::default());
        contract::filters_and_sorts_bank_accounts(
            &service,
            CompanyId::generate(),
            CompanyId::generate(),
        )
        .await;
    }
}
