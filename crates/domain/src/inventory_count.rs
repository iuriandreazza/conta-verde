use crate::name::optional_line;
use crate::{Amount, CalendarDate, CompanyId, DomainError, InventoryCountId, ProductId};
use rust_decimal::Decimal;

/// How many products a count has: a bound, not a rule.
pub const MAX_COUNT_LINES: usize = 500;

const MAX_NOTES_CHARS: usize = 200;

/// Where a count is in its life ([ADR 0056]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountStatus {
    /// Being counted: the only state in which it changes.
    Open,
    /// Its differences were written as adjustments of the stock.
    Applied,
    Cancelled,
}

impl CountStatus {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Applied => "applied",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "open" => Some(Self::Open),
            "applied" => Some(Self::Applied),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// One product counted, as it is asked for.
#[derive(Debug, Clone)]
pub struct CountLineDraft {
    pub product_id: ProductId,
    /// What was found on the shelf: 0 or more, up to three decimals.
    pub counted: String,
}

/// Unvalidated input for writing or changing a count.
#[derive(Debug, Clone)]
pub struct InventoryCountDraft {
    /// The day of the count, which is the day the adjustments are booked.
    pub counted_on: String,
    pub notes: Option<String>,
    pub lines: Vec<CountLineDraft>,
}

/// What was found of one product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CountLine {
    product_id: ProductId,
    counted: Amount,
}

impl CountLine {
    pub fn product_id(&self) -> ProductId {
        self.product_id
    }

    pub fn counted(&self) -> Amount {
        self.counted
    }
}

/// A count of the stock on a day: what was found of some products ([ADR 0056]). Applying it writes, for each
/// product that differs, an adjustment to what was found. What the stock was expected to hold is not part of it: it
/// is what the stock holds when the count is looked at or applied.
///
/// Whether each product is one that has stock is the use case's to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryCount {
    id: InventoryCountId,
    company_id: CompanyId,
    /// The company's own sequence, given by the store when the count is first written; 0 before.
    number: i64,
    counted_on: CalendarDate,
    notes: Option<String>,
    lines: Vec<CountLine>,
    status: CountStatus,
}

impl InventoryCount {
    pub fn new(
        id: InventoryCountId,
        company_id: CompanyId,
        draft: InventoryCountDraft,
    ) -> Result<Self, DomainError> {
        let counted_on =
            CalendarDate::parse(&draft.counted_on).ok_or(DomainError::InvalidDate("counted_on"))?;
        let notes = optional_line(
            draft.notes.as_deref(),
            MAX_NOTES_CHARS,
            DomainError::InvalidNote,
        )?;
        if draft.lines.is_empty() {
            return Err(DomainError::NoCountLines);
        }
        if draft.lines.len() > MAX_COUNT_LINES {
            return Err(DomainError::TooManyCountLines);
        }
        let mut lines: Vec<CountLine> = Vec::with_capacity(draft.lines.len());
        for line in &draft.lines {
            let counted = Amount::parse(line.counted.trim())
                .filter(|counted| counted.as_decimal().scale() <= 3)
                .ok_or(DomainError::InvalidCountQuantity)?;
            if lines.iter().any(|seen| seen.product_id == line.product_id) {
                return Err(DomainError::DuplicateCountLine);
            }
            lines.push(CountLine {
                product_id: line.product_id,
                counted,
            });
        }
        Ok(Self {
            id,
            company_id,
            number: 0,
            counted_on,
            notes,
            lines,
            status: CountStatus::Open,
        })
    }

    /// The count as it was stored: its number and its status.
    pub fn restored(mut self, number: i64, status: CountStatus) -> Self {
        self.number = number;
        self.status = status;
        self
    }

    pub fn with_number(mut self, number: i64) -> Self {
        self.number = number;
        self
    }

    /// The count with its content replaced by the draft's; only one that is open can change.
    pub fn replaced_with(&self, draft: InventoryCountDraft) -> Result<Self, DomainError> {
        if self.status != CountStatus::Open {
            return Err(DomainError::CountIsLocked);
        }
        Ok(Self::new(self.id, self.company_id, draft)?.restored(self.number, self.status))
    }

    /// The count with only its status changed: what a store does when it records a transition.
    pub fn with_status(&self, status: CountStatus) -> Self {
        Self {
            status,
            ..self.clone()
        }
    }

    pub fn applied(&self) -> Result<Self, DomainError> {
        if self.status != CountStatus::Open {
            return Err(DomainError::CountIsLocked);
        }
        Ok(self.with_status(CountStatus::Applied))
    }

    pub fn cancelled(&self) -> Result<Self, DomainError> {
        if self.status != CountStatus::Open {
            return Err(DomainError::CountIsLocked);
        }
        Ok(self.with_status(CountStatus::Cancelled))
    }

    /// What the count says of a product, given what the stock holds: the quantity to adjust by, which is 0 when they
    /// agree.
    pub fn difference(line: &CountLine, on_hand: Decimal) -> Decimal {
        (line.counted.as_decimal() - on_hand).normalize()
    }

    pub fn id(&self) -> InventoryCountId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn number(&self) -> i64 {
        self.number
    }

    pub fn counted_on(&self) -> CalendarDate {
        self.counted_on
    }

    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }

    pub fn lines(&self) -> &[CountLine] {
        &self.lines
    }

    pub fn status(&self) -> CountStatus {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(product: ProductId, counted: &str) -> CountLineDraft {
        CountLineDraft {
            product_id: product,
            counted: counted.into(),
        }
    }

    fn draft(lines: Vec<CountLineDraft>) -> InventoryCountDraft {
        InventoryCountDraft {
            counted_on: "2026-11-30".into(),
            notes: None,
            lines,
        }
    }

    fn count(lines: Vec<CountLineDraft>) -> InventoryCount {
        InventoryCount::new(
            InventoryCountId::generate(),
            CompanyId::generate(),
            draft(lines),
        )
        .unwrap()
        .with_number(3)
    }

    #[test]
    fn a_count_has_each_product_once_with_what_was_found_of_it() {
        let (coffee, bread) = (ProductId::generate(), ProductId::generate());
        let counted = count(vec![line(coffee, "12.5"), line(bread, "0")]);
        assert_eq!(counted.lines().len(), 2);
        assert_eq!(counted.lines()[1].counted().to_string(), "0");
        for (lines, error) in [
            (vec![], DomainError::NoCountLines),
            (
                vec![line(coffee, "1"), line(coffee, "2")],
                DomainError::DuplicateCountLine,
            ),
            (vec![line(coffee, "-1")], DomainError::InvalidCountQuantity),
            (
                vec![line(coffee, "1.0001")],
                DomainError::InvalidCountQuantity,
            ),
            (vec![line(coffee, "x")], DomainError::InvalidCountQuantity),
        ] {
            assert_eq!(
                InventoryCount::new(
                    InventoryCountId::generate(),
                    CompanyId::generate(),
                    draft(lines)
                )
                .unwrap_err(),
                error
            );
        }
        let many = (0..=MAX_COUNT_LINES)
            .map(|_| line(ProductId::generate(), "1"))
            .collect();
        assert_eq!(
            InventoryCount::new(
                InventoryCountId::generate(),
                CompanyId::generate(),
                draft(many)
            )
            .unwrap_err(),
            DomainError::TooManyCountLines
        );
    }

    #[test]
    fn an_open_count_is_applied_or_cancelled_once_and_changes_only_while_open() {
        let open = count(vec![line(ProductId::generate(), "1")]);
        assert_eq!(open.status(), CountStatus::Open);
        let applied = open.applied().unwrap();
        assert_eq!(applied.applied().unwrap_err(), DomainError::CountIsLocked);
        assert_eq!(applied.cancelled().unwrap_err(), DomainError::CountIsLocked);
        assert_eq!(
            applied
                .replaced_with(draft(vec![line(ProductId::generate(), "2")]))
                .unwrap_err(),
            DomainError::CountIsLocked
        );
        assert_eq!(open.cancelled().unwrap().status(), CountStatus::Cancelled);
        let replaced = open
            .replaced_with(draft(vec![line(ProductId::generate(), "2")]))
            .unwrap();
        assert_eq!(
            (replaced.number(), replaced.status()),
            (3, CountStatus::Open)
        );
    }

    #[test]
    fn the_difference_is_what_was_found_less_what_the_stock_holds() {
        let counted = count(vec![line(ProductId::generate(), "10")]);
        let found = &counted.lines()[0];
        assert_eq!(
            InventoryCount::difference(found, Decimal::new(75, 1)).to_string(),
            "2.5"
        );
        assert_eq!(
            InventoryCount::difference(found, Decimal::from(12)).to_string(),
            "-2"
        );
        assert_eq!(
            InventoryCount::difference(found, Decimal::from(10)).to_string(),
            "0"
        );
    }
}
