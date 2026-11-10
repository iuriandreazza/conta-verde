use crate::{Actor, BankAccountId, BankConnectionId, CalendarDate, CompanyId, DomainError};

/// How many connections a company may keep, disconnected ones included.
pub const MAX_BANK_CONNECTIONS: usize = 50;
/// What is kept of why a sync failed: a short reason on one line, never what the aggregator answered.
pub const MAX_SYNC_ERROR_CHARS: usize = 200;

/// Where a connection stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Pulled every day.
    Connected,
    /// The bank needs the person: a login that failed, a consent that expired. Nothing is pulled until it is made again.
    NeedsAttention,
    /// Ended by the person. Never pulled again; kept for its history.
    Disconnected,
}

impl ConnectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::NeedsAttention => "needs_attention",
            Self::Disconnected => "disconnected",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Connected, Self::NeedsAttention, Self::Disconnected]
            .into_iter()
            .find(|status| status.as_str() == raw)
    }
}

/// The company's link between one of its bank accounts and an account at an aggregator ([ADR 0089]): which bank account is
/// fed, by which provider, from which of the aggregator's accounts. **It holds no credential**: the aggregator's keys are the
/// installation's, and an item and an account id give nothing without them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankConnection {
    id: BankConnectionId,
    company_id: CompanyId,
    bank_account_id: BankAccountId,
    /// The aggregator, by its key (`pluggy`).
    provider: String,
    /// The aggregator's connection with the bank (its "item"): what the person made in its consent flow.
    item_id: String,
    /// The aggregator's account inside it.
    account_id: String,
    status: ConnectionStatus,
    /// The last day a pull was done through. Bookkeeping, not audited.
    last_synced_on: Option<CalendarDate>,
    /// Why the last pull failed, if it did. Bookkeeping, not audited.
    last_error: Option<String>,
    /// Whoever connected it: the pulls are made, and audited, in their name.
    connected_by: Actor,
    /// Unix milliseconds.
    created_at: i64,
}

impl BankConnection {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: BankConnectionId,
        company_id: CompanyId,
        bank_account_id: BankAccountId,
        provider: &str,
        item_id: &str,
        account_id: &str,
        connected_by: Actor,
        now_millis: i64,
    ) -> Result<Self, DomainError> {
        let provider = provider.trim();
        if provider.is_empty()
            || provider.len() > 30
            || !provider.bytes().all(|byte| byte.is_ascii_lowercase())
        {
            return Err(DomainError::InvalidBankConnection("provider"));
        }
        Ok(Self {
            id,
            company_id,
            bank_account_id,
            provider: provider.to_owned(),
            item_id: external_id(item_id, "item")?,
            account_id: external_id(account_id, "account")?,
            status: ConnectionStatus::Connected,
            last_synced_on: None,
            last_error: None,
            connected_by,
            created_at: now_millis,
        })
    }

    /// The connection as it was stored.
    #[allow(clippy::too_many_arguments)]
    pub fn restored(
        id: BankConnectionId,
        company_id: CompanyId,
        bank_account_id: BankAccountId,
        provider: String,
        item_id: String,
        account_id: String,
        status: ConnectionStatus,
        last_synced_on: Option<CalendarDate>,
        last_error: Option<String>,
        connected_by: Actor,
        created_at: i64,
    ) -> Self {
        Self {
            id,
            company_id,
            bank_account_id,
            provider,
            item_id,
            account_id,
            status,
            last_synced_on,
            last_error,
            connected_by,
            created_at,
        }
    }

    /// The same connection in another status, with why when it needs the person.
    pub fn with_status(&self, status: ConnectionStatus, error: Option<&str>) -> Self {
        let mut next = self.clone();
        next.status = status;
        next.last_error = error.map(sync_error);
        next
    }

    pub fn synced_through(&self, day: CalendarDate) -> Self {
        let mut next = self.clone();
        next.last_synced_on = Some(day);
        next.last_error = None;
        next
    }

    pub fn id(&self) -> BankConnectionId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn bank_account_id(&self) -> BankAccountId {
        self.bank_account_id
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn item_id(&self) -> &str {
        &self.item_id
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn status(&self) -> ConnectionStatus {
        self.status
    }

    pub fn last_synced_on(&self) -> Option<CalendarDate> {
        self.last_synced_on
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn connected_by(&self) -> &Actor {
        &self.connected_by
    }

    pub fn created_at(&self) -> i64 {
        self.created_at
    }
}

/// An id the aggregator gave: 1 to 100 printable characters with no space.
fn external_id(raw: &str, what: &'static str) -> Result<String, DomainError> {
    let id = raw.trim();
    if id.is_empty() || id.len() > 100 || !id.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(DomainError::InvalidBankConnection(what));
    }
    Ok(id.to_owned())
}

/// A short reason, on one line.
pub fn sync_error(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_SYNC_ERROR_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserId;

    fn make(provider: &str, item: &str, account: &str) -> Result<BankConnection, DomainError> {
        BankConnection::new(
            BankConnectionId::generate(),
            CompanyId::generate(),
            BankAccountId::generate(),
            provider,
            item,
            account,
            Actor::User {
                id: UserId::generate(),
                name: "Ana".into(),
            },
            0,
        )
    }

    #[test]
    fn a_connection_names_its_provider_and_the_aggregators_ids_and_holds_no_secret() {
        let connection = make(" pluggy ", " 8f1c-item ", "acc_1").unwrap();
        assert_eq!(
            (
                connection.provider(),
                connection.item_id(),
                connection.account_id()
            ),
            ("pluggy", "8f1c-item", "acc_1")
        );
        assert_eq!(connection.status(), ConnectionStatus::Connected);
        for (provider, item, account) in [
            ("", "i", "a"),
            ("Pluggy", "i", "a"),
            ("plug gy", "i", "a"),
            ("pluggy", "", "a"),
            ("pluggy", "has space", "a"),
            ("pluggy", "i", "tab\there"),
            ("pluggy", &"x".repeat(101), "a"),
        ] {
            assert!(
                make(provider, item, account).is_err(),
                "{provider:?} {item:?} {account:?}"
            );
        }
    }

    #[test]
    fn a_failure_is_one_short_line_and_a_sync_clears_it() {
        let connection = make("pluggy", "i", "a").unwrap();
        let needs = connection.with_status(ConnectionStatus::NeedsAttention, Some("login\nfailed"));
        assert_eq!(needs.status(), ConnectionStatus::NeedsAttention);
        assert_eq!(needs.last_error(), Some("login failed"));
        let day = CalendarDate::parse("2026-11-10").unwrap();
        let synced = needs.synced_through(day);
        assert_eq!(
            (synced.last_synced_on(), synced.last_error()),
            (Some(day), None)
        );
        assert_eq!(
            sync_error(&"x".repeat(500)).chars().count(),
            MAX_SYNC_ERROR_CHARS
        );
    }
}
