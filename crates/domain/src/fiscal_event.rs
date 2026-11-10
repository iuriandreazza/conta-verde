use crate::{
    BlobId, CompanyId, DomainError, FiscalDocumentId, FiscalEnvironment, FiscalEventId,
    FiscalRefusal, UserId,
};

const MIN_TEXT_CHARS: usize = 15;
const MAX_CORRECTION_CHARS: usize = 1000;
const MAX_REASON_CHARS: usize = 255;
/// The authority takes at most this many letters for one document.
pub const MAX_CORRECTIONS: u8 = 20;
const MAX_NUMBER: u32 = 999_999_999;

/// What an event of the fiscal documents is about ([ADR 0076]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiscalEventSubject {
    /// A correction letter (CC-e) of an authorized NF-e: its place among that document's letters and its text.
    Correction {
        document: FiscalDocumentId,
        sequence: u8,
        text: String,
    },
    /// A range of numbers of a series declared unused (inutilização), in one environment.
    Invalidation {
        series: u16,
        first: u32,
        last: u32,
        environment: FiscalEnvironment,
        reason: String,
    },
}

impl FiscalEventSubject {
    pub fn correction(
        document: FiscalDocumentId,
        sequence: u8,
        text: &str,
    ) -> Result<Self, DomainError> {
        if sequence == 0 || sequence > MAX_CORRECTIONS {
            return Err(DomainError::InvalidFiscalDocument("sequence"));
        }
        Ok(Self::Correction {
            document,
            sequence,
            text: one_line(text, MAX_CORRECTION_CHARS, "text")?,
        })
    }

    pub fn invalidation(
        series: u16,
        first: u32,
        last: u32,
        environment: FiscalEnvironment,
        reason: &str,
    ) -> Result<Self, DomainError> {
        if series == 0 || series > 999 || first == 0 || last < first || last > MAX_NUMBER {
            return Err(DomainError::InvalidFiscalDocument("range"));
        }
        Ok(Self::Invalidation {
            series,
            first,
            last,
            environment,
            reason: one_line(reason, MAX_REASON_CHARS, "reason")?,
        })
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::Correction { .. } => "correction",
            Self::Invalidation { .. } => "invalidation",
        }
    }

    /// The text the person wrote: the letter's, or the reason of the invalidation.
    pub fn body(&self) -> &str {
        match self {
            Self::Correction { text, .. } => text,
            Self::Invalidation { reason, .. } => reason,
        }
    }
}

fn one_line(raw: &str, max: usize, field: &'static str) -> Result<String, DomainError> {
    let text = raw.trim();
    let length = text.chars().count();
    if (MIN_TEXT_CHARS..=max).contains(&length) && !text.chars().any(char::is_control) {
        Ok(text.to_owned())
    } else {
        Err(DomainError::InvalidFiscalDocument(field))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiscalEventStatus {
    /// Recorded and sent: the answer is not known yet, or was lost. It is never sent again by the product.
    Sent,
    Accepted,
    Refused,
}

impl FiscalEventStatus {
    pub fn key(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Accepted => "accepted",
            Self::Refused => "refused",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "sent" => Some(Self::Sent),
            "accepted" => Some(Self::Accepted),
            "refused" => Some(Self::Refused),
            _ => None,
        }
    }
}

/// An event of the fiscal documents: **recorded before the gateway is asked**, then settled once with what the
/// authority said. Never edited or deleted otherwise ([ADR 0076]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalEvent {
    id: FiscalEventId,
    company_id: CompanyId,
    subject: FiscalEventSubject,
    requested_by: UserId,
    requested_by_name: String,
    status: FiscalEventStatus,
    protocol: Option<String>,
    refusal: Option<FiscalRefusal>,
    xml: Option<(BlobId, u64)>,
}

impl FiscalEvent {
    pub fn new(
        id: FiscalEventId,
        company_id: CompanyId,
        subject: FiscalEventSubject,
        requested_by: UserId,
        requested_by_name: &str,
    ) -> Result<Self, DomainError> {
        let name = requested_by_name.trim();
        if name.is_empty() || name.chars().count() > 150 || name.chars().any(char::is_control) {
            return Err(DomainError::InvalidFiscalDocument("requested_by_name"));
        }
        Ok(Self {
            id,
            company_id,
            subject,
            requested_by,
            requested_by_name: name.to_owned(),
            status: FiscalEventStatus::Sent,
            protocol: None,
            refusal: None,
            xml: None,
        })
    }

    /// The event as it was stored.
    pub fn restored(
        mut self,
        status: FiscalEventStatus,
        protocol: Option<String>,
        refusal: Option<FiscalRefusal>,
        xml: Option<(BlobId, u64)>,
    ) -> Self {
        self.status = status;
        self.protocol = protocol;
        self.refusal = refusal;
        self.xml = xml;
        self
    }

    /// The authority took it. `xml` is the stored file of the event, when the gateway gave one.
    pub fn accepted(
        &self,
        protocol: Option<&str>,
        xml: Option<(BlobId, u64)>,
    ) -> Result<Self, DomainError> {
        self.settled(FiscalEventStatus::Accepted, |event| {
            event.protocol = protocol
                .map(|raw| FiscalRefusal::new(raw, "").code().to_owned())
                .filter(|protocol| !protocol.is_empty());
            event.xml = xml.filter(|(_, size)| *size > 0);
        })
    }

    pub fn refused(&self, refusal: FiscalRefusal) -> Result<Self, DomainError> {
        self.settled(FiscalEventStatus::Refused, |event| {
            event.refusal = Some(refusal);
        })
    }

    fn settled(
        &self,
        status: FiscalEventStatus,
        change: impl FnOnce(&mut Self),
    ) -> Result<Self, DomainError> {
        if self.status != FiscalEventStatus::Sent {
            return Err(DomainError::InvalidFiscalTransition);
        }
        let mut event = self.clone();
        event.status = status;
        change(&mut event);
        Ok(event)
    }

    pub fn id(&self) -> FiscalEventId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn subject(&self) -> &FiscalEventSubject {
        &self.subject
    }

    pub fn requested_by(&self) -> UserId {
        self.requested_by
    }

    pub fn requested_by_name(&self) -> &str {
        &self.requested_by_name
    }

    pub fn status(&self) -> FiscalEventStatus {
        self.status
    }

    pub fn protocol(&self) -> Option<&str> {
        self.protocol.as_deref()
    }

    pub fn refusal(&self) -> Option<&FiscalRefusal> {
        self.refusal.as_ref()
    }

    pub fn xml(&self) -> Option<(BlobId, u64)> {
        self.xml
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn correction(sequence: u8) -> FiscalEventSubject {
        FiscalEventSubject::correction(
            FiscalDocumentId::generate(),
            sequence,
            "Corrige o endereço de entrega do destinatário",
        )
        .unwrap()
    }

    fn event(subject: FiscalEventSubject) -> FiscalEvent {
        FiscalEvent::new(
            FiscalEventId::generate(),
            CompanyId::generate(),
            subject,
            UserId::generate(),
            "Maria",
        )
        .unwrap()
    }

    #[test]
    fn a_correction_has_a_place_from_1_to_20_and_a_text_of_15_to_1000_characters_on_one_line() {
        let document = FiscalDocumentId::generate();
        let text = "Corrige o endereço de entrega";
        assert!(FiscalEventSubject::correction(document, 1, text).is_ok());
        assert!(FiscalEventSubject::correction(document, 20, text).is_ok());
        assert!(FiscalEventSubject::correction(document, 0, text).is_err());
        assert!(FiscalEventSubject::correction(document, 21, text).is_err());
        assert!(FiscalEventSubject::correction(document, 1, "curto").is_err());
        assert!(FiscalEventSubject::correction(document, 1, &"a".repeat(1001)).is_err());
        assert!(FiscalEventSubject::correction(document, 1, &format!("{text}\nlinha")).is_err());
    }

    #[test]
    fn an_invalidation_is_a_range_of_a_series_with_a_reason() {
        let environment = FiscalEnvironment::Homologation;
        let reason = "Numeração pulada por falha do sistema";
        assert!(FiscalEventSubject::invalidation(1, 10, 10, environment, reason).is_ok());
        assert!(FiscalEventSubject::invalidation(1, 10, 9, environment, reason).is_err());
        assert!(FiscalEventSubject::invalidation(0, 1, 1, environment, reason).is_err());
        assert!(FiscalEventSubject::invalidation(1000, 1, 1, environment, reason).is_err());
        assert!(FiscalEventSubject::invalidation(1, 0, 1, environment, reason).is_err());
        assert!(
            FiscalEventSubject::invalidation(1, 1, 1_000_000_000, environment, reason).is_err()
        );
        assert!(FiscalEventSubject::invalidation(1, 1, 1, environment, "curto").is_err());
    }

    #[test]
    fn an_event_is_settled_once() {
        let sent = event(correction(1));
        assert_eq!(sent.status(), FiscalEventStatus::Sent);
        let accepted = sent
            .accepted(Some(" 135260000000009 "), Some((BlobId::generate(), 10)))
            .unwrap();
        assert_eq!(accepted.protocol(), Some("135260000000009"));
        assert!(accepted.xml().is_some());
        assert_eq!(
            accepted.refused(FiscalRefusal::new("1", "x")),
            Err(DomainError::InvalidFiscalTransition)
        );
        let refused = sent
            .refused(FiscalRefusal::new("494", "Chave inexistente"))
            .unwrap();
        assert_eq!(refused.refusal().unwrap().code(), "494");
        assert_eq!(
            refused.accepted(None, None),
            Err(DomainError::InvalidFiscalTransition)
        );
    }
}
