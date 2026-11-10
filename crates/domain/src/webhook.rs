use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use uuid::Uuid;

use crate::{Actor, CompanyId, DomainError, WebhookDeliveryId, WebhookSubscriptionId};

/// How many subscriptions a company may have ([ADR 0088]).
pub const MAX_WEBHOOK_SUBSCRIPTIONS: usize = 20;
/// The longest address a subscription may have.
pub const MAX_WEBHOOK_URL_CHARS: usize = 2000;
/// How long the previous secret goes on signing after it was replaced, in milliseconds (24 hours).
pub const SECRET_GRACE_MILLIS: i64 = 24 * 3_600_000;
/// What is kept of why a delivery failed: a short reason, never the subscriber's answer.
pub const MAX_DELIVERY_ERROR_CHARS: usize = 200;

/// What happened that a subscriber may be told about. The text is what the API sends, what a subscription lists and what the
/// database stores; it does not change, and the list only grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebhookEventKind {
    SaleCreated,
    SaleCompleted,
    SaleCancelled,
    ChargePaid,
    StockBelowMinimum,
    /// Sent by the person who asks for it, to see that a subscription works. Never subscribed to.
    Test,
}

impl WebhookEventKind {
    /// What a subscription may ask for, in the order the page lists them.
    pub const SUBSCRIBABLE: [WebhookEventKind; 5] = [
        Self::SaleCreated,
        Self::SaleCompleted,
        Self::SaleCancelled,
        Self::ChargePaid,
        Self::StockBelowMinimum,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SaleCreated => "sale.created",
            Self::SaleCompleted => "sale.completed",
            Self::SaleCancelled => "sale.cancelled",
            Self::ChargePaid => "charge.paid",
            Self::StockBelowMinimum => "stock.below_minimum",
            Self::Test => "webhook.test",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::SUBSCRIBABLE
            .into_iter()
            .chain([Self::Test])
            .find(|kind| kind.as_str() == raw)
    }
}

/// Unvalidated input for making or changing a subscription.
#[derive(Debug, Clone)]
pub struct WebhookSubscriptionDraft {
    pub url: String,
    /// The events asked for; none means every one.
    pub events: Vec<String>,
    pub active: bool,
}

/// Where a company wants to be told what happens, and what. The signing secret is not part of it: it is sealed and kept apart,
/// and nothing here can print it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookSubscription {
    id: WebhookSubscriptionId,
    company_id: CompanyId,
    url: String,
    /// Sorted, once each; empty is every event.
    events: Vec<WebhookEventKind>,
    active: bool,
    created_by: Actor,
    /// Unix milliseconds.
    created_at: i64,
}

impl WebhookSubscription {
    /// `allow_private_targets` is the operator's development and test opt-in ([ADR 0088]): it allows `http` and an address that
    /// is not public.
    pub fn new(
        id: WebhookSubscriptionId,
        company_id: CompanyId,
        draft: WebhookSubscriptionDraft,
        allow_private_targets: bool,
        created_by: Actor,
        now_millis: i64,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            company_id,
            url: clean_url(&draft.url, allow_private_targets)?,
            events: clean_events(&draft.events)?,
            active: draft.active,
            created_by,
            created_at: now_millis,
        })
    }

    /// Whether an address could be the one of a subscription, without making one.
    pub fn check_address(raw: &str, allow_private_targets: bool) -> Result<(), DomainError> {
        clean_url(raw, allow_private_targets).map(|_| ())
    }

    /// The same subscription, off. Its address is not looked at again: an address that was fine when it was made (before the
    /// operator took the development opt-in away) must not keep it from being switched off.
    pub fn switched_off(&self) -> Self {
        Self {
            active: false,
            ..self.clone()
        }
    }

    /// The same subscription with its address, its events and whether it is on replaced by the draft's.
    pub fn replaced_with(
        &self,
        draft: WebhookSubscriptionDraft,
        allow_private_targets: bool,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            url: clean_url(&draft.url, allow_private_targets)?,
            events: clean_events(&draft.events)?,
            active: draft.active,
            ..self.clone()
        })
    }

    /// The subscription as it was stored.
    pub fn restored(
        id: WebhookSubscriptionId,
        company_id: CompanyId,
        url: String,
        events: Vec<WebhookEventKind>,
        active: bool,
        created_by: Actor,
        created_at: i64,
    ) -> Self {
        Self {
            id,
            company_id,
            url,
            events,
            active,
            created_by,
            created_at,
        }
    }

    /// Whether this subscription wants to be told of `kind`.
    pub fn wants(&self, kind: WebhookEventKind) -> bool {
        self.active
            && (kind == WebhookEventKind::Test
                || self.events.is_empty()
                || self.events.contains(&kind))
    }

    pub fn id(&self) -> WebhookSubscriptionId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn events(&self) -> &[WebhookEventKind] {
        &self.events
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn created_by(&self) -> &Actor {
        &self.created_by
    }

    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    /// The events as the trail writes them.
    pub fn events_text(&self) -> String {
        if self.events.is_empty() {
            "all".to_owned()
        } else {
            self.events
                .iter()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>()
                .join(",")
        }
    }
}

fn clean_events(raw: &[String]) -> Result<Vec<WebhookEventKind>, DomainError> {
    let mut events = raw
        .iter()
        .map(|text| {
            WebhookEventKind::parse(text)
                .filter(|kind| *kind != WebhookEventKind::Test)
                .ok_or(DomainError::InvalidWebhookEvent)
        })
        .collect::<Result<Vec<_>, _>>()?;
    events.sort_by_key(|kind| kind.as_str());
    events.dedup();
    Ok(events)
}

/// An address a subscriber is told at: `https`, a host that is a name or an address that is public, an optional port, and no
/// credentials in it. `http` and a private address are only allowed by the operator's opt-in, for a listener on this machine.
fn clean_url(raw: &str, allow_private_targets: bool) -> Result<String, DomainError> {
    let url = raw.trim();
    let refused = || DomainError::InvalidWebhookUrl;
    if url.is_empty()
        || url.chars().count() > MAX_WEBHOOK_URL_CHARS
        || url.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(refused());
    }
    let rest = match url.split_once("://") {
        Some(("https", rest)) => rest,
        Some(("http", rest)) if allow_private_targets => rest,
        _ => return Err(refused()),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return Err(refused());
    }
    let (host, port) = split_host_port(authority).ok_or_else(refused)?;
    if let Some(port) = port
        && port.parse::<u16>().ok().filter(|port| *port > 0).is_none()
    {
        return Err(refused());
    }
    if !allow_private_targets {
        match host.parse::<IpAddr>() {
            Ok(address) if is_blocked_address(address) => return Err(refused()),
            Ok(_) => {}
            Err(_) => {
                // A trailing dot names the same host.
                let lower = host.trim_end_matches('.').to_ascii_lowercase();
                let is_name = !lower.is_empty()
                    && lower
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
                if !is_name || lower == "localhost" || lower.ends_with(".localhost") {
                    return Err(refused());
                }
            }
        }
    }
    Ok(url.to_owned())
}

/// `host`, `host:port`, `[v6]` and `[v6]:port`, with the brackets taken off a v6.
pub fn split_host_port(authority: &str) -> Option<(&str, Option<&str>)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        return match after {
            "" => Some((host, None)),
            _ => Some((host, Some(after.strip_prefix(':')?))),
        };
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => Some((host, Some(port))),
        Some(_) => None,
        None => Some((authority, None)),
    }
}

/// Whether a subscriber must **not** be reached at this address: it is not on the public internet, so a subscription pointed
/// at it would make the server call the operator's own network ([ADR 0088]). An IPv4 address inside an IPv6 one is judged as
/// the IPv4 it is.
pub fn is_blocked_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_blocked_v4(v4),
            None => is_blocked_v6(v6),
        },
    }
}

fn is_blocked_v4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    address.is_loopback()
        || a == 0
        || address.is_private()
        || address.is_link_local()
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..=19).contains(&b))
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 240
        || address.is_multicast()
}

fn is_blocked_v6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    // `::a.b.c.d` (IPv4-compatible, deprecated) carries an IPv4 in its last 32 bits: it is judged as that one.
    if segments[..6].iter().all(|segment| *segment == 0)
        && !address.is_unspecified()
        && !address.is_loopback()
    {
        let [high, low] = [segments[6], segments[7]];
        return is_blocked_v4(Ipv4Addr::new(
            (high >> 8) as u8,
            high as u8,
            (low >> 8) as u8,
            low as u8,
        ));
    }
    address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        // Unique local fc00::/7 and link-local fe80::/10.
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        // Documentation 2001:db8::/32, NAT64 64:ff9b::/96 and 6to4 2002::/16.
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || (segments[0] == 0x0064 && segments[1] == 0xff9b)
        || segments[0] == 0x2002
        // Site-local fec0::/10, discard 100::/64, Teredo 2001::/32 and the SIIT range ::ffff:0:0:0/96.
        || (segments[0] & 0xffc0) == 0xfec0
        || (segments[0] == 0x0100 && segments[1..4].iter().all(|segment| *segment == 0))
        || (segments[0] == 0x2001 && segments[1] == 0)
        || (segments[..4].iter().all(|segment| *segment == 0) && segments[4] == 0xffff && segments[5] == 0)
}

/// Where a delivery stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    /// Waiting for a try, or for the next one.
    Pending,
    Delivered,
    /// Given up on after the last try.
    Failed,
}

impl DeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Pending, Self::Delivered, Self::Failed]
            .into_iter()
            .find(|status| status.as_str() == raw)
    }
}

/// One event, for one subscription: the body that is sent, how many times it was tried and what the last try said. The body
/// is the company's own data and is kept for 30 days; what a subscriber answered is never kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookDelivery {
    pub id: WebhookDeliveryId,
    pub company_id: CompanyId,
    pub subscription_id: WebhookSubscriptionId,
    /// The event's id, the same for every subscription and every try.
    pub event_id: Uuid,
    pub kind: WebhookEventKind,
    pub payload: String,
    pub status: DeliveryStatus,
    pub attempts: u32,
    /// The status code of the last try, when the subscriber answered.
    pub last_status: Option<u16>,
    /// Why the last try failed, in a few words.
    pub last_error: Option<String>,
    /// Unix milliseconds.
    pub created_at: i64,
    pub updated_at: i64,
    pub delivered_at: Option<i64>,
}

/// A short reason, on one line: what is kept of a failure is never longer than this.
pub fn delivery_error(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_DELIVERY_ERROR_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserId;

    fn actor() -> Actor {
        Actor::User {
            id: UserId::generate(),
            name: "Ana".into(),
        }
    }

    fn make(url: &str, events: &[&str], allow: bool) -> Result<WebhookSubscription, DomainError> {
        WebhookSubscription::new(
            WebhookSubscriptionId::generate(),
            CompanyId::generate(),
            WebhookSubscriptionDraft {
                url: url.into(),
                events: events.iter().map(|e| (*e).to_owned()).collect(),
                active: true,
            },
            allow,
            actor(),
            0,
        )
    }

    #[test]
    fn an_address_is_https_with_a_public_host_and_no_credentials() {
        for good in [
            "https://example.com/hooks",
            "https://hooks.example.com:8443/a?b=c",
            "https://8.8.8.8/x",
            "https://[2606:4700::1111]/x",
        ] {
            assert!(make(good, &[], false).is_ok(), "{good}");
        }
        for bad in [
            "",
            "http://example.com/x",
            "ftp://example.com/x",
            "example.com/x",
            "https://",
            "https://user:pass@example.com/x",
            "https://example.com:0/x",
            "https://example.com:99999/x",
            "https://exa mple.com/x",
            "https://localhost/x",
            "https://a.localhost/x",
            "https://localhost./x",
            "https://127.0.0.1/x",
            "https://10.1.2.3/x",
            "https://169.254.169.254/latest/meta-data",
            "https://[::1]/x",
            "https://[::ffff:127.0.0.1]/x",
            "https://[fd00::1]/x",
            "https://bad_host!/x",
        ] {
            assert_eq!(
                make(bad, &[], false).unwrap_err(),
                DomainError::InvalidWebhookUrl,
                "{bad}"
            );
        }
        // The operator's opt-in, for a listener on this machine, allows http and a private address.
        assert!(make("http://127.0.0.1:9000/hook", &[], true).is_ok());
        assert!(make("http://localhost:9000/hook", &[], true).is_ok());
    }

    #[test]
    fn every_address_that_is_not_public_is_blocked() {
        let blocked = [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "192.0.0.8",
            "198.18.0.1",
            "198.19.255.255",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "ff02::1",
            "2001:db8::1",
            "64:ff9b::7f00:1",
            "2002:7f00:1::1",
            "::7f00:1",
            "::a00:1",
            "fec0::1",
            "100::1",
            "2001::1",
            "64:ff9b:1::1",
        ];
        for text in blocked {
            assert!(is_blocked_address(text.parse().unwrap()), "{text}");
        }
        for text in [
            "8.8.8.8",
            "1.1.1.1",
            "172.32.0.1",
            "100.128.0.1",
            "198.20.0.1",
            "2606:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(!is_blocked_address(text.parse().unwrap()), "{text}");
        }
    }

    #[test]
    fn the_events_are_known_sorted_once_each_and_never_the_test() {
        let sub = make(
            "https://example.com/x",
            &["sale.completed", "charge.paid", "sale.completed"],
            false,
        )
        .unwrap();
        assert_eq!(sub.events_text(), "charge.paid,sale.completed");
        assert_eq!(
            make("https://example.com/x", &[], false)
                .unwrap()
                .events_text(),
            "all"
        );
        for bad in ["sale.sold", "webhook.test", ""] {
            assert_eq!(
                make("https://example.com/x", &[bad], false).unwrap_err(),
                DomainError::InvalidWebhookEvent,
                "{bad}"
            );
        }
    }

    #[test]
    fn a_subscription_wants_what_it_asked_for_while_it_is_on_and_always_the_test() {
        let mut draft = WebhookSubscriptionDraft {
            url: "https://example.com/x".into(),
            events: vec!["sale.created".into()],
            active: true,
        };
        let sub = make("https://example.com/x", &["sale.created"], false).unwrap();
        assert!(sub.wants(WebhookEventKind::SaleCreated));
        assert!(!sub.wants(WebhookEventKind::SaleCancelled));
        assert!(sub.wants(WebhookEventKind::Test));
        draft.active = false;
        let off = sub.replaced_with(draft, false).unwrap();
        assert!(!off.wants(WebhookEventKind::SaleCreated));
        assert!(!off.wants(WebhookEventKind::Test));
        assert!(
            make("https://example.com/x", &[], false)
                .unwrap()
                .wants(WebhookEventKind::ChargePaid)
        );
    }

    #[test]
    fn a_failure_is_kept_as_one_short_line() {
        assert_eq!(delivery_error("a\nb\tc"), "a b c");
        assert_eq!(
            delivery_error(&"x".repeat(500)).chars().count(),
            MAX_DELIVERY_ERROR_CHARS
        );
    }
}
