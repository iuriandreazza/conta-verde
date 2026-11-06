use std::fmt;
use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};

/// The one rounding rule of sales (ADR 0037): to the cent, halves away from zero. A quantity times a
/// price, a rate of a commission and a percentage of an adjustment go through it; splitting an amount
/// into installments does not (the remainder goes to the last one).
pub fn round_to_cents(value: Decimal) -> Decimal {
    value
        .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
        .normalize()
}

// What fits `NUMERIC(19, 6)`, the column for unit prices and quantities (ADR 0003).
const MAX_INTEGER_DIGITS: usize = 13;
const MAX_FRACTION_DIGITS: usize = 6;

/// A non-negative decimal used for unit prices and quantities.
///
/// Parsing is strict on purpose: no sign, no exponent, no separators and no more than six decimals.
/// A value with more precision is refused, never rounded, because every rounding point is a named
/// rule of its own (ADR 0003). The value is kept without trailing zeros, so it reads the same
/// whichever adapter it came from: Postgres hands back `10.500000` for what was stored as `10.5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amount(Decimal);

impl Amount {
    pub const ZERO: Self = Self(Decimal::ZERO);

    /// An amount in cents, which is what money in Brazil is paid in: at most two decimals, never
    /// rounded. `10` and `10.50` are, `10.505` is not.
    pub fn in_cents(raw: &str) -> Option<Self> {
        Self::parse(raw).filter(|amount| amount.0.scale() <= 2)
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let (integer, fraction) = match raw.split_once('.') {
            Some((integer, fraction)) => (integer, Some(fraction)),
            None => (raw, None),
        };
        let digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
        let integer_fits =
            !integer.is_empty() && integer.len() <= MAX_INTEGER_DIGITS && digits(integer);
        let fraction_fits = fraction.is_none_or(|fraction| {
            !fraction.is_empty() && fraction.len() <= MAX_FRACTION_DIGITS && digits(fraction)
        });
        if !(integer_fits && fraction_fits) {
            return None;
        }
        Decimal::from_str(raw)
            .ok()
            .map(|value| Self(value.normalize()))
    }

    pub fn as_decimal(&self) -> Decimal {
        self.0
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A decimal that may be negative, in cents: an opening balance, which a card or an account in
/// overdraft starts below zero. Written without trailing zeros, like [`Amount`]; `-0` is `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedAmount(Decimal);

impl SignedAmount {
    pub const ZERO: Self = Self(Decimal::ZERO);

    /// At most two decimals and thirteen digits before the point, with an optional leading `-` and
    /// nothing else (`+1` and `- 1` are refused).
    pub fn in_cents(raw: &str) -> Option<Self> {
        let (negative, digits) = match raw.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, raw),
        };
        let magnitude = Amount::in_cents(digits)?.0;
        Some(Self(
            if negative { -magnitude } else { magnitude }.normalize(),
        ))
    }

    pub fn as_decimal(&self) -> Decimal {
        self.0
    }
}

impl fmt::Display for SignedAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(raw: &str) -> Option<String> {
        Amount::parse(raw).map(|amount| amount.to_string())
    }

    #[test]
    fn rounds_to_the_cent_with_halves_going_up() {
        let rounded = |raw: &str| round_to_cents(Decimal::from_str(raw).unwrap()).to_string();
        assert_eq!(rounded("4.995"), "5");
        assert_eq!(rounded("4.994999"), "4.99");
        assert_eq!(rounded("0.005"), "0.01");
        assert_eq!(rounded("2.5000"), "2.5");
        assert_eq!(rounded("10"), "10");
    }

    #[test]
    fn accepts_plain_decimals_up_to_the_column_precision() {
        assert_eq!(text("10").as_deref(), Some("10"));
        assert_eq!(text("10.5").as_deref(), Some("10.5"));
        assert_eq!(text("0.000001").as_deref(), Some("0.000001"));
        assert_eq!(
            text("9999999999999.999999").as_deref(),
            Some("9999999999999.999999")
        );
    }

    #[test]
    fn keeps_no_trailing_zeros_so_every_adapter_reads_the_same_text() {
        assert_eq!(text("10.500000").as_deref(), Some("10.5"));
        assert_eq!(text("007").as_deref(), Some("7"));
        assert_eq!(text("0.00").as_deref(), Some("0"));
        assert_eq!(Amount::parse("10.50"), Amount::parse("10.5"));
    }

    #[test]
    fn refuses_anything_that_is_not_a_plain_non_negative_decimal() {
        for raw in [
            "", " ", "-1", "+1", "1e3", "1E3", "1_000", "1,5", " 1", "1 ", ".5", "5.", "1..5",
            "NaN", "inf", "0x10", "١٢",
        ] {
            assert_eq!(Amount::parse(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn refuses_what_would_not_fit_instead_of_rounding_it() {
        assert_eq!(Amount::parse("1.1234567"), None);
        assert_eq!(Amount::parse("10000000000000"), None);
    }
}
