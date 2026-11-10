use crate::{AccessKey, Amount, CalendarDate, CompanyId, DomainError, ReceivedInvoiceId};

const MAX_TEXT_CHARS: usize = 150;
const MIN_JUSTIFICATION_CHARS: usize = 15;
const MAX_JUSTIFICATION_CHARS: usize = 255;

/// What the authority says of an invoice that was issued against the company ([ADR 0083]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceivedSituation {
    Authorized,
    Cancelled,
    Denied,
}

impl ReceivedSituation {
    /// The key the API sends and the database stores; it does not change.
    pub fn key(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::Cancelled => "cancelled",
            Self::Denied => "denied",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "authorized" => Some(Self::Authorized),
            "cancelled" => Some(Self::Cancelled),
            "denied" => Some(Self::Denied),
            _ => None,
        }
    }
}

/// What the recipient has declared to the authority about an invoice (*manifestação do destinatário*): awareness, then at most
/// one final word: confirmation, "unknown" or "operation not performed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manifestation {
    None,
    Awareness,
    Confirmation,
    Unknown,
    NotPerformed,
}

impl Manifestation {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Awareness,
        Self::Confirmation,
        Self::Unknown,
        Self::NotPerformed,
    ];

    /// The key the API sends and the database stores; it does not change.
    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Awareness => "awareness",
            Self::Confirmation => "confirmation",
            Self::Unknown => "unknown",
            Self::NotPerformed => "not_performed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == raw)
    }

    /// How far along the declarations this word is: nothing, awareness, a final word. What the gateway lists is never taken
    /// to be behind what the company already holds ([ADR 0083]).
    pub fn rank(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Awareness => 1,
            _ => 2,
        }
    }

    /// Whether this is the last word: nothing is declared after it.
    pub fn is_final(self) -> bool {
        matches!(
            self,
            Self::Confirmation | Self::Unknown | Self::NotPerformed
        )
    }

    /// Whether the recipient may declare `next` from here: awareness from nothing, a final word from nothing or from awareness.
    pub fn may_become(self, next: Self) -> bool {
        match (self, next) {
            (Self::None, Self::Awareness) => true,
            (Self::None | Self::Awareness, next) => next.is_final(),
            _ => false,
        }
    }
}

/// An invoice as the gateway lists it, before it is ours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedInvoiceDraft {
    pub access_key: AccessKey,
    pub issuer_name: String,
    /// The issuer's CPF or CNPJ, without punctuation.
    pub issuer_document: String,
    pub total: Amount,
    pub issued_on: CalendarDate,
    pub situation: ReceivedSituation,
    pub manifestation: Manifestation,
    /// The gateway's version of the record: what a later listing asks "newer than" about.
    pub version: u64,
}

/// An invoice issued against the company by another, as the gateway told it: a **mirror** of the gateway's list that the
/// company may answer to the authority. What the mirror says is the gateway's; what is recorded as the person's act is the
/// manifestation ([ADR 0083]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedInvoice {
    id: ReceivedInvoiceId,
    company_id: CompanyId,
    content: ReceivedInvoiceDraft,
    /// Why the recipient said "operation not performed", the only word that has a reason.
    justification: Option<String>,
}

impl ReceivedInvoice {
    pub fn new(
        id: ReceivedInvoiceId,
        company_id: CompanyId,
        draft: ReceivedInvoiceDraft,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            company_id,
            content: validated(draft)?,
            justification: None,
        })
    }

    /// The invoice with the justification it was stored with.
    pub fn restored(mut self, justification: Option<String>) -> Self {
        self.justification = justification;
        self
    }

    /// The same invoice as the gateway says it now (its situation, its manifestation, its version): the id, the company
    /// and the key stay.
    pub fn mirrored(&self, draft: ReceivedInvoiceDraft) -> Result<Self, DomainError> {
        if draft.access_key != self.content.access_key {
            return Err(DomainError::InvalidReceivedInvoice("access_key"));
        }
        Ok(Self {
            content: validated(draft)?,
            ..self.clone()
        })
    }

    /// The recipient's declaration to the authority. Only an authorized invoice is answered; the word must follow the
    /// one before it; "operation not performed" needs a reason of 15 to 255 characters on one line, and no other word has one.
    pub fn manifested(
        &self,
        next: Manifestation,
        justification: Option<&str>,
    ) -> Result<Self, DomainError> {
        if self.content.situation != ReceivedSituation::Authorized {
            return Err(DomainError::InvalidReceivedInvoice("situation"));
        }
        if !self.content.manifestation.may_become(next) {
            return Err(DomainError::InvalidReceivedInvoice("manifestation"));
        }
        let justification = justification.map(str::trim).filter(|text| !text.is_empty());
        let justification = match (next, justification) {
            (Manifestation::NotPerformed, Some(text))
                if (MIN_JUSTIFICATION_CHARS..=MAX_JUSTIFICATION_CHARS)
                    .contains(&text.chars().count())
                    && !text.chars().any(char::is_control) =>
            {
                Some(text.to_owned())
            }
            (Manifestation::NotPerformed, _) => {
                return Err(DomainError::InvalidReceivedInvoice("justification"));
            }
            (_, None) => None,
            (_, Some(_)) => return Err(DomainError::InvalidReceivedInvoice("justification")),
        };
        let mut content = self.content.clone();
        content.manifestation = next;
        Ok(Self {
            content,
            justification,
            ..self.clone()
        })
    }

    /// The invoice with the word and the reason an adapter holds for it, **without the rules of the words**: what is stored is
    /// what the authority took, and the rules were applied when it was declared.
    pub fn with_manifestation(&self, word: Manifestation, justification: Option<String>) -> Self {
        let mut content = self.content.clone();
        content.manifestation = word;
        Self {
            content,
            justification,
            ..self.clone()
        }
    }

    pub fn id(&self) -> ReceivedInvoiceId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn access_key(&self) -> &AccessKey {
        &self.content.access_key
    }

    pub fn issuer_name(&self) -> &str {
        &self.content.issuer_name
    }

    pub fn issuer_document(&self) -> &str {
        &self.content.issuer_document
    }

    pub fn total(&self) -> Amount {
        self.content.total
    }

    pub fn issued_on(&self) -> CalendarDate {
        self.content.issued_on
    }

    pub fn situation(&self) -> ReceivedSituation {
        self.content.situation
    }

    pub fn manifestation(&self) -> Manifestation {
        self.content.manifestation
    }

    pub fn version(&self) -> u64 {
        self.content.version
    }

    pub fn justification(&self) -> Option<&str> {
        self.justification.as_deref()
    }
}

fn validated(mut draft: ReceivedInvoiceDraft) -> Result<ReceivedInvoiceDraft, DomainError> {
    let name = draft.issuer_name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_TEXT_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(DomainError::InvalidReceivedInvoice("issuer_name"));
    }
    draft.issuer_name = name.to_owned();
    let document_valid = matches!(draft.issuer_document.len(), 11 | 14)
        && draft
            .issuer_document
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase());
    if !document_valid {
        return Err(DomainError::InvalidReceivedInvoice("issuer_document"));
    }
    if draft.total.as_decimal().scale() > 2 {
        return Err(DomainError::InvalidReceivedInvoice("total"));
    }
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> ReceivedInvoiceDraft {
        ReceivedInvoiceDraft {
            access_key: AccessKey::from_parts(
                35,
                CalendarDate::parse("2026-11-01").unwrap(),
                "12345678000195",
                55,
                1,
                77,
                1,
                11111111,
            )
            .unwrap(),
            issuer_name: " Fornecedor Ltda ".into(),
            issuer_document: "12345678000195".into(),
            total: Amount::parse("150.50").unwrap(),
            issued_on: CalendarDate::parse("2026-11-01").unwrap(),
            situation: ReceivedSituation::Authorized,
            manifestation: Manifestation::None,
            version: 10,
        }
    }

    fn invoice() -> ReceivedInvoice {
        ReceivedInvoice::new(
            ReceivedInvoiceId::generate(),
            CompanyId::generate(),
            draft(),
        )
        .unwrap()
    }

    #[test]
    fn awareness_comes_first_and_then_one_final_word() {
        use Manifestation::*;
        assert!(None.may_become(Awareness));
        for last in [Confirmation, Unknown, NotPerformed] {
            assert!(None.may_become(last), "a final word may be the first");
            assert!(Awareness.may_become(last));
            assert!(!last.may_become(Awareness));
            for other in Manifestation::ALL {
                assert!(!last.may_become(other), "nothing follows the last word");
            }
        }
        assert!(!Awareness.may_become(Awareness));
        assert!(!Awareness.may_become(None));
        assert!(!None.may_become(None));
    }

    #[test]
    fn only_an_authorized_invoice_is_answered_and_only_in_order() {
        let aware = invoice()
            .manifested(Manifestation::Awareness, None)
            .unwrap();
        assert_eq!(aware.manifestation(), Manifestation::Awareness);
        let confirmed = aware.manifested(Manifestation::Confirmation, None).unwrap();
        assert!(confirmed.manifested(Manifestation::Unknown, None).is_err());
        assert!(aware.manifested(Manifestation::Awareness, None).is_err());
        for situation in [ReceivedSituation::Cancelled, ReceivedSituation::Denied] {
            let mut gone = draft();
            gone.situation = situation;
            let gone =
                ReceivedInvoice::new(ReceivedInvoiceId::generate(), CompanyId::generate(), gone)
                    .unwrap();
            assert!(gone.manifested(Manifestation::Awareness, None).is_err());
        }
    }

    #[test]
    fn only_not_performed_has_a_reason_and_it_is_one_line_of_15_to_255_characters() {
        let reason = "A mercadoria nunca chegou ao estabelecimento";
        let said = invoice()
            .manifested(Manifestation::NotPerformed, Some(&format!(" {reason} ")))
            .unwrap();
        assert_eq!(said.justification(), Some(reason));
        for bad in [
            None,
            Some("curta"),
            Some("duas\nlinhas na justificativa aqui"),
        ] {
            assert!(
                invoice()
                    .manifested(Manifestation::NotPerformed, bad)
                    .is_err()
            );
        }
        assert!(
            invoice()
                .manifested(Manifestation::NotPerformed, Some(&"x".repeat(256)))
                .is_err()
        );
        assert!(
            invoice()
                .manifested(Manifestation::Confirmation, Some(reason))
                .is_err(),
            "a confirmation has no reason"
        );
        assert!(
            invoice()
                .manifested(Manifestation::Confirmation, Some("  "))
                .is_ok()
        );
    }

    #[test]
    fn the_words_are_ranked_and_an_adapter_can_hold_a_word_without_the_rules() {
        assert!(Manifestation::None.rank() < Manifestation::Awareness.rank());
        assert!(Manifestation::Awareness.rank() < Manifestation::Confirmation.rank());
        assert_eq!(
            Manifestation::Unknown.rank(),
            Manifestation::NotPerformed.rank()
        );
        let held = invoice().with_manifestation(Manifestation::Confirmation, None);
        assert_eq!(held.manifestation(), Manifestation::Confirmation);
    }

    #[test]
    fn the_mirror_follows_the_gateway_but_never_another_invoice() {
        let stored = invoice();
        let mut newer = draft();
        newer.manifestation = Manifestation::Awareness;
        newer.version = 11;
        let mirrored = stored.mirrored(newer).unwrap();
        assert_eq!(mirrored.id(), stored.id());
        assert_eq!(mirrored.manifestation(), Manifestation::Awareness);
        assert_eq!(mirrored.version(), 11);
        let mut other = draft();
        other.access_key = AccessKey::from_parts(
            35,
            CalendarDate::parse("2026-11-01").unwrap(),
            "12345678000195",
            55,
            1,
            78,
            1,
            11111111,
        )
        .unwrap();
        assert!(stored.mirrored(other).is_err());
    }

    #[test]
    fn what_a_row_could_not_hold_is_refused() {
        assert_eq!(invoice().issuer_name(), "Fornecedor Ltda");
        let mut bad = draft();
        bad.issuer_name = " ".into();
        assert!(
            ReceivedInvoice::new(ReceivedInvoiceId::generate(), CompanyId::generate(), bad)
                .is_err()
        );
        let mut bad = draft();
        bad.issuer_document = "123".into();
        assert!(
            ReceivedInvoice::new(ReceivedInvoiceId::generate(), CompanyId::generate(), bad)
                .is_err()
        );
        assert_eq!(
            Manifestation::parse("awareness"),
            Some(Manifestation::Awareness)
        );
        assert_eq!(
            ReceivedSituation::parse("denied"),
            Some(ReceivedSituation::Denied)
        );
    }
}
