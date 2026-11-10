use std::sync::{Arc, Mutex};

use conta_verde_application::{BankConnectionRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, BankConnection, BankConnectionId, CalendarDate,
    CompanyId, ConnectionStatus,
};

use crate::memory_audit::AuditLog;

/// Keeps bank connections and their audit trail in process memory. Used by tests and for running the API without a database.
#[derive(Default, Clone)]
pub struct InMemoryBankConnectionRepository {
    state: Arc<Mutex<ConnectionState>>,
}

#[derive(Default)]
struct ConnectionState {
    connections: Vec<BankConnection>,
    audit: AuditLog,
}

impl ConnectionState {
    fn position(&self, company_id: CompanyId, id: BankConnectionId) -> Option<usize> {
        self.connections
            .iter()
            .position(|stored| stored.company_id() == company_id && stored.id() == id)
    }
}

impl BankConnectionRepository for InMemoryBankConnectionRepository {
    async fn save(
        &self,
        connection: &BankConnection,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, connection)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let taken = state.connections.iter().any(|stored| {
            stored.company_id() == connection.company_id()
                && stored.bank_account_id() == connection.bank_account_id()
                && stored.status() != ConnectionStatus::Disconnected
        });
        if taken {
            return Err(RepositoryError::DuplicateBankConnection);
        }
        state.connections.push(connection.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn update_status(
        &self,
        connection: &BankConnection,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, connection)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .position(connection.company_id(), connection.id())
            .ok_or(RepositoryError::NotFound)?;
        // Only the status and its reason move: what a pull wrote meanwhile (the day) stays.
        let stored = &state.connections[position];
        let next = stored.with_status(connection.status(), connection.last_error());
        state.connections[position] = next;
        state.audit.record(audit);
        Ok(())
    }

    async fn record_sync(&self, connection: &BankConnection) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .position(connection.company_id(), connection.id())
            .ok_or(RepositoryError::NotFound)?;
        let stored = &state.connections[position];
        let mut next = stored.clone();
        // The day only moves forward.
        if let Some(day) = connection
            .last_synced_on()
            .filter(|day| stored.last_synced_on().is_none_or(|held| *day > held))
        {
            next = next.synced_through(day);
        }
        // A failure is written without moving the day or the status.
        next = next.with_status(stored.status(), connection.last_error());
        state.connections[position] = next;
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
    ) -> Result<Option<BankConnection>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .position(company_id, id)
            .map(|position| state.connections[position].clone()))
    }

    async fn find_by_id(
        &self,
        id: BankConnectionId,
    ) -> Result<Option<BankConnection>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .connections
            .iter()
            .find(|stored| stored.id() == id)
            .cloned())
    }

    async fn list(&self, company_id: CompanyId) -> Result<Vec<BankConnection>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<BankConnection> = state
            .connections
            .iter()
            .filter(|stored| stored.company_id() == company_id)
            .cloned()
            .collect();
        found.sort_by_key(|stored| std::cmp::Reverse((stored.created_at(), stored.id().as_uuid())));
        Ok(found)
    }

    async fn due(
        &self,
        day: CalendarDate,
        limit: usize,
    ) -> Result<Vec<BankConnection>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<BankConnection> = state
            .connections
            .iter()
            .filter(|stored| {
                stored.status() == ConnectionStatus::Connected
                    && stored.last_synced_on().is_none_or(|last| last < day)
            })
            .cloned()
            .collect();
        found.sort_by_key(|stored| (stored.last_synced_on(), stored.id().as_uuid()));
        found.truncate(limit);
        Ok(found)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::BankConnection, id.as_uuid()))
    }
}
