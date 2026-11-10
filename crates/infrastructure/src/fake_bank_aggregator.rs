use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{
    AggregatedAccount, AggregatedTransaction, AggregatorError, BankAggregator, ItemState,
};
use conta_verde_domain::{CalendarDate, CompanyId};

/// What the installation uses when no aggregator is configured: nothing is connected and nothing is pulled, and OFX import is the way
/// to bring a statement in ([ADR 0089]).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBankAggregator;

impl BankAggregator for NoBankAggregator {
    fn provider(&self) -> &'static str {
        "none"
    }

    fn is_enabled(&self) -> bool {
        false
    }

    async fn connect_token(
        &self,
        _company_id: CompanyId,
        _item: Option<&str>,
    ) -> Result<String, AggregatorError> {
        Err(AggregatorError::Disabled)
    }

    async fn accounts(&self, _item: &str) -> Result<Vec<AggregatedAccount>, AggregatorError> {
        Err(AggregatorError::Disabled)
    }

    async fn item_state(&self, _item: &str) -> Result<ItemState, AggregatorError> {
        Err(AggregatorError::Disabled)
    }

    async fn transactions(
        &self,
        _account: &str,
        _from: CalendarDate,
        _to: CalendarDate,
    ) -> Result<Vec<AggregatedTransaction>, AggregatorError> {
        Err(AggregatorError::Disabled)
    }

    async fn disconnect(&self, _item: &str) -> Result<(), AggregatorError> {
        Err(AggregatorError::Disabled)
    }
}

#[derive(Default)]
struct World {
    accounts: HashMap<String, Vec<AggregatedAccount>>,
    transactions: HashMap<String, Vec<AggregatedTransaction>>,
    states: HashMap<String, ItemState>,
    /// Every pull asked for, as the account and the days.
    pulls: Vec<(String, CalendarDate, CalendarDate)>,
    disconnected: Vec<String>,
    down: bool,
    /// The items it no longer accepts, as a real one does for a consent that was revoked.
    refused: bool,
}

/// An aggregator for tests and for trying the product without an account: it knows the accounts and the movements it was told of,
/// answers pulls by their days like a real one, and can be made to fail or to say that the bank needs the person. Clones share the
/// same world.
#[derive(Default, Clone)]
pub struct FakeBankAggregator {
    world: Arc<Mutex<World>>,
}

impl FakeBankAggregator {
    pub fn with_account(self, item: &str, id: &str, name: &str) -> Self {
        self.add_account(item, id, name);
        self
    }

    pub fn add_account(&self, item: &str, id: &str, name: &str) {
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .accounts
            .entry(item.to_owned())
            .or_default()
            .push(AggregatedAccount {
                id: id.to_owned(),
                name: name.to_owned(),
                kind: "CHECKING_ACCOUNT".to_owned(),
                number: Some("12345-6".to_owned()),
            });
    }

    /// Makes it refuse every connection, as a real one does when the consent was revoked there.
    pub fn set_refused(&self, refused: bool) {
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .refused = refused;
    }

    /// A posted movement of an account.
    pub fn add_transaction(
        &self,
        account: &str,
        id: &str,
        day: &str,
        amount: &str,
        description: &str,
    ) {
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .transactions
            .entry(account.to_owned())
            .or_default()
            .push(AggregatedTransaction {
                id: id.to_owned(),
                posted_on: CalendarDate::parse(day).expect("a test day"),
                amount: amount.to_owned(),
                description: description.to_owned(),
            });
    }

    pub fn set_state(&self, item: &str, state: ItemState) {
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .states
            .insert(item.to_owned(), state);
    }

    /// While it is down every call fails as an unavailable aggregator does.
    pub fn set_down(&self, down: bool) {
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .down = down;
    }

    pub fn pulls(&self) -> Vec<(String, CalendarDate, CalendarDate)> {
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .pulls
            .clone()
    }

    pub fn disconnected(&self) -> Vec<String> {
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .disconnected
            .clone()
    }

    fn check_up(&self) -> Result<(), AggregatorError> {
        if self
            .world
            .lock()
            .expect("fake aggregator lock poisoned")
            .down
        {
            return Err(AggregatorError::Unavailable(
                "the fake aggregator is down".to_owned(),
            ));
        }
        Ok(())
    }
}

impl BankAggregator for FakeBankAggregator {
    fn provider(&self) -> &'static str {
        "fake"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn connect_token(
        &self,
        company_id: CompanyId,
        item: Option<&str>,
    ) -> Result<String, AggregatorError> {
        self.check_up()?;
        Ok(format!(
            "fake-token-{}-{}",
            company_id.as_uuid(),
            item.unwrap_or("new")
        ))
    }

    async fn accounts(&self, item: &str) -> Result<Vec<AggregatedAccount>, AggregatorError> {
        self.check_up()?;
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .accounts
            .get(item)
            .cloned()
            .ok_or_else(|| AggregatorError::Refused("no such item".to_owned()))
    }

    async fn item_state(&self, item: &str) -> Result<ItemState, AggregatorError> {
        self.check_up()?;
        if self
            .world
            .lock()
            .expect("fake aggregator lock poisoned")
            .refused
        {
            return Err(AggregatorError::Refused("no longer accepted".to_owned()));
        }
        Ok(self
            .world
            .lock()
            .expect("fake aggregator lock poisoned")
            .states
            .get(item)
            .cloned()
            .unwrap_or(ItemState::Ready))
    }

    async fn transactions(
        &self,
        account: &str,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<Vec<AggregatedTransaction>, AggregatorError> {
        self.check_up()?;
        let mut world = self.world.lock().expect("fake aggregator lock poisoned");
        world.pulls.push((account.to_owned(), from, to));
        Ok(world
            .transactions
            .get(account)
            .into_iter()
            .flatten()
            .filter(|transaction| (from..=to).contains(&transaction.posted_on))
            .cloned()
            .collect())
    }

    async fn disconnect(&self, item: &str) -> Result<(), AggregatorError> {
        self.check_up()?;
        self.world
            .lock()
            .expect("fake aggregator lock poisoned")
            .disconnected
            .push(item.to_owned());
        Ok(())
    }
}
