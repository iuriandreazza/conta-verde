use rust_decimal::Decimal;
use uuid::Uuid;

use crate::name::optional_line;
use crate::{
    Amount, CalendarDate, CompanyId, DomainError, ProductId, StockMovementId, round_to_cents,
};

const MAX_NOTE_CHARS: usize = 200;

/// The most a quantity or a value may hold: what a `NUMERIC(19, 6)` column fits.
const LIMIT: Decimal = Decimal::from_parts(1_316_134_912, 2_328, 0, false, 0);

/// What a movement did to the stock ([ADR 0052]). An `in` adds, an `out` takes, an `adjustment` corrects in
/// either direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockMovementKind {
    In,
    Out,
    Adjustment,
}

impl StockMovementKind {
    /// The key the API sends and the database stores; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
            Self::Adjustment => "adjustment",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "in" => Some(Self::In),
            "out" => Some(Self::Out),
            "adjustment" => Some(Self::Adjustment),
            _ => None,
        }
    }
}

/// What made a movement. A movement a person writes by hand has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockOriginKind {
    PurchaseReceipt,
    Sale,
    SaleReturn,
    InventoryCount,
    /// The opening balance a file gave a product ([ADR 0094]).
    Import,
}

impl StockOriginKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PurchaseReceipt => "purchase_receipt",
            Self::Sale => "sale",
            Self::SaleReturn => "sale_return",
            Self::InventoryCount => "inventory_count",
            Self::Import => "import",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "purchase_receipt" => Some(Self::PurchaseReceipt),
            "sale" => Some(Self::Sale),
            "sale_return" => Some(Self::SaleReturn),
            "inventory_count" => Some(Self::InventoryCount),
            "import" => Some(Self::Import),
            _ => None,
        }
    }
}

/// The thing a movement came from, and the line of it: an event writes each of its movements once, so doing it
/// twice, or in two places, is safe ([ADR 0052]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockOrigin {
    pub kind: StockOriginKind,
    pub id: Uuid,
    /// The place of the line in the document, from 1.
    pub line: u16,
}

/// Where a product stands against what it holds, for the tiles that filter the stock position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockSituation {
    /// Something on hand.
    InStock,
    /// A minimum is set and the quantity is under it (so a product that ran out with a minimum is here too).
    BelowMinimum,
    /// Nothing on hand, or less than nothing.
    Depleted,
}

impl StockSituation {
    /// The key the API sends; it does not change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InStock => "in_stock",
            Self::BelowMinimum => "below_minimum",
            Self::Depleted => "depleted",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "in_stock" => Some(Self::InStock),
            "below_minimum" => Some(Self::BelowMinimum),
            "depleted" => Some(Self::Depleted),
            _ => None,
        }
    }

    /// Whether a product with `minimum` that holds `balance` is in this situation.
    pub fn holds(self, balance: &StockBalance, minimum: Option<Decimal>) -> bool {
        match self {
            Self::InStock => balance.quantity() > Decimal::ZERO,
            Self::BelowMinimum => minimum.is_some_and(|minimum| balance.quantity() < minimum),
            Self::Depleted => balance.quantity() <= Decimal::ZERO,
        }
    }
}

/// What a product has: the sums of its movements. The average cost is derived from them and never stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StockBalance {
    quantity: Decimal,
    /// In cents, like the value of a movement.
    value: Decimal,
}

impl StockBalance {
    pub const EMPTY: Self = Self {
        quantity: Decimal::ZERO,
        value: Decimal::ZERO,
    };

    pub fn new(quantity: Decimal, value: Decimal) -> Self {
        Self {
            quantity: quantity.normalize(),
            value: value.normalize(),
        }
    }

    /// The balance the movements add up to.
    pub fn of<'a>(movements: impl IntoIterator<Item = &'a StockMovement>) -> Self {
        movements
            .into_iter()
            .fold(Self::EMPTY, |balance, movement| balance.after(movement))
    }

    pub fn quantity(&self) -> Decimal {
        self.quantity
    }

    pub fn value(&self) -> Decimal {
        self.value
    }

    /// The balance once `movement` is in.
    pub fn after(&self, movement: &StockMovement) -> Self {
        Self::new(
            self.quantity + movement.quantity,
            self.value + movement.value,
        )
    }

    /// What one unit is worth on average, with six decimals; there is none without something on hand, nor when the
    /// value is below zero.
    pub fn average_cost(&self) -> Option<Decimal> {
        (self.quantity > Decimal::ZERO && self.value >= Decimal::ZERO)
            .then(|| (self.value / self.quantity).round_dp(6).normalize())
    }

    /// The value an exit of `quantity` (above 0) takes: the average cost at this moment, to the cent, and the
    /// exit that takes the whole balance takes all of the value, so a product sold down to zero holds no cents.
    /// From a balance that is zero or negative there is no value to take.
    pub fn exit_value(&self, quantity: Decimal) -> Decimal {
        // Undoing an entry whose goods were sold can leave the value at or below zero with something on hand: the
        // ledger stays exact, and what leaves then takes none, so a movement never has a value of the other sign.
        if self.quantity <= Decimal::ZERO || self.value <= Decimal::ZERO {
            Decimal::ZERO
        } else if quantity >= self.quantity {
            self.value
        } else {
            round_to_cents(self.value * quantity / self.quantity)
        }
    }

    /// The value of `quantity` more of what is on hand, at its average cost; nothing on hand, nothing to value.
    fn average_value(&self, quantity: Decimal) -> Decimal {
        if self.quantity <= Decimal::ZERO || self.value <= Decimal::ZERO {
            Decimal::ZERO
        } else {
            round_to_cents(self.value * quantity / self.quantity)
        }
    }
}

/// Unvalidated input for writing a movement. The quantity is a decimal text: above 0 for an `in` and an `out`
/// (the direction is the kind's), signed for an `adjustment`.
#[derive(Debug, Clone)]
pub struct StockMovementDraft {
    pub product_id: ProductId,
    pub kind: StockMovementKind,
    pub quantity: String,
    /// What an `in` cost in all, in cents. An `in` says this or `unit_cost`; the others say neither: they are
    /// valued from the balance.
    pub value: Option<String>,
    /// What an `in` cost per unit, up to six decimals; the value is the quantity times it, to the cent.
    pub unit_cost: Option<String>,
    pub occurred_on: String,
    pub note: Option<String>,
}

/// One entry of the ledger of a product's stock ([ADR 0052]): a signed quantity and the signed value of what
/// moved, on a day, never edited or deleted. Undoing one writes another that names it.
///
/// Whether the product is one that has stock is the use case's to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockMovement {
    id: StockMovementId,
    company_id: CompanyId,
    product_id: ProductId,
    kind: StockMovementKind,
    quantity: Decimal,
    value: Decimal,
    occurred_on: CalendarDate,
    origin: Option<StockOrigin>,
    reverses: Option<StockMovementId>,
    note: Option<String>,
}

fn parse_quantity(raw: &str, signed: bool) -> Result<Decimal, DomainError> {
    let raw = raw.trim();
    let (negative, digits) = match raw.strip_prefix('-') {
        Some(rest) if signed => (true, rest),
        _ => (false, raw),
    };
    let magnitude = Amount::parse(digits)
        .filter(|quantity| {
            quantity.as_decimal() > Decimal::ZERO && quantity.as_decimal().scale() <= 3
        })
        .ok_or(DomainError::InvalidStockQuantity)?
        .as_decimal();
    Ok(if negative { -magnitude } else { magnitude })
}

impl StockMovement {
    /// A movement of the draft, valued against `balance`, what the product has just before it.
    pub fn new(
        id: StockMovementId,
        company_id: CompanyId,
        draft: StockMovementDraft,
        origin: Option<StockOrigin>,
        balance: &StockBalance,
    ) -> Result<Self, DomainError> {
        let occurred_on = CalendarDate::parse(&draft.occurred_on)
            .ok_or(DomainError::InvalidDate("occurred_on"))?;
        let note = optional_line(
            draft.note.as_deref(),
            MAX_NOTE_CHARS,
            DomainError::InvalidNote,
        )?;
        let magnitude =
            parse_quantity(&draft.quantity, draft.kind == StockMovementKind::Adjustment)?;
        let (quantity, value) = match draft.kind {
            StockMovementKind::In => {
                let value = match (draft.value.as_deref(), draft.unit_cost.as_deref()) {
                    (Some(total), None) => Amount::in_cents(total.trim())
                        .ok_or(DomainError::InvalidAmount("value"))?
                        .as_decimal(),
                    (None, Some(unit)) => round_to_cents(
                        magnitude
                            * Amount::parse(unit.trim())
                                .ok_or(DomainError::InvalidAmount("unit_cost"))?
                                .as_decimal(),
                    ),
                    _ => return Err(DomainError::StockCostRequired),
                };
                (magnitude, value)
            }
            StockMovementKind::Out => {
                refuse_a_cost(&draft)?;
                (-magnitude, -balance.exit_value(magnitude))
            }
            StockMovementKind::Adjustment => {
                refuse_a_cost(&draft)?;
                if magnitude > Decimal::ZERO {
                    (magnitude, balance.average_value(magnitude))
                } else {
                    (magnitude, -balance.exit_value(-magnitude))
                }
            }
        };
        if quantity.abs() >= LIMIT || value.abs() >= LIMIT {
            return Err(DomainError::DocumentTooLarge);
        }
        Ok(Self {
            id,
            company_id,
            product_id: draft.product_id,
            kind: draft.kind,
            quantity: quantity.normalize(),
            value: value.normalize(),
            occurred_on,
            origin,
            reverses: None,
            note,
        })
    }

    /// The movement that undoes `original`: the opposite quantity and the opposite value, so the ledger adds up as
    /// if neither had been written. A reversal is not itself undone.
    pub fn reversing(
        id: StockMovementId,
        original: &StockMovement,
        occurred_on: CalendarDate,
        note: Option<String>,
    ) -> Result<Self, DomainError> {
        if original.reverses.is_some() {
            return Err(DomainError::StockMovementIsAReversal);
        }
        let note = optional_line(note.as_deref(), MAX_NOTE_CHARS, DomainError::InvalidNote)?;
        Ok(Self {
            id,
            company_id: original.company_id,
            product_id: original.product_id,
            kind: match original.kind {
                StockMovementKind::In => StockMovementKind::Out,
                StockMovementKind::Out => StockMovementKind::In,
                StockMovementKind::Adjustment => StockMovementKind::Adjustment,
            },
            quantity: (-original.quantity).normalize(),
            value: (-original.value).normalize(),
            occurred_on,
            origin: original.origin,
            reverses: Some(original.id),
            note,
        })
    }

    /// A movement as it was stored: what it was valued at is a fact of the day it was written.
    #[allow(clippy::too_many_arguments)]
    pub fn restored(
        id: StockMovementId,
        company_id: CompanyId,
        product_id: ProductId,
        kind: StockMovementKind,
        quantity: Decimal,
        value: Decimal,
        occurred_on: CalendarDate,
        origin: Option<StockOrigin>,
        reverses: Option<StockMovementId>,
        note: Option<String>,
    ) -> Self {
        Self {
            id,
            company_id,
            product_id,
            kind,
            quantity: quantity.normalize(),
            value: value.normalize(),
            occurred_on,
            origin,
            reverses,
            note,
        }
    }

    pub fn id(&self) -> StockMovementId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn product_id(&self) -> ProductId {
        self.product_id
    }

    pub fn kind(&self) -> StockMovementKind {
        self.kind
    }

    /// Signed: what an `in` adds is above 0, what an `out` takes is below.
    pub fn quantity(&self) -> Decimal {
        self.quantity
    }

    /// Signed like the quantity, in cents.
    pub fn value(&self) -> Decimal {
        self.value
    }

    pub fn occurred_on(&self) -> CalendarDate {
        self.occurred_on
    }

    pub fn origin(&self) -> Option<StockOrigin> {
        self.origin
    }

    pub fn reverses(&self) -> Option<StockMovementId> {
        self.reverses
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

/// An `out` or an `adjustment` is valued from the balance, never from a figure the person gives.
fn refuse_a_cost(draft: &StockMovementDraft) -> Result<(), DomainError> {
    if draft.value.is_some() || draft.unit_cost.is_some() {
        return Err(DomainError::StockCostNotAllowed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use StockMovementKind::{Adjustment, In, Out};

    fn draft(kind: StockMovementKind, quantity: &str, value: Option<&str>) -> StockMovementDraft {
        StockMovementDraft {
            product_id: ProductId::generate(),
            kind,
            quantity: quantity.into(),
            value: value.map(Into::into),
            unit_cost: None,
            occurred_on: "2026-11-06".into(),
            note: None,
        }
    }

    /// Writes each movement against the balance the ones before it left, as a store does.
    fn ledger(steps: &[(StockMovementKind, &str, Option<&str>)]) -> Vec<StockMovement> {
        let company = CompanyId::generate();
        let mut written: Vec<StockMovement> = Vec::new();
        for (kind, quantity, value) in steps {
            let balance = StockBalance::of(&written);
            written.push(
                StockMovement::new(
                    StockMovementId::generate(),
                    company,
                    draft(*kind, quantity, *value),
                    None,
                    &balance,
                )
                .unwrap(),
            );
        }
        written
    }

    fn state(movements: &[StockMovement]) -> (String, String, Option<String>) {
        let balance = StockBalance::of(movements);
        (
            balance.quantity().to_string(),
            balance.value().to_string(),
            balance.average_cost().map(|cost| cost.to_string()),
        )
    }

    fn values(movements: &[StockMovement]) -> Vec<String> {
        movements.iter().map(|m| m.value().to_string()).collect()
    }

    #[test]
    fn the_worked_example_of_the_average_cost() {
        // 10 bought for 50 (5 each), then 10 for 70 (7 each): 20 worth 120, average 6.
        let mut book = ledger(&[(In, "10", Some("50")), (In, "10", Some("70"))]);
        assert_eq!(state(&book), ("20".into(), "120".into(), Some("6".into())));
        // Selling 5 takes them at the average, 30; the average stays 6.
        book.extend(ledger_after(&book, &[(Out, "5", None)]));
        assert_eq!(values(&book), ["50", "70", "-30"]);
        assert_eq!(state(&book), ("15".into(), "90".into(), Some("6".into())));
        // Selling everything that is left takes all the value: nothing is left behind.
        book.extend(ledger_after(&book, &[(Out, "15", None)]));
        assert_eq!(values(&book)[3], "-90");
        assert_eq!(state(&book), ("0".into(), "0".into(), None));
    }

    /// More movements of the same product and company, written against what `book` already holds.
    fn ledger_after(
        book: &[StockMovement],
        steps: &[(StockMovementKind, &str, Option<&str>)],
    ) -> Vec<StockMovement> {
        let mut all: Vec<StockMovement> = book.to_vec();
        for (kind, quantity, value) in steps {
            let mut next = draft(*kind, quantity, *value);
            next.product_id = book[0].product_id();
            let balance = StockBalance::of(&all);
            all.push(
                StockMovement::new(
                    StockMovementId::generate(),
                    book[0].company_id(),
                    next,
                    None,
                    &balance,
                )
                .unwrap(),
            );
        }
        all.split_off(book.len())
    }

    #[test]
    fn the_cents_of_an_average_that_does_not_divide_are_all_given_out() {
        // 3 bought for 10.00 (3.333333 each): 1 takes 3.33, the next takes half of 6.67 = 3.335 -> 3.34 (halves
        // away from zero), and the last takes what is left, 3.33: the three exits take exactly 10.00.
        let book = ledger(&[
            (In, "3", Some("10")),
            (Out, "1", None),
            (Out, "1", None),
            (Out, "1", None),
        ]);
        assert_eq!(values(&book), ["10", "-3.33", "-3.34", "-3.33"]);
        assert_eq!(state(&book), ("0".into(), "0".into(), None));
    }

    #[test]
    fn an_exit_beyond_the_balance_takes_what_there_is_and_the_rest_costs_nothing() {
        let book = ledger(&[(In, "2", Some("10")), (Out, "5", None), (Out, "1", None)]);
        assert_eq!(values(&book), ["10", "-10", "0"]);
        assert_eq!(state(&book), ("-4".into(), "0".into(), None));
    }

    #[test]
    fn what_leaves_a_balance_whose_value_is_not_above_zero_takes_no_value() {
        // 10 for 100 and 10 for 10; 10 sold take 55; undoing the first entry leaves 0 / -45, then 5 more for 25
        // leave 5 / -20: an exit takes nothing, never a value of the other sign.
        let mut book = ledger(&[
            (In, "10", Some("100")),
            (In, "10", Some("10")),
            (Out, "10", None),
        ]);
        let undo = StockMovement::reversing(
            StockMovementId::generate(),
            &book[0],
            CalendarDate::parse("2026-11-07").unwrap(),
            None,
        )
        .unwrap();
        book.push(undo);
        book.extend(ledger_after(
            &book,
            &[(In, "5", Some("25")), (Out, "1", None)],
        ));
        assert_eq!(values(&book), ["100", "10", "-55", "-100", "25", "0"]);
        assert_eq!(state(&book), ("4".into(), "-20".into(), None));
    }

    #[test]
    fn an_adjustment_up_is_worth_the_average_and_one_down_is_an_exit() {
        let book = ledger(&[
            (In, "4", Some("10")),
            (Adjustment, "2", None),
            (Adjustment, "-3", None),
        ]);
        // 2 more at 2.5 each is 5; 3 fewer out of 6 worth 15 is 7.5.
        assert_eq!(values(&book), ["10", "5", "-7.5"]);
        assert_eq!(state(&book), ("3".into(), "7.5".into(), Some("2.5".into())));
        // With nothing on hand, there is no average to value an adjustment up with.
        let empty = ledger(&[(Adjustment, "2", None)]);
        assert_eq!(empty[0].value().to_string(), "0");
    }

    #[test]
    fn an_in_is_valued_by_its_total_or_by_its_unit_cost_to_the_cent() {
        let make = |value: Option<&str>, unit: Option<&str>| {
            let mut entering = draft(In, "3", value);
            entering.unit_cost = unit.map(Into::into);
            StockMovement::new(
                StockMovementId::generate(),
                CompanyId::generate(),
                entering,
                None,
                &StockBalance::EMPTY,
            )
        };
        assert_eq!(
            make(Some("10.5"), None).unwrap().value().to_string(),
            "10.5"
        );
        // 3 x 3.3333 = 9.9999, which rounds to 10.
        assert_eq!(
            make(None, Some("3.3333")).unwrap().value().to_string(),
            "10"
        );
        assert_eq!(
            make(None, None).unwrap_err(),
            DomainError::StockCostRequired
        );
        assert_eq!(
            make(Some("1"), Some("1")).unwrap_err(),
            DomainError::StockCostRequired
        );
        assert_eq!(
            make(Some("1.005"), None).unwrap_err(),
            DomainError::InvalidAmount("value")
        );
    }

    #[test]
    fn what_cannot_be_a_quantity_or_a_cost_is_refused() {
        let attempt = |kind, quantity: &str, value: Option<&str>| {
            StockMovement::new(
                StockMovementId::generate(),
                CompanyId::generate(),
                draft(kind, quantity, value),
                None,
                &StockBalance::EMPTY,
            )
            .unwrap_err()
        };
        for (kind, quantity) in [
            (In, "0"),
            (In, "-1"),
            (Out, "-1"),
            (Out, "1.0001"),
            (Out, "x"),
            (Adjustment, "0"),
            (Adjustment, "--1"),
            (Adjustment, "1e2"),
        ] {
            assert_eq!(
                attempt(kind, quantity, Some("1")),
                DomainError::InvalidStockQuantity,
                "{kind:?} {quantity}"
            );
        }
        // Only an `in` is given a cost.
        assert_eq!(
            attempt(Out, "1", Some("1")),
            DomainError::StockCostNotAllowed
        );
        assert_eq!(
            attempt(Adjustment, "1", Some("1")),
            DomainError::StockCostNotAllowed
        );
    }

    #[test]
    fn a_reversal_is_the_opposite_in_quantity_and_value_and_is_not_itself_undone() {
        let book = ledger(&[(In, "10", Some("50")), (Out, "4", None)]);
        let day = CalendarDate::parse("2026-11-07").unwrap();
        let undo =
            StockMovement::reversing(StockMovementId::generate(), &book[1], day, None).unwrap();
        assert_eq!(undo.kind(), In);
        assert_eq!(
            (undo.quantity().to_string(), undo.value().to_string()),
            ("4".into(), "20".into())
        );
        assert_eq!(undo.reverses(), Some(book[1].id()));
        let mut all = book.clone();
        all.push(undo.clone());
        assert_eq!(state(&all), ("10".into(), "50".into(), Some("5".into())));
        assert_eq!(
            StockMovement::reversing(StockMovementId::generate(), &undo, day, None).unwrap_err(),
            DomainError::StockMovementIsAReversal
        );
        let of_in =
            StockMovement::reversing(StockMovementId::generate(), &book[0], day, None).unwrap();
        assert_eq!(of_in.kind(), Out);
    }
}
