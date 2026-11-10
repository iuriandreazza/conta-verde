use crate::name::clean_name;
use crate::{Actor, ApiKeyId, CompanyId, ConnectorId, DomainError, WebhookSubscriptionId};

/// How many connectors a company may keep, one per store.
pub const MAX_CONNECTORS: usize = 10;
/// A connector's name is the key's name after a prefix, and a key's name is 200 characters at most.
pub const MAX_CONNECTOR_NAME_CHARS: usize = 100;

/// The e-commerce packages the product knows ([ADR 0090]). The list is fixed in the code: a connector is a separate package that
/// speaks only the public API and the webhooks, and a kind here says what the company must give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind {
    WooCommerce,
}

impl ConnectorKind {
    pub const ALL: [Self; 1] = [Self::WooCommerce];

    /// The text the API and the database use; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WooCommerce => "woocommerce",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }

    /// What the person is told of it.
    pub fn title(self) -> &'static str {
        match self {
            Self::WooCommerce => "WooCommerce",
        }
    }

    /// The scopes of the key it speaks with: the least that covers the calls the package makes.
    pub fn scopes(self) -> &'static [&'static str] {
        match self {
            Self::WooCommerce => &["products:read", "sales:write"],
        }
    }

    /// The events its subscription asks for.
    pub fn events(self) -> &'static [&'static str] {
        match self {
            Self::WooCommerce => &["sale.completed", "sale.cancelled"],
        }
    }

    /// Where the package and its instructions are, inside the repository.
    pub fn package_path(self) -> &'static str {
        match self {
            Self::WooCommerce => "connectors/woocommerce",
        }
    }
}

/// A company's use of one connector kind for one store ([ADR 0090]). **It holds no credential of the store** and never calls it:
/// the package runs where the person puts it, with the store's own settings. Turning it on makes only what any integrator could
/// make by hand, an API key and a webhook subscription, and turning it off revokes the key and switches the subscription off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connector {
    id: ConnectorId,
    company_id: CompanyId,
    kind: ConnectorKind,
    name: String,
    active: bool,
    /// The key made when it was turned on; none while it is off.
    api_key_id: Option<ApiKeyId>,
    /// The subscription made the first time it was turned on, switched on and off with it afterwards.
    webhook_subscription_id: Option<WebhookSubscriptionId>,
    created_by: Actor,
    created_at: i64,
}

impl Connector {
    pub fn new(
        id: ConnectorId,
        company_id: CompanyId,
        kind: ConnectorKind,
        name: &str,
        created_by: Actor,
        now_millis: i64,
    ) -> Result<Self, DomainError> {
        let name = clean_name(name)?;
        if name.chars().count() > MAX_CONNECTOR_NAME_CHARS || name.chars().any(char::is_control) {
            return Err(DomainError::InvalidName);
        }
        Ok(Self {
            id,
            company_id,
            kind,
            name,
            active: false,
            api_key_id: None,
            webhook_subscription_id: None,
            created_by,
            created_at: now_millis,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restored(
        id: ConnectorId,
        company_id: CompanyId,
        kind: ConnectorKind,
        name: String,
        active: bool,
        api_key_id: Option<ApiKeyId>,
        webhook_subscription_id: Option<WebhookSubscriptionId>,
        created_by: Actor,
        created_at: i64,
    ) -> Self {
        Self {
            id,
            company_id,
            kind,
            name,
            active,
            api_key_id,
            webhook_subscription_id,
            created_by,
            created_at,
        }
    }

    /// On, with the key it speaks with and the subscription that tells it what happens.
    pub fn turned_on(&self, key: ApiKeyId, subscription: WebhookSubscriptionId) -> Self {
        let mut next = self.clone();
        next.active = true;
        next.api_key_id = Some(key);
        next.webhook_subscription_id = Some(subscription);
        next
    }

    /// Off: the key is gone, the subscription stays (switched off) for the next time.
    pub fn turned_off(&self) -> Self {
        let mut next = self.clone();
        next.active = false;
        next.api_key_id = None;
        next
    }

    pub fn id(&self) -> ConnectorId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn kind(&self) -> ConnectorKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn api_key_id(&self) -> Option<ApiKeyId> {
        self.api_key_id
    }

    pub fn webhook_subscription_id(&self) -> Option<WebhookSubscriptionId> {
        self.webhook_subscription_id
    }

    pub fn created_by(&self) -> &Actor {
        &self.created_by
    }

    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    /// The name of the key made for it, which tells on the keys page what it is for.
    pub fn key_name(&self) -> String {
        format!("Conector {}: {}", self.kind.title(), self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserId;

    fn ana() -> Actor {
        Actor::User {
            id: UserId::generate(),
            name: "Ana".into(),
        }
    }

    fn make(name: &str) -> Result<Connector, DomainError> {
        Connector::new(
            ConnectorId::generate(),
            CompanyId::generate(),
            ConnectorKind::WooCommerce,
            name,
            ana(),
            0,
        )
    }

    #[test]
    fn a_connector_starts_off_with_nothing_made_for_it() {
        let connector = make("  Loja do centro ").unwrap();
        assert_eq!(connector.name(), "Loja do centro");
        assert!(!connector.is_active());
        assert_eq!(
            (connector.api_key_id(), connector.webhook_subscription_id()),
            (None, None)
        );
        for bad in ["", "   ", "a\nb", &"x".repeat(101)] {
            assert!(make(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn turning_it_on_and_off_keeps_the_subscription_and_drops_the_key() {
        let off = make("Loja").unwrap();
        let (key, subscription) = (ApiKeyId::generate(), WebhookSubscriptionId::generate());
        let on = off.turned_on(key, subscription);
        assert!(on.is_active());
        assert_eq!(on.api_key_id(), Some(key));
        let again = on.turned_off();
        assert!(!again.is_active());
        assert_eq!(
            (again.api_key_id(), again.webhook_subscription_id()),
            (None, Some(subscription))
        );
    }

    #[test]
    fn the_key_name_fits_the_keys_limit_for_the_longest_connector_name() {
        let name = "x".repeat(MAX_CONNECTOR_NAME_CHARS);
        assert!(make(&name).unwrap().key_name().chars().count() <= 200);
    }

    #[test]
    fn the_catalogue_asks_for_the_least_it_needs() {
        let kind = ConnectorKind::parse("woocommerce").unwrap();
        assert_eq!(kind.scopes(), ["products:read", "sales:write"]);
        assert_eq!(kind.events(), ["sale.completed", "sale.cancelled"]);
        assert!(ConnectorKind::parse("shopify").is_none());
    }
}
