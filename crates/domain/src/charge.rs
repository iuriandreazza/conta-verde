use crate::financial_entry::MAX_ENTRY_INSTALLMENTS;
use crate::name::{optional_text, required_text};
use crate::{Actor, Amount, CalendarDate, ChargeId, CompanyId, DomainError, EntryId};

const MAX_PROVIDER_KEY_CHARS: usize = 32;
const MAX_EXTERNAL_ID_CHARS: usize = 200;
const MAX_DIGITABLE_LINE_CHARS: usize = 100;
const MAX_PIX_PAYLOAD_CHARS: usize = 1000;
const MAX_URL_CHARS: usize = 2000;

/// How the customer is asked to pay ([ADR 0045]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeMethod {
    /// A registered boleto: a digitable line, and the provider's page for the paper.
    Boleto,
    /// A dynamic Pix QR code, paid by its copy-and-paste text.
    Pix,
    /// A link to the provider's page, where the customer pays by card.
    CardLink,
}

impl ChargeMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boleto => "boleto",
            Self::Pix => "pix",
            Self::CardLink => "card_link",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "boleto" => Some(Self::Boleto),
            "pix" => Some(Self::Pix),
            "card_link" => Some(Self::CardLink),
            _ => None,
        }
    }
}

/// Where a charge is. Only an open charge can still be paid; the other three are final.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeStatus {
    Open,
    Paid,
    Cancelled,
    Expired,
}

impl ChargeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Paid => "paid",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "open" => Some(Self::Open),
            "paid" => Some(Self::Paid),
            "cancelled" => Some(Self::Cancelled),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

/// What the customer needs to pay, as the provider gave it. Which parts a charge has depends on its method.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChargePayment {
    /// The boleto's line, to type in an app.
    pub digitable_line: Option<String>,
    /// The Pix copy-and-paste text: printable ASCII, spaces included (a merchant name has them).
    pub pix_payload: Option<String>,
    /// The provider's page: the boleto's paper, or the card checkout.
    pub payment_url: Option<String>,
}

/// Unvalidated description of a charge once the provider has made it.
#[derive(Debug, Clone)]
pub struct ChargeDraft {
    pub entry_id: EntryId,
    pub installment: u16,
    pub method: ChargeMethod,
    pub amount: Amount,
    pub due_on: CalendarDate,
    pub provider: String,
    pub external_id: String,
    pub payment: ChargePayment,
}

/// A request to be paid for one installment of a receivable, made at a payment provider ([ADR 0045]).
/// It is never edited or deleted: it is cancelled, expires or is paid, and the trail keeps each move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charge {
    id: ChargeId,
    company_id: CompanyId,
    entry_id: EntryId,
    installment: u16,
    method: ChargeMethod,
    amount: Amount,
    due_on: CalendarDate,
    status: ChargeStatus,
    provider: String,
    external_id: String,
    payment: ChargePayment,
    issued_by: Actor,
}

fn is_provider_key(raw: &str) -> bool {
    let mut chars = raw.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && raw.len() <= MAX_PROVIDER_KEY_CHARS
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A provider's id is one line: no control character and no space that is not a plain one (the line
/// separators of Unicode are not control characters, and a database may judge them otherwise).
fn is_one_line(text: &str) -> bool {
    !text
        .chars()
        .any(|c| c.is_control() || (c.is_whitespace() && c != ' '))
}

/// An address the customer is sent to: https, no spaces, nothing a browser would take for a script.
fn clean_url(raw: Option<&str>) -> Result<Option<String>, DomainError> {
    let url = optional_text(raw, MAX_URL_CHARS, DomainError::InvalidChargePayment)?;
    match url {
        Some(url)
            if url.starts_with("https://")
                && url.len() > "https://".len()
                && !url.chars().any(|c| c.is_whitespace() || c.is_control()) =>
        {
            Ok(Some(url))
        }
        Some(_) => Err(DomainError::InvalidChargePayment),
        None => Ok(None),
    }
}

impl Charge {
    /// A new open charge. Whether the entry is a receivable of the company, has the installment and
    /// has the balance is the use case's to check.
    pub fn new(
        id: ChargeId,
        company_id: CompanyId,
        draft: ChargeDraft,
        issued_by: Actor,
    ) -> Result<Self, DomainError> {
        if draft.installment == 0 || usize::from(draft.installment) > MAX_ENTRY_INSTALLMENTS {
            return Err(DomainError::InvalidInstallmentNumber);
        }
        if draft.amount.as_decimal().is_zero() || draft.amount.as_decimal().scale() > 2 {
            return Err(DomainError::InvalidChargeAmount);
        }
        let provider = draft.provider.trim();
        if !is_provider_key(provider) {
            return Err(DomainError::InvalidChargeProvider);
        }
        let external_id = required_text(
            &draft.external_id,
            MAX_EXTERNAL_ID_CHARS,
            DomainError::InvalidChargeProvider,
        )?;
        if !is_one_line(&external_id) {
            return Err(DomainError::InvalidChargeProvider);
        }
        let payment = clean_payment(draft.method, &draft.payment)?;
        Ok(Self {
            id,
            company_id,
            entry_id: draft.entry_id,
            installment: draft.installment,
            method: draft.method,
            amount: draft.amount,
            due_on: draft.due_on,
            status: ChargeStatus::Open,
            provider: provider.to_owned(),
            external_id,
            payment,
            issued_by,
        })
    }

    /// The same charge in another status: what storage hands back, and what a move makes.
    pub fn with_status(mut self, status: ChargeStatus) -> Self {
        self.status = status;
        self
    }

    /// Calls the charge off. Only an open charge can be.
    pub fn cancelled(&self) -> Result<Self, DomainError> {
        if self.status != ChargeStatus::Open {
            return Err(DomainError::ChargeIsNotOpen);
        }
        Ok(self.clone().with_status(ChargeStatus::Cancelled))
    }

    pub fn id(&self) -> ChargeId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn entry_id(&self) -> EntryId {
        self.entry_id
    }

    pub fn installment(&self) -> u16 {
        self.installment
    }

    pub fn method(&self) -> ChargeMethod {
        self.method
    }

    pub fn amount(&self) -> Amount {
        self.amount
    }

    pub fn due_on(&self) -> CalendarDate {
        self.due_on
    }

    pub fn status(&self) -> ChargeStatus {
        self.status
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn external_id(&self) -> &str {
        &self.external_id
    }

    pub fn payment(&self) -> &ChargePayment {
        &self.payment
    }

    /// Who issued it: the settlement it leads to is made in their name, there being no system user.
    pub fn issued_by(&self) -> &Actor {
        &self.issued_by
    }
}

/// Each method has what it needs and nothing of another's: a Pix charge with a boleto line would be
/// a charge that does not say what it is.
fn clean_payment(method: ChargeMethod, raw: &ChargePayment) -> Result<ChargePayment, DomainError> {
    let digitable_line = optional_text(
        raw.digitable_line.as_deref(),
        MAX_DIGITABLE_LINE_CHARS,
        DomainError::InvalidChargePayment,
    )?;
    if digitable_line.as_deref().is_some_and(|line| {
        !line
            .chars()
            .all(|c| c.is_ascii_digit() || c == ' ' || c == '.')
    }) {
        return Err(DomainError::InvalidChargePayment);
    }
    let pix_payload = optional_text(
        raw.pix_payload.as_deref(),
        MAX_PIX_PAYLOAD_CHARS,
        DomainError::InvalidChargePayment,
    )?;
    if pix_payload
        .as_deref()
        .is_some_and(|payload| !payload.chars().all(|c| matches!(c, ' '..='~')))
    {
        return Err(DomainError::InvalidChargePayment);
    }
    let payment_url = clean_url(raw.payment_url.as_deref())?;
    let complete = match method {
        ChargeMethod::Boleto => digitable_line.is_some() && pix_payload.is_none(),
        ChargeMethod::Pix => pix_payload.is_some() && digitable_line.is_none(),
        ChargeMethod::CardLink => {
            payment_url.is_some() && digitable_line.is_none() && pix_payload.is_none()
        }
    };
    if !complete {
        return Err(DomainError::InvalidChargePayment);
    }
    Ok(ChargePayment {
        digitable_line,
        pix_payload,
        payment_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> Actor {
        Actor::User {
            id: crate::UserId::generate(),
            name: "Ana".into(),
        }
    }

    fn payment(line: Option<&str>, pix: Option<&str>, url: Option<&str>) -> ChargePayment {
        ChargePayment {
            digitable_line: line.map(str::to_owned),
            pix_payload: pix.map(str::to_owned),
            payment_url: url.map(str::to_owned),
        }
    }

    fn draft(method: ChargeMethod, payment: ChargePayment) -> ChargeDraft {
        ChargeDraft {
            entry_id: EntryId::generate(),
            installment: 1,
            method,
            amount: Amount::in_cents("150.50").unwrap(),
            due_on: CalendarDate::parse("2026-12-10").unwrap(),
            provider: "fake".into(),
            external_id: "ch_123".into(),
            payment,
        }
    }

    fn build(draft: ChargeDraft) -> Result<Charge, DomainError> {
        Charge::new(ChargeId::generate(), CompanyId::generate(), draft, actor())
    }

    #[test]
    fn a_new_charge_is_open_and_keeps_what_the_provider_said() {
        let charge = build(draft(
            ChargeMethod::Boleto,
            payment(
                Some("00190.00009 01234.567890"),
                None,
                Some("https://pay.example/b/1"),
            ),
        ))
        .unwrap();
        assert_eq!(charge.status(), ChargeStatus::Open);
        assert_eq!(charge.provider(), "fake");
        assert_eq!(charge.amount().to_string(), "150.5");
        assert_eq!(
            charge.payment().payment_url.as_deref(),
            Some("https://pay.example/b/1")
        );
    }

    #[test]
    fn each_method_has_what_it_needs_and_nothing_of_another() {
        let pix = payment(None, Some("00020126580014br.gov.bcb.pix"), None);
        assert!(build(draft(ChargeMethod::Pix, pix.clone())).is_ok());
        assert!(build(draft(ChargeMethod::Boleto, pix)).is_err());
        let both = payment(Some("1234"), Some("0002"), None);
        assert!(build(draft(ChargeMethod::Boleto, both.clone())).is_err());
        assert!(build(draft(ChargeMethod::Pix, both)).is_err());
        let link = payment(None, None, Some("https://pay.example/c/1"));
        assert!(build(draft(ChargeMethod::CardLink, link)).is_ok());
        assert!(build(draft(ChargeMethod::CardLink, payment(None, None, None))).is_err());
    }

    #[test]
    fn refuses_an_address_that_is_not_https_or_has_spaces() {
        for url in [
            "http://pay.example/1",
            "javascript:alert(1)",
            "https://",
            "https://pay.example/a b",
        ] {
            let link = payment(None, None, Some(url));
            assert_eq!(
                build(draft(ChargeMethod::CardLink, link)),
                Err(DomainError::InvalidChargePayment),
                "{url}"
            );
        }
    }

    #[test]
    fn refuses_a_provider_key_or_id_that_is_not_one() {
        let ok = payment(None, Some("0002"), None);
        for provider in ["", "Fake", "1fake", "fa ke", &"a".repeat(33)] {
            let mut charge = draft(ChargeMethod::Pix, ok.clone());
            charge.provider = provider.into();
            assert_eq!(build(charge), Err(DomainError::InvalidChargeProvider));
        }
        for id in ["", "  ", "a\nb"] {
            let mut charge = draft(ChargeMethod::Pix, ok.clone());
            charge.external_id = id.into();
            assert_eq!(build(charge), Err(DomainError::InvalidChargeProvider));
        }
    }

    #[test]
    fn refuses_an_amount_of_zero_or_a_bad_installment() {
        let ok = payment(None, Some("0002"), None);
        let mut charge = draft(ChargeMethod::Pix, ok.clone());
        charge.amount = Amount::ZERO;
        assert_eq!(build(charge), Err(DomainError::InvalidChargeAmount));
        for number in [0, 121] {
            let mut charge = draft(ChargeMethod::Pix, ok.clone());
            charge.installment = number;
            assert_eq!(build(charge), Err(DomainError::InvalidInstallmentNumber));
        }
    }

    #[test]
    fn only_an_open_charge_is_cancelled() {
        let charge = build(draft(ChargeMethod::Pix, payment(None, Some("0002"), None))).unwrap();
        let cancelled = charge.cancelled().unwrap();
        assert_eq!(cancelled.status(), ChargeStatus::Cancelled);
        assert_eq!(cancelled.cancelled(), Err(DomainError::ChargeIsNotOpen));
    }

    #[test]
    fn names_round_trip() {
        for method in [
            ChargeMethod::Boleto,
            ChargeMethod::Pix,
            ChargeMethod::CardLink,
        ] {
            assert_eq!(ChargeMethod::parse(method.as_str()), Some(method));
        }
        for status in [
            ChargeStatus::Open,
            ChargeStatus::Paid,
            ChargeStatus::Cancelled,
            ChargeStatus::Expired,
        ] {
            assert_eq!(ChargeStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(ChargeMethod::parse("cash"), None);
    }
}
