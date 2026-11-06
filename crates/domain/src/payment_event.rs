use crate::{ChargeId, CompanyId, DomainError, PaymentEventId};

const MAX_EVENT_ID_CHARS: usize = 200;
const MAX_NOTE_CHARS: usize = 200;
/// The most a provider's notice can weigh: it is kept as it came.
pub const MAX_EVENT_BYTES: usize = 64 * 1024;

/// What a provider told us about a charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentEventKind {
    Paid,
    Cancelled,
    Expired,
}

impl PaymentEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paid => "paid",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "paid" => Some(Self::Paid),
            "cancelled" => Some(Self::Cancelled),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

/// What came of an event: not looked at yet (or looked at and interrupted), applied, or kept without
/// being applied because it no longer fits (the person has to look at it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentEventOutcome {
    Pending,
    Applied,
    NotApplied,
}

impl PaymentEventOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Applied => "applied",
            Self::NotApplied => "not_applied",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(Self::Pending),
            "applied" => Some(Self::Applied),
            "not_applied" => Some(Self::NotApplied),
            _ => None,
        }
    }
}

/// A note as an event keeps it: cut to a short line, and nothing when nothing is left.
pub fn event_note(note: Option<&str>) -> Option<String> {
    note.map(|text| {
        text.chars()
            .filter(|c| !c.is_control())
            .take(MAX_NOTE_CHARS)
            .collect::<String>()
    })
    .filter(|text| !text.is_empty())
}

/// A notice from a payment provider about a charge, kept as it came ([ADR 0046]) under the provider's own
/// id for it, so the same notice twice is one. It is a log and not a register: it is not audited and
/// not edited, only finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentEvent {
    id: PaymentEventId,
    company_id: CompanyId,
    charge_id: ChargeId,
    provider: String,
    event_id: String,
    kind: PaymentEventKind,
    outcome: PaymentEventOutcome,
    note: Option<String>,
    raw: Vec<u8>,
}

impl PaymentEvent {
    /// A new event, pending. The provider's key and id and the size of the notice are checked here; that
    /// the charge is the company's is the use case's.
    pub fn new(
        id: PaymentEventId,
        company_id: CompanyId,
        charge_id: ChargeId,
        provider: &str,
        event_id: &str,
        kind: PaymentEventKind,
        raw: Vec<u8>,
    ) -> Result<Self, DomainError> {
        let event_id = event_id.trim();
        if event_id.is_empty()
            || event_id.chars().count() > MAX_EVENT_ID_CHARS
            || event_id.chars().any(|c| c.is_control())
            || provider.is_empty()
            || raw.len() > MAX_EVENT_BYTES
        {
            return Err(DomainError::InvalidPaymentEvent);
        }
        Ok(Self {
            id,
            company_id,
            charge_id,
            provider: provider.to_owned(),
            event_id: event_id.to_owned(),
            kind,
            outcome: PaymentEventOutcome::Pending,
            note: None,
            raw,
        })
    }

    /// The same event with what came of it: what storage hands back, and what finishing it makes.
    pub fn with_outcome(mut self, outcome: PaymentEventOutcome, note: Option<&str>) -> Self {
        self.outcome = outcome;
        self.note = event_note(note);
        self
    }

    pub fn id(&self) -> PaymentEventId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn charge_id(&self) -> ChargeId {
        self.charge_id
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    pub fn kind(&self) -> PaymentEventKind {
        self.kind
    }

    pub fn outcome(&self) -> PaymentEventOutcome {
        self.outcome
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(event_id: &str, raw: Vec<u8>) -> Result<PaymentEvent, DomainError> {
        PaymentEvent::new(
            PaymentEventId::generate(),
            CompanyId::generate(),
            ChargeId::generate(),
            "fake",
            event_id,
            PaymentEventKind::Paid,
            raw,
        )
    }

    #[test]
    fn a_new_event_is_pending_and_finishes_with_a_short_note() {
        let event = build(" evt_1 ", b"{}".to_vec()).unwrap();
        assert_eq!(event.event_id(), "evt_1");
        assert_eq!(event.outcome(), PaymentEventOutcome::Pending);
        let done = event.with_outcome(PaymentEventOutcome::NotApplied, Some(&"a".repeat(300)));
        assert_eq!(done.note().unwrap().chars().count(), 200);
        let clean = done.with_outcome(PaymentEventOutcome::Applied, Some(""));
        assert_eq!(clean.note(), None);
    }

    #[test]
    fn refuses_an_id_that_is_not_one_line_and_a_notice_too_big() {
        for id in ["", "  ", "a\nb"] {
            assert_eq!(build(id, vec![]), Err(DomainError::InvalidPaymentEvent));
        }
        assert_eq!(
            build("e", vec![0; MAX_EVENT_BYTES + 1]),
            Err(DomainError::InvalidPaymentEvent)
        );
        assert!(build("e", vec![0; MAX_EVENT_BYTES]).is_ok());
    }

    #[test]
    fn names_round_trip() {
        for kind in [
            PaymentEventKind::Paid,
            PaymentEventKind::Cancelled,
            PaymentEventKind::Expired,
        ] {
            assert_eq!(PaymentEventKind::parse(kind.as_str()), Some(kind));
        }
        for outcome in [
            PaymentEventOutcome::Pending,
            PaymentEventOutcome::Applied,
            PaymentEventOutcome::NotApplied,
        ] {
            assert_eq!(PaymentEventOutcome::parse(outcome.as_str()), Some(outcome));
        }
    }
}
