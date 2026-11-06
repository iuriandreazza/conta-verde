use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{
    AccountMovement, DateCursor, DayMovement, OpeningRepository, RepositoryError, TransferQuery,
    TransferRepository, ensure_audit_describes,
};
use conta_verde_domain::{
    AccountOpening, AuditEntity, AuditEntry, AuditRecord, BankAccountId, CalendarDate, CompanyId,
    Transfer, TransferId,
};

use crate::memory_audit::AuditLog;

/// Keeps transfers and their audit trail in process memory, for tests and for running the API without
/// a database. Clones share the same transfers.
#[derive(Default, Clone)]
pub struct InMemoryTransferRepository {
    state: Arc<Mutex<TransferState>>,
}

#[derive(Default)]
struct TransferState {
    transfers: Vec<Transfer>,
    audit: AuditLog,
}

impl TransferRepository for InMemoryTransferRepository {
    async fn save(&self, transfer: &Transfer, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, transfer)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        state.transfers.push(transfer.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn reverse(
        &self,
        reversed: &Transfer,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, reversed)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .transfers
            .iter_mut()
            .find(|stored| {
                stored.company_id() == reversed.company_id()
                    && stored.id() == reversed.id()
                    && !stored.is_reversed()
            })
            .ok_or(RepositoryError::NotFound)?;
        *stored = stored.clone().with_reversed(true);
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: TransferId,
    ) -> Result<Option<Transfer>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .transfers
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &TransferQuery,
        after: Option<&DateCursor>,
        limit: usize,
    ) -> Result<Vec<Transfer>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let key = |transfer: &Transfer| (transfer.occurred_on(), transfer.id().as_uuid());
        let mut rows: Vec<Transfer> = state
            .transfers
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

    async fn history(
        &self,
        company_id: CompanyId,
        id: TransferId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::Transfer, id.as_uuid()))
    }

    async fn movement_by_day(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<DayMovement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut days: BTreeMap<CalendarDate, AccountMovement> = BTreeMap::new();
        for transfer in state.transfers.iter().filter(|stored| {
            stored.company_id() == company_id
                && !stored.is_reversed()
                && (since..=until).contains(&stored.occurred_on())
        }) {
            let day = days.entry(transfer.occurred_on()).or_default();
            if transfer.to() == account {
                day.received += transfer.amount().as_decimal();
            }
            if transfer.from() == account {
                day.paid += transfer.amount().as_decimal();
            }
        }
        Ok(days
            .into_iter()
            .filter(|(_, moved)| !moved.received.is_zero() || !moved.paid.is_zero())
            .map(|(date, moved)| DayMovement {
                date,
                received: moved.received.normalize(),
                paid: moved.paid.normalize(),
            })
            .collect())
    }

    async fn movement_of_account(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: Option<CalendarDate>,
        until: CalendarDate,
    ) -> Result<AccountMovement, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut movement = AccountMovement::default();
        for transfer in state.transfers.iter().filter(|stored| {
            stored.company_id() == company_id
                && !stored.is_reversed()
                && stored.occurred_on() <= until
                && since.is_none_or(|day| stored.occurred_on() >= day)
        }) {
            if transfer.to() == account {
                movement.received += transfer.amount().as_decimal();
            }
            if transfer.from() == account {
                movement.paid += transfer.amount().as_decimal();
            }
        }
        movement.received = movement.received.normalize();
        movement.paid = movement.paid.normalize();
        Ok(movement)
    }
}

/// Keeps the opening balances and their audit trail in process memory. Clones share them.
#[derive(Default, Clone)]
pub struct InMemoryOpeningRepository {
    state: Arc<Mutex<OpeningState>>,
}

#[derive(Default)]
struct OpeningState {
    openings: Vec<AccountOpening>,
    audit: AuditLog,
}

impl OpeningRepository for InMemoryOpeningRepository {
    async fn set(
        &self,
        opening: &AccountOpening,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, opening)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        match state.openings.iter_mut().find(|stored| {
            stored.company_id() == opening.company_id()
                && stored.account_id() == opening.account_id()
        }) {
            Some(stored) => *stored = opening.clone(),
            None => state.openings.push(opening.clone()),
        }
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
    ) -> Result<Option<AccountOpening>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .openings
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.account_id() == account)
            .cloned())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::AccountOpening, account.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AccountOpening>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .openings
            .iter()
            .filter(|stored| stored.company_id() == company_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{BalanceService, FinancialEntryService, TransferService};

    use super::*;
    use crate::InMemoryFinancialEntryRepository;
    use crate::contract;
    use crate::memory_entries::tests::Registers;

    #[tokio::test]
    async fn moves_money_between_accounts() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let store = InMemoryTransferRepository::default();
        let service = TransferService::new(store.clone(), registers.references());
        contract::moves_money_between_accounts(&service, &store, company, other_company, &seed)
            .await;
    }

    #[tokio::test]
    async fn adds_balances_up() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let transfer_store = InMemoryTransferRepository::default();
        let openings = InMemoryOpeningRepository::default();
        let balances = BalanceService::new(
            registers.bank_accounts.clone(),
            entry_store.clone(),
            transfer_store.clone(),
            openings.clone(),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        let transfers = TransferService::new(transfer_store, registers.references());
        contract::adds_balances_up(
            &balances,
            &entries,
            &transfers,
            &openings,
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn adds_the_cash_flow_up() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entry_store = InMemoryFinancialEntryRepository::default();
        let transfer_store = InMemoryTransferRepository::default();
        let balances = BalanceService::new(
            registers.bank_accounts.clone(),
            entry_store.clone(),
            transfer_store.clone(),
            InMemoryOpeningRepository::default(),
        );
        let entries = FinancialEntryService::new(entry_store, registers.references());
        let transfers = TransferService::new(transfer_store, registers.references());
        contract::adds_the_cash_flow_up(
            &balances,
            &entries,
            &transfers,
            company,
            other_company,
            &seed,
        )
        .await;
    }
}
