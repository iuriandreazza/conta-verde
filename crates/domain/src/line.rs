use rust_decimal::Decimal;

use crate::financial_entry::clean_description;
use crate::{Amount, DomainError, ProductId, round_to_cents};

/// How many lines a quote, a sale or a contract has: a bound, not a rule.
pub const MAX_LINES: usize = 100;

/// What an entry's amount column holds: a document may not be worth more.
const LIMIT: Decimal = Decimal::from_parts(1_316_134_912, 2_328, 0, false, 0);

/// One line as it is asked for: unvalidated, the numbers as decimal text.
#[derive(Debug, Clone)]
pub struct LineDraft {
    pub product_id: ProductId,
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub discount: String,
}

/// A product or service sold, in some quantity at a unit price, with a discount ([ADR 0037]): a quote,
/// a sale and a contract are made of them. The product's name is copied as the description, so the
/// document reads the same after the product is renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    product_id: ProductId,
    description: String,
    quantity: Amount,
    unit_price: Amount,
    discount: Amount,
}

impl Line {
    pub fn new(draft: &LineDraft) -> Result<Self, DomainError> {
        let description = clean_description(&draft.description)?;
        let quantity = Amount::parse(draft.quantity.trim())
            .filter(|quantity| {
                quantity.as_decimal() > Decimal::ZERO && quantity.as_decimal().scale() <= 3
            })
            .ok_or(DomainError::InvalidLineQuantity)?;
        let unit_price = Amount::parse(draft.unit_price.trim())
            .ok_or(DomainError::InvalidAmount("unit_price"))?;
        let discount = Amount::in_cents(draft.discount.trim())
            .ok_or(DomainError::InvalidAmount("discount"))?;
        let line = Self {
            product_id: draft.product_id,
            description,
            quantity,
            unit_price,
            discount,
        };
        if discount.as_decimal() > line.gross() {
            return Err(DomainError::DiscountExceedsLine);
        }
        if line.gross() >= LIMIT {
            return Err(DomainError::DocumentTooLarge);
        }
        Ok(line)
    }

    pub fn product_id(&self) -> ProductId {
        self.product_id
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn quantity(&self) -> Amount {
        self.quantity
    }

    pub fn unit_price(&self) -> Amount {
        self.unit_price
    }

    pub fn discount(&self) -> Amount {
        self.discount
    }

    /// Quantity times unit price, rounded to the cent.
    pub fn gross(&self) -> Decimal {
        round_to_cents(self.quantity.as_decimal() * self.unit_price.as_decimal())
    }

    /// What the line is worth: its gross less the discount. It is never negative.
    pub fn total(&self) -> Decimal {
        (self.gross() - self.discount.as_decimal()).normalize()
    }
}

/// The lines of a document: at least one and at most [`MAX_LINES`], and a total that fits.
pub(crate) fn validated_lines(drafts: &[LineDraft]) -> Result<Vec<Line>, DomainError> {
    if drafts.is_empty() {
        return Err(DomainError::NoLines);
    }
    if drafts.len() > MAX_LINES {
        return Err(DomainError::TooManyLines);
    }
    let lines = drafts
        .iter()
        .map(Line::new)
        .collect::<Result<Vec<_>, _>>()?;
    if lines_total(&lines) >= LIMIT {
        return Err(DomainError::DocumentTooLarge);
    }
    Ok(lines)
}

/// The sum of the lines, exact; a document's total is never stored.
pub fn lines_total(lines: &[Line]) -> Decimal {
    lines.iter().map(Line::total).sum::<Decimal>().normalize()
}

/// The lines in one canonical text, `product:quantity:unit_price:discount` separated by commas, which
/// is what the audit trail records: the same lines always read the same.
pub(crate) fn lines_text(lines: &[Line]) -> String {
    lines
        .iter()
        .map(|line| {
            format!(
                "{}:{}:{}:{}",
                line.product_id.as_uuid(),
                line.quantity,
                line.unit_price,
                line.discount
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(quantity: &str, unit_price: &str, discount: &str) -> LineDraft {
        LineDraft {
            product_id: ProductId::generate(),
            description: " Consultoria ".into(),
            quantity: quantity.into(),
            unit_price: unit_price.into(),
            discount: discount.into(),
        }
    }

    #[test]
    fn a_line_is_worth_its_rounded_gross_less_the_discount() {
        let line = Line::new(&draft("1.5", "3.33", "0.5")).unwrap();
        assert_eq!(line.description(), "Consultoria");
        // 1.5 x 3.33 = 4.995, which rounds up to 5.
        assert_eq!(line.gross().to_string(), "5");
        assert_eq!(line.total().to_string(), "4.5");
    }

    #[test]
    fn refuses_what_is_not_a_quantity_a_price_or_a_discount() {
        for (quantity, price, discount, error) in [
            ("0", "1", "0", DomainError::InvalidLineQuantity),
            ("1.0001", "1", "0", DomainError::InvalidLineQuantity),
            ("-1", "1", "0", DomainError::InvalidLineQuantity),
            ("1", "x", "0", DomainError::InvalidAmount("unit_price")),
            ("1", "1", "0.001", DomainError::InvalidAmount("discount")),
            ("1", "10", "10.01", DomainError::DiscountExceedsLine),
        ] {
            assert_eq!(
                Line::new(&draft(quantity, price, discount)).unwrap_err(),
                error,
                "{quantity} {price} {discount}"
            );
        }
        assert_eq!(
            Line::new(&draft("1", "10", "10"))
                .unwrap()
                .total()
                .to_string(),
            "0"
        );
    }

    #[test]
    fn a_document_has_between_one_and_a_hundred_lines_that_fit() {
        assert_eq!(validated_lines(&[]).unwrap_err(), DomainError::NoLines);
        let many = vec![draft("1", "1", "0"); MAX_LINES + 1];
        assert_eq!(
            validated_lines(&many).unwrap_err(),
            DomainError::TooManyLines
        );
        let huge = vec![draft("1", "9999999999999", "0"); 2];
        assert_eq!(
            validated_lines(&huge).unwrap_err(),
            DomainError::DocumentTooLarge
        );
        let lines = validated_lines(&[draft("2", "10.10", "0.2"), draft("1", "5", "0")]).unwrap();
        assert_eq!(lines_total(&lines).to_string(), "25");
    }
}
