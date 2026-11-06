use rust_decimal::Decimal;

use crate::{
    BankAccountId, CalendarDate, CompanyId, DomainError, SettlementId, SignedAmount,
    StatementLineId, TransferId,
};

const MAX_DESCRIPTION_CHARS: usize = 200;
const MAX_BANK_REF_CHARS: usize = 100;

/// What the bank says about one line, unvalidated: the amount as decimal text with a `-` when money
/// went out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementLineDraft {
    pub posted_on: String,
    pub amount: String,
    pub description: String,
    /// The bank's own reference for it (`FITID` in OFX), when the file has one.
    pub bank_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStatus {
    Pending,
    Matched,
    Ignored,
}

impl LineStatus {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Matched => "matched",
            Self::Ignored => "ignored",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(Self::Pending),
            "matched" => Some(Self::Matched),
            "ignored" => Some(Self::Ignored),
            _ => None,
        }
    }
}

/// What a line was matched to: one settlement of an entry, or one transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMatch {
    Settlement(SettlementId),
    Transfer(TransferId),
}

/// One line of a bank statement ([ADR 0034]): what the bank says happened in an account. It is never
/// edited: only its status moves, between pending, matched to a settlement or a transfer, and ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementLine {
    id: StatementLineId,
    company_id: CompanyId,
    account_id: BankAccountId,
    posted_on: CalendarDate,
    amount: SignedAmount,
    description: String,
    bank_ref: Option<String>,
    fingerprint: String,
    status: LineStatus,
    matched: Option<LineMatch>,
}

impl StatementLine {
    /// A new pending line. `occurrence` is its place among the lines of the file that are the same as
    /// it (0 for the first), which keeps two identical lines of a day apart in the fingerprint.
    /// Whether the account is the company's and usable is the use case's to check.
    pub fn new(
        id: StatementLineId,
        company_id: CompanyId,
        account_id: BankAccountId,
        draft: StatementLineDraft,
        occurrence: u32,
    ) -> Result<Self, DomainError> {
        let posted_on = CalendarDate::parse(draft.posted_on.trim())
            .ok_or(DomainError::InvalidDate("posted_on"))?;
        let amount = SignedAmount::in_cents(draft.amount.trim())
            .filter(|amount| amount.as_decimal() != Decimal::ZERO)
            .ok_or(DomainError::InvalidStatementAmount)?;
        let description = collapsed_line(&draft.description, MAX_DESCRIPTION_CHARS)
            .ok_or(DomainError::InvalidStatementDescription)?;
        let bank_ref = match draft.bank_ref.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(reference) => Some(
                collapsed_line(reference, MAX_BANK_REF_CHARS)
                    .ok_or(DomainError::InvalidStatementDescription)?,
            ),
        };
        let fingerprint = format!(
            "{posted_on}|{amount}|{}|{}|{occurrence}",
            description.to_lowercase(),
            bank_ref.as_deref().unwrap_or("")
        );
        Ok(Self {
            id,
            company_id,
            account_id,
            posted_on,
            amount,
            description,
            bank_ref,
            fingerprint,
            status: LineStatus::Pending,
            matched: None,
        })
    }

    /// A line read back from storage: its stored fingerprint, which counts its place among identical
    /// lines of the file it came in, and the status it was left in.
    pub fn with_stored_state(
        mut self,
        fingerprint: String,
        status: LineStatus,
        matched: Option<LineMatch>,
    ) -> Self {
        self.fingerprint = fingerprint;
        self.status = status;
        self.matched = matched;
        self
    }

    /// Pending to matched.
    pub fn matched_to(&self, target: LineMatch) -> Result<Self, DomainError> {
        self.moved(LineStatus::Pending, LineStatus::Matched, Some(target))
            .ok_or(DomainError::LineIsNotPending)
    }

    /// Matched to pending.
    pub fn unmatched(&self) -> Result<Self, DomainError> {
        self.moved(LineStatus::Matched, LineStatus::Pending, None)
            .ok_or(DomainError::LineIsNotMatched)
    }

    /// Pending to ignored.
    pub fn ignored(&self) -> Result<Self, DomainError> {
        self.moved(LineStatus::Pending, LineStatus::Ignored, None)
            .ok_or(DomainError::LineIsNotPending)
    }

    /// Ignored to pending.
    pub fn restored(&self) -> Result<Self, DomainError> {
        self.moved(LineStatus::Ignored, LineStatus::Pending, None)
            .ok_or(DomainError::LineIsNotIgnored)
    }

    fn moved(&self, from: LineStatus, to: LineStatus, matched: Option<LineMatch>) -> Option<Self> {
        (self.status == from).then(|| Self {
            status: to,
            matched,
            ..self.clone()
        })
    }

    pub fn id(&self) -> StatementLineId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn account_id(&self) -> BankAccountId {
        self.account_id
    }

    pub fn posted_on(&self) -> CalendarDate {
        self.posted_on
    }

    /// Positive came into the account, negative went out of it. Never 0.
    pub fn amount(&self) -> SignedAmount {
        self.amount
    }

    /// Whether money came into the account.
    pub fn is_incoming(&self) -> bool {
        self.amount.as_decimal() > Decimal::ZERO
    }

    /// How much moved, whichever way: more than 0, in cents.
    pub fn magnitude(&self) -> Decimal {
        self.amount.as_decimal().abs()
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn bank_ref(&self) -> Option<&str> {
        self.bank_ref.as_deref()
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn status(&self) -> LineStatus {
        self.status
    }

    pub fn matched(&self) -> Option<LineMatch> {
        self.matched
    }
}

/// One line of text, trimmed, with every run of white space as a single space: 1 to `max` characters,
/// no control characters and no line or paragraph separators.
fn collapsed_line(raw: &str, max: usize) -> Option<String> {
    if raw
        .chars()
        .any(|c| matches!(c, '\u{2028}' | '\u{2029}') || (c.is_control() && !c.is_whitespace()))
    {
        return None;
    }
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty() && text.chars().count() <= max).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(amount: &str, description: &str) -> StatementLineDraft {
        StatementLineDraft {
            posted_on: "2026-11-05".into(),
            amount: amount.into(),
            description: description.into(),
            bank_ref: Some(" ABC123 ".into()),
        }
    }

    fn line(amount: &str, description: &str, occurrence: u32) -> StatementLine {
        StatementLine::new(
            StatementLineId::generate(),
            CompanyId::generate(),
            BankAccountId::generate(),
            draft(amount, description),
            occurrence,
        )
        .unwrap()
    }

    #[test]
    fn reads_the_line_as_the_bank_wrote_it() {
        let incoming = line("1250.50", "  PIX   RECEBIDO \n Maria ", 0);
        assert_eq!(incoming.description(), "PIX RECEBIDO Maria");
        assert_eq!(incoming.bank_ref(), Some("ABC123"));
        assert!(incoming.is_incoming());
        assert_eq!(incoming.magnitude().to_string(), "1250.5");
        assert_eq!(incoming.amount().to_string(), "1250.5");
        assert_eq!(incoming.status(), LineStatus::Pending);
        let outgoing = line("-30", "TARIFA", 0);
        assert!(!outgoing.is_incoming());
        assert_eq!(outgoing.magnitude().to_string(), "30");
    }

    #[test]
    fn refuses_what_is_not_a_line() {
        let new = |draft| {
            StatementLine::new(
                StatementLineId::generate(),
                CompanyId::generate(),
                BankAccountId::generate(),
                draft,
                0,
            )
            .unwrap_err()
        };
        for amount in ["0", "-0", "0.00", "1.005", "abc", "", "+5"] {
            assert_eq!(
                new(draft(amount, "X")),
                DomainError::InvalidStatementAmount,
                "{amount:?}"
            );
        }
        for description in ["", "   ", "a\u{0}b", "a\u{2028}b"] {
            assert_eq!(
                new(draft("1", description)),
                DomainError::InvalidStatementDescription,
                "{description:?}"
            );
        }
        assert_eq!(
            new(draft("1", &"x".repeat(201))),
            DomainError::InvalidStatementDescription
        );
        assert_eq!(
            new(StatementLineDraft {
                posted_on: "2026-02-30".into(),
                ..draft("1", "X")
            }),
            DomainError::InvalidDate("posted_on")
        );
    }

    #[test]
    fn the_fingerprint_is_the_same_for_the_same_line_and_apart_for_identical_ones_of_a_file() {
        let first = line("-5", "CAFE", 0);
        assert_eq!(first.fingerprint(), line("-5", "cafe", 0).fingerprint());
        assert_ne!(first.fingerprint(), line("-5", "CAFE", 1).fingerprint());
        assert_ne!(first.fingerprint(), line("-5.01", "CAFE", 0).fingerprint());
        assert_ne!(first.fingerprint(), line("5", "CAFE", 0).fingerprint());
    }

    #[test]
    fn moves_between_pending_matched_and_ignored_and_nowhere_else() {
        let pending = line("100", "VENDA", 0);
        let target = LineMatch::Settlement(SettlementId::generate());
        let matched = pending.matched_to(target).unwrap();
        assert_eq!(
            (matched.status(), matched.matched()),
            (LineStatus::Matched, Some(target))
        );
        assert_eq!(
            matched.matched_to(target),
            Err(DomainError::LineIsNotPending)
        );
        assert_eq!(matched.ignored(), Err(DomainError::LineIsNotPending));
        assert_eq!(matched.restored(), Err(DomainError::LineIsNotIgnored));
        let back = matched.unmatched().unwrap();
        assert_eq!((back.status(), back.matched()), (LineStatus::Pending, None));
        assert_eq!(back.unmatched(), Err(DomainError::LineIsNotMatched));
        let ignored = pending.ignored().unwrap();
        assert_eq!(ignored.status(), LineStatus::Ignored);
        assert_eq!(ignored.unmatched(), Err(DomainError::LineIsNotMatched));
        assert_eq!(ignored.restored().unwrap().status(), LineStatus::Pending);
        assert_eq!(pending.restored(), Err(DomainError::LineIsNotIgnored));
    }

    #[test]
    fn statuses_read_and_write_the_same_key() {
        for status in [
            LineStatus::Pending,
            LineStatus::Matched,
            LineStatus::Ignored,
        ] {
            assert_eq!(LineStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(LineStatus::parse("done"), None);
    }
}
