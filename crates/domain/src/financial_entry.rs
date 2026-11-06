use rust_decimal::Decimal;

use crate::name::required_text;
use crate::{
    Amount, CalendarDate, CategoryId, CompanyId, CostCenterId, DomainError, EntryId, PartyId,
    RecurrenceId, SaleId, SaleReturnId,
};

/// How many installments an entry can have: ten years of monthly ones, a bound and not a rule.
pub const MAX_ENTRY_INSTALLMENTS: usize = 120;
const MAX_DESCRIPTION_CHARS: usize = 200;

/// Whether the money is owed to the company or by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Receivable,
    Payable,
}

impl EntryKind {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receivable => "receivable",
            Self::Payable => "payable",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "receivable" => Some(Self::Receivable),
            "payable" => Some(Self::Payable),
            _ => None,
        }
    }
}

/// One installment as it is asked for: unvalidated, the amount as decimal text.
#[derive(Debug, Clone)]
pub struct EntryInstallmentDraft {
    pub due_date: String,
    pub amount: String,
}

/// One installment of an entry: its place in the order (from 1), the day it falls due and what is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryInstallment {
    number: u16,
    due_date: CalendarDate,
    amount: Amount,
}

impl EntryInstallment {
    pub fn number(&self) -> u16 {
        self.number
    }

    pub fn due_date(&self) -> CalendarDate {
        self.due_date
    }

    pub fn amount(&self) -> Amount {
        self.amount
    }
}

/// An occurrence of a recurrence ([ADR 0033]): a recurrence has at most one entry per occurrence day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecurrenceOrigin {
    pub recurrence_id: RecurrenceId,
    pub occurrence_on: CalendarDate,
}

/// Where an entry came from when something other than a person made it ([ADR 0037]): each origin has at
/// most one entry, so making it twice, or in two places, is safe. It never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOrigin {
    /// A recurrence generated it on one of its occurrence days.
    Recurrence(RecurrenceOrigin),
    /// The receivable of a completed sale.
    Sale(SaleId),
    /// The refund of a return of goods.
    SaleReturn(SaleReturnId),
}

/// Unvalidated input for registering or changing an entry.
#[derive(Debug, Clone)]
pub struct FinancialEntryDraft {
    pub kind: EntryKind,
    pub description: String,
    pub party_id: Option<PartyId>,
    pub category_id: CategoryId,
    pub cost_center_id: Option<CostCenterId>,
    pub competence_date: String,
    pub installments: Vec<EntryInstallmentDraft>,
}

/// Money a company is owed or owes ([ADR 0029]): classified by one category, with an optional party
/// and cost center, belonging to a competence day, and split into installments that fall due on
/// their own days. The total is the sum of the installments and is never stored.
///
/// Whether the category, the cost center and the party are usable is the use case's to check: the
/// entry only holds their ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinancialEntry {
    id: EntryId,
    company_id: CompanyId,
    kind: EntryKind,
    description: String,
    party_id: Option<PartyId>,
    category_id: CategoryId,
    cost_center_id: Option<CostCenterId>,
    competence_date: CalendarDate,
    installments: Vec<EntryInstallment>,
    origin: Option<EntryOrigin>,
    /// Set once, by whatever made the entry, when it is cancelled: it then counts nowhere ([ADR 0037]).
    voided: bool,
}

impl FinancialEntry {
    pub fn new(
        id: EntryId,
        company_id: CompanyId,
        draft: FinancialEntryDraft,
    ) -> Result<Self, DomainError> {
        let description = clean_description(&draft.description)?;
        let competence_date = CalendarDate::parse(&draft.competence_date)
            .ok_or(DomainError::InvalidDate("competence_date"))?;
        let installments = validated_installments(&draft.installments)?;
        Ok(Self {
            id,
            company_id,
            kind: draft.kind,
            description,
            party_id: draft.party_id,
            category_id: draft.category_id,
            cost_center_id: draft.cost_center_id,
            competence_date,
            installments,
            origin: None,
            voided: false,
        })
    }

    /// The same entry, said to have been made by something: a recurrence, a sale, a return.
    pub fn with_origin(mut self, origin: EntryOrigin) -> Self {
        self.origin = Some(origin);
        self
    }

    pub fn origin(&self) -> Option<EntryOrigin> {
        self.origin
    }

    /// The same entry with every editable field replaced by the draft's. Where it came from is not
    /// editable, a voided entry is not edited at all, and what a sale or a return made changes through them
    /// (a receivable that drifted from its sale would be two records of one fact).
    pub fn replaced_with(&self, draft: FinancialEntryDraft) -> Result<Self, DomainError> {
        if self.voided {
            return Err(DomainError::EntryIsVoided);
        }
        if matches!(
            self.origin,
            Some(EntryOrigin::Sale(_) | EntryOrigin::SaleReturn(_))
        ) {
            return Err(DomainError::EntryFollowsASale);
        }
        let mut replaced = Self::new(self.id, self.company_id, draft)?;
        replaced.origin = self.origin;
        Ok(replaced)
    }

    /// The entry cancelled. It is never un-voided: another one is made.
    pub fn voided(&self) -> Result<Self, DomainError> {
        if self.voided {
            return Err(DomainError::EntryIsVoided);
        }
        Ok(Self {
            voided: true,
            ..self.clone()
        })
    }

    /// The entry as it was stored, voided or not.
    pub fn with_voided(mut self, voided: bool) -> Self {
        self.voided = voided;
        self
    }

    pub fn is_voided(&self) -> bool {
        self.voided
    }

    pub fn id(&self) -> EntryId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn party_id(&self) -> Option<PartyId> {
        self.party_id
    }

    pub fn category_id(&self) -> CategoryId {
        self.category_id
    }

    pub fn cost_center_id(&self) -> Option<CostCenterId> {
        self.cost_center_id
    }

    pub fn competence_date(&self) -> CalendarDate {
        self.competence_date
    }

    pub fn installments(&self) -> &[EntryInstallment] {
        &self.installments
    }

    /// The installment with this number, if the entry has it.
    pub fn installment(&self, number: u16) -> Option<&EntryInstallment> {
        self.installments.iter().find(|item| item.number == number)
    }

    /// The sum of the installments, exact. Written without trailing zeros, like every amount.
    pub fn total(&self) -> Decimal {
        let sum: Decimal = self
            .installments
            .iter()
            .map(|installment| installment.amount.as_decimal())
            .sum();
        sum.normalize()
    }

    /// The installments in one canonical text, `number:due_date:amount` separated by commas, which
    /// is what the audit trail records: the same installments always read the same, so an edit that
    /// changes nothing compares equal.
    pub fn installments_text(&self) -> String {
        self.installments
            .iter()
            .map(|item| format!("{}:{}:{}", item.number, item.due_date, item.amount))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// One line, 1 to 200 characters: a description is read in lists and e-mails.
pub(crate) fn clean_description(raw: &str) -> Result<String, DomainError> {
    let text = required_text(raw, MAX_DESCRIPTION_CHARS, DomainError::InvalidDescription)?;
    // The line and paragraph separators are not control characters, and break a line all the same.
    if text
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        return Err(DomainError::InvalidDescription);
    }
    Ok(text)
}

/// The rules that span the installments of an entry, which no single-row constraint can state:
/// there is at least one and at most [`MAX_ENTRY_INSTALLMENTS`], and the due dates do not decrease
/// with the number. Each amount is more than 0 and in cents.
fn validated_installments(
    drafts: &[EntryInstallmentDraft],
) -> Result<Vec<EntryInstallment>, DomainError> {
    if drafts.is_empty() {
        return Err(DomainError::NoEntryInstallments);
    }
    if drafts.len() > MAX_ENTRY_INSTALLMENTS {
        return Err(DomainError::TooManyEntryInstallments);
    }
    let mut installments = Vec::with_capacity(drafts.len());
    for (index, draft) in drafts.iter().enumerate() {
        let due_date = CalendarDate::parse(&draft.due_date)
            .ok_or(DomainError::InvalidDate("installment due_date"))?;
        let amount = Amount::in_cents(draft.amount.trim())
            .filter(|amount| amount.as_decimal() > Decimal::ZERO)
            .ok_or(DomainError::InvalidInstallmentAmount)?;
        installments.push(EntryInstallment {
            number: u16::try_from(index + 1).expect("at most 120 installments"),
            due_date,
            amount,
        });
    }
    if installments
        .windows(2)
        .any(|pair| pair[0].due_date > pair[1].due_date)
    {
        return Err(DomainError::InstallmentDatesDecrease);
    }
    Ok(installments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installment(due: &str, amount: &str) -> EntryInstallmentDraft {
        EntryInstallmentDraft {
            due_date: due.into(),
            amount: amount.into(),
        }
    }

    fn draft(parts: &[(&str, &str)]) -> FinancialEntryDraft {
        FinancialEntryDraft {
            kind: EntryKind::Receivable,
            description: "  Venda de bolos  ".into(),
            party_id: None,
            category_id: CategoryId::generate(),
            cost_center_id: None,
            competence_date: "2026-11-05".into(),
            installments: parts
                .iter()
                .map(|(due, amount)| installment(due, amount))
                .collect(),
        }
    }

    fn build(parts: &[(&str, &str)]) -> Result<FinancialEntry, DomainError> {
        FinancialEntry::new(EntryId::generate(), CompanyId::generate(), draft(parts))
    }

    #[test]
    fn numbers_the_installments_in_order_and_sums_them_exactly() {
        let entry = build(&[
            ("2026-12-10", "100.10"),
            ("2027-01-10", "100.20"),
            ("2027-01-10", "0.1"),
        ])
        .unwrap();
        assert_eq!(entry.description(), "Venda de bolos");
        let numbers: Vec<u16> = entry.installments().iter().map(|i| i.number()).collect();
        assert_eq!(numbers, [1, 2, 3]);
        assert_eq!(entry.total().to_string(), "200.4");
        assert_eq!(
            entry.installments_text(),
            "1:2026-12-10:100.1,2:2027-01-10:100.2,3:2027-01-10:0.1"
        );
    }

    #[test]
    fn needs_between_one_and_the_most_installments() {
        assert_eq!(build(&[]).unwrap_err(), DomainError::NoEntryInstallments);
        let many: Vec<(&str, &str)> = (0..=MAX_ENTRY_INSTALLMENTS)
            .map(|_| ("2026-12-10", "1"))
            .collect();
        assert_eq!(
            build(&many).unwrap_err(),
            DomainError::TooManyEntryInstallments
        );
        let most: Vec<(&str, &str)> = (0..MAX_ENTRY_INSTALLMENTS)
            .map(|_| ("2026-12-10", "1"))
            .collect();
        assert!(build(&most).is_ok());
    }

    #[test]
    fn an_amount_is_more_than_zero_and_in_cents_and_never_rounded() {
        for amount in [
            "0", "0.00", "-1", "1.001", "1.005", "abc", "", "1e3", "1,50",
        ] {
            assert_eq!(
                build(&[("2026-12-10", amount)]).unwrap_err(),
                DomainError::InvalidInstallmentAmount,
                "{amount:?}"
            );
        }
        assert!(build(&[("2026-12-10", "0.01")]).is_ok());
        assert!(build(&[("2026-12-10", "9999999999999.99")]).is_ok());
        assert!(build(&[("2026-12-10", "10000000000000")]).is_err());
    }

    #[test]
    fn due_dates_do_not_decrease_but_may_repeat() {
        assert!(build(&[("2026-12-10", "1"), ("2026-12-10", "1")]).is_ok());
        assert_eq!(
            build(&[("2026-12-10", "1"), ("2026-12-09", "1")]).unwrap_err(),
            DomainError::InstallmentDatesDecrease
        );
    }

    #[test]
    fn names_the_date_that_is_wrong() {
        assert_eq!(
            build(&[("10/12/2026", "1")]).unwrap_err(),
            DomainError::InvalidDate("installment due_date")
        );
        let mut bad = draft(&[("2026-12-10", "1")]);
        bad.competence_date = "2026-02-30".into();
        assert_eq!(
            FinancialEntry::new(EntryId::generate(), CompanyId::generate(), bad).unwrap_err(),
            DomainError::InvalidDate("competence_date")
        );
    }

    #[test]
    fn a_description_is_one_line_of_one_to_two_hundred_characters() {
        for description in [
            "",
            "   ",
            "a\nb",
            "a\tb",
            "a\0b",
            "a\u{2028}b",
            "a\u{2029}b",
            &"x".repeat(201),
        ] {
            let mut bad = draft(&[("2026-12-10", "1")]);
            bad.description = description.into();
            assert_eq!(
                FinancialEntry::new(EntryId::generate(), CompanyId::generate(), bad).unwrap_err(),
                DomainError::InvalidDescription,
                "{description:?}"
            );
        }
        let mut good = draft(&[("2026-12-10", "1")]);
        good.description = "x".repeat(200);
        assert!(FinancialEntry::new(EntryId::generate(), CompanyId::generate(), good).is_ok());
    }

    #[test]
    fn replacing_keeps_the_identity() {
        let entry = build(&[("2026-12-10", "10")]).unwrap();
        let mut changed = draft(&[("2027-01-10", "20")]);
        changed.kind = EntryKind::Payable;
        let replaced = entry.replaced_with(changed).unwrap();
        assert_eq!(
            (replaced.id(), replaced.company_id()),
            (entry.id(), entry.company_id())
        );
        assert_eq!(replaced.kind(), EntryKind::Payable);
        assert_eq!(replaced.total().to_string(), "20");
    }

    #[test]
    fn the_kind_keys_are_stable_and_round_trip() {
        for kind in [EntryKind::Receivable, EntryKind::Payable] {
            assert_eq!(EntryKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(EntryKind::parse("Receivable"), None);
    }
}
