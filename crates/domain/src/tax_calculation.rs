use rust_decimal::Decimal;

use crate::{Amount, DomainError, TaxRate, TaxRule, round_to_cents};

/// What one tax comes to on one item: the rate the company's rule gives and the amount that rate makes of the base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaxAmount {
    pub rate: TaxRate,
    pub amount: Amount,
}

/// The taxes of one item, worked out from the company's own rule ([ADR 0084]).
///
/// The product embeds **no tax law**: the base is the item's value less its discount (an assumption an accountant must
/// confirm, see `docs/fiscal/accountant-checklist.md`), the rates are the ones the company wrote in its rule, and a tax the rule
/// gives no rate for is **not calculated** (`None`), never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemTax {
    /// The value of the item less its discount, in cents.
    pub base: Amount,
    pub icms: Option<TaxAmount>,
    pub pis: Option<TaxAmount>,
    pub cofins: Option<TaxAmount>,
}

/// What an invoice's items come to: each tax is the **sum of the items' rounded amounts**, never the rounding of the sum, so the
/// total is what the items print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentTax {
    pub base: Amount,
    pub icms: Amount,
    pub pis: Amount,
    pub cofins: Amount,
}

/// The taxes of an item whose value is `gross` and whose discount is `discount` (both in cents, the discount not above the value),
/// under `rule`. Each amount is `base x rate / 100` rounded to the cent, halves away from zero, which is the one rounding rule of
/// the product (`round_to_cents`); a rate of 0 is an amount of 0, and no rate is no amount.
pub fn item_tax(rule: &TaxRule, gross: Amount, discount: Amount) -> Result<ItemTax, DomainError> {
    let in_cents = |amount: Amount| amount.as_decimal().scale() <= 2;
    if !in_cents(gross) || !in_cents(discount) || discount.as_decimal() > gross.as_decimal() {
        return Err(DomainError::InvalidTaxCalculation);
    }
    let base = Amount::parse(
        &(gross.as_decimal() - discount.as_decimal())
            .normalize()
            .to_string(),
    )
    .ok_or(DomainError::InvalidTaxCalculation)?;
    let tax = |rate: Option<TaxRate>| -> Result<Option<TaxAmount>, DomainError> {
        rate.map(|rate| {
            let amount =
                round_to_cents(base.as_decimal() * rate.as_decimal() / Decimal::ONE_HUNDRED);
            Amount::parse(&amount.to_string())
                .map(|amount| TaxAmount { rate, amount })
                .ok_or(DomainError::InvalidTaxCalculation)
        })
        .transpose()
    };
    Ok(ItemTax {
        base,
        icms: tax(rule.icms_rate())?,
        pis: tax(rule.pis_rate())?,
        cofins: tax(rule.cofins_rate())?,
    })
}

/// The totals of the items' taxes: sums of what each item came to (a tax with no rate on an item adds nothing).
pub fn document_tax(items: &[ItemTax]) -> Result<DocumentTax, DomainError> {
    let sum = |amount: &dyn Fn(&ItemTax) -> Option<Amount>| -> Result<Amount, DomainError> {
        let total: Decimal = items
            .iter()
            .filter_map(amount)
            .map(|amount| amount.as_decimal())
            .sum();
        Amount::parse(&total.normalize().to_string()).ok_or(DomainError::InvalidTaxCalculation)
    };
    Ok(DocumentTax {
        base: sum(&|item| Some(item.base))?,
        icms: sum(&|item| item.icms.map(|tax| tax.amount))?,
        pis: sum(&|item| item.pis.map(|tax| tax.amount))?,
        cofins: sum(&|item| item.cofins.map(|tax| tax.amount))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TaxRegime, TaxRuleDraft};

    fn rule(icms: Option<&str>, pis: Option<&str>, cofins: Option<&str>) -> TaxRule {
        TaxRule::new(&TaxRuleDraft {
            regime: TaxRegime::LucroPresumido,
            ncm_prefix: None,
            icms_code: "00".into(),
            pis_code: "01".into(),
            cofins_code: "01".into(),
            icms_rate: icms.map(str::to_owned),
            pis_rate: pis.map(str::to_owned),
            cofins_rate: cofins.map(str::to_owned),
        })
        .unwrap()
    }

    fn cents(raw: &str) -> Amount {
        Amount::in_cents(raw).unwrap()
    }

    fn amounts(tax: &ItemTax) -> (Option<String>, Option<String>, Option<String>) {
        let text = |tax: Option<TaxAmount>| tax.map(|tax| tax.amount.as_decimal().to_string());
        (text(tax.icms), text(tax.pis), text(tax.cofins))
    }

    /// An independent reference in whole cents and basis points, with no decimal type: what an accountant would work by hand.
    fn reference(base_cents: i128, rate_hundredths: i128) -> i128 {
        // base (cents) x rate (hundredths of a percent) / 10_000, halves away from zero.
        let numerator = base_cents * rate_hundredths;
        (numerator + 5_000) / 10_000
    }

    #[test]
    fn each_tax_is_the_base_times_the_rate_rounded_to_the_cent_halves_up() {
        let tax = item_tax(
            &rule(Some("18"), Some("0.65"), Some("3")),
            cents("100.00"),
            cents("0"),
        )
        .unwrap();
        assert_eq!(tax.base, cents("100"));
        assert_eq!(
            amounts(&tax),
            (Some("18".into()), Some("0.65".into()), Some("3".into()))
        );
        // The half of a cent goes up: 10.05 x 5% is 0.5025, 0.5025 -> 0.50; 0.30 x 1.65% is 0.00495 -> 0.00;
        // 0.50 x 1% is 0.005 -> 0.01.
        let tax = item_tax(
            &rule(Some("5"), Some("1.65"), Some("1")),
            cents("10.05"),
            cents("0"),
        )
        .unwrap();
        assert_eq!(amounts(&tax).0.as_deref(), Some("0.5"));
        let tax = item_tax(
            &rule(None, Some("1.65"), Some("1")),
            cents("0.50"),
            cents("0"),
        )
        .unwrap();
        assert_eq!(
            amounts(&tax).2.as_deref(),
            Some("0.01"),
            "0.005 is a half and goes up"
        );
        let tax = item_tax(&rule(None, Some("1.65"), None), cents("0.30"), cents("0")).unwrap();
        assert_eq!(
            amounts(&tax).1.as_deref(),
            Some("0"),
            "0.00495 is below a half"
        );
    }

    #[test]
    fn the_base_is_the_value_less_the_discount_and_a_discount_above_it_is_refused() {
        let tax = item_tax(&rule(Some("10"), None, None), cents("200"), cents("50.50")).unwrap();
        assert_eq!(tax.base, cents("149.50"));
        assert_eq!(amounts(&tax).0.as_deref(), Some("14.95"));
        let all = item_tax(&rule(Some("10"), None, None), cents("50"), cents("50")).unwrap();
        assert_eq!(all.base, Amount::ZERO);
        assert_eq!(amounts(&all).0.as_deref(), Some("0"));
        assert!(item_tax(&rule(Some("10"), None, None), cents("50"), cents("50.01")).is_err());
        // An amount in fractions of a cent is not an invoice's: it is refused, never rounded here.
        assert!(
            item_tax(
                &rule(Some("10"), None, None),
                Amount::parse("10.005").unwrap(),
                cents("0")
            )
            .is_err()
        );
    }

    #[test]
    fn no_rate_is_no_amount_and_a_rate_of_zero_is_an_amount_of_zero() {
        let tax = item_tax(&rule(None, Some("0"), None), cents("100"), cents("0")).unwrap();
        assert_eq!(tax.icms, None);
        assert_eq!(tax.cofins, None);
        assert_eq!(amounts(&tax).1.as_deref(), Some("0"));
        let all = item_tax(
            &rule(Some("100"), Some("100"), Some("100")),
            cents("33.33"),
            cents("0"),
        )
        .unwrap();
        assert_eq!(
            amounts(&all),
            (
                Some("33.33".into()),
                Some("33.33".into()),
                Some("33.33".into())
            )
        );
    }

    #[test]
    fn a_document_totals_what_its_items_came_to_not_the_rounding_of_the_sum() {
        // Three items of 0.50 at 1%: each is 0.005 -> 0.01, so the document is 0.03; rounding the sum (0.015) would be 0.02.
        let one = item_tax(&rule(Some("1"), None, None), cents("0.50"), cents("0")).unwrap();
        let total = document_tax(&[one, one, one]).unwrap();
        assert_eq!(total.icms, cents("0.03"));
        assert_eq!(total.base, cents("1.5"));
        assert_eq!(total.pis, Amount::ZERO);
        assert_eq!(document_tax(&[]).unwrap().icms, Amount::ZERO);
        // A tax one item has no rate for adds nothing for it.
        let other = item_tax(&rule(None, Some("2"), None), cents("10"), cents("0")).unwrap();
        let mixed = document_tax(&[one, other]).unwrap();
        assert_eq!((mixed.icms, mixed.pis), (cents("0.01"), cents("0.2")));
    }

    #[test]
    fn it_agrees_with_an_integer_reference_over_many_values_and_rates() {
        let rates = [
            "0", "0.01", "0.65", "1.65", "3", "7.6", "12", "17.5", "18", "25.99", "99.99", "100",
        ];
        let mut checked = 0;
        for gross in (0..=20_000i128)
            .step_by(37)
            .chain([9_999_999_999, 1, 5, 50, 995])
        {
            for discount_cents in [0, 1, 5, 333] {
                if discount_cents > gross {
                    continue;
                }
                for rate in rates {
                    let tax = item_tax(
                        &rule(Some(rate), Some(rate), Some(rate)),
                        cents(&format!("{}.{:02}", gross / 100, gross % 100)),
                        cents(&format!(
                            "{}.{:02}",
                            discount_cents / 100,
                            discount_cents % 100
                        )),
                    )
                    .unwrap();
                    let hundredths = {
                        let (whole, fraction) = rate.split_once('.').unwrap_or((rate, ""));
                        whole.parse::<i128>().unwrap() * 100
                            + format!("{fraction:0<2}").parse::<i128>().unwrap()
                    };
                    let expected = reference(gross - discount_cents, hundredths);
                    let got = tax.icms.unwrap().amount.as_decimal() * Decimal::ONE_HUNDRED;
                    assert_eq!(
                        got,
                        Decimal::from(expected),
                        "{gross} - {discount_cents} at {rate}%"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 5_000);
    }
}
