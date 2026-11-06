use rust_decimal::Decimal;

use crate::{
    CalendarDate, CommissionRuleId, CompanyId, DomainError, PartyId, Percent, ProductId, Sale,
    SaleReturn, SaleStatus, net_of_lines, round_to_cents,
};

/// Unvalidated input for registering or changing a commission rule.
#[derive(Debug, Clone)]
pub struct CommissionRuleDraft {
    /// `None` is every seller.
    pub seller_id: Option<PartyId>,
    /// `None` is every product.
    pub product_id: Option<ProductId>,
    /// A percentage above 0 and at most 100, with at most two decimals.
    pub rate: String,
    pub starts_on: String,
    pub ends_on: Option<String>,
    pub active: bool,
}

/// What a seller earns on what is sold, as a percentage of the net value of the lines ([ADR 0041]): for one
/// seller or for all, for one product or for all, on the sales made from `starts_on` through `ends_on`. The
/// days are what keeps a statement from changing under a rule that was changed later: a new rate is a new rule
/// that starts later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommissionRule {
    id: CommissionRuleId,
    company_id: CompanyId,
    seller_id: Option<PartyId>,
    product_id: Option<ProductId>,
    rate: Percent,
    starts_on: CalendarDate,
    ends_on: Option<CalendarDate>,
    active: bool,
}

impl CommissionRule {
    pub fn new(
        id: CommissionRuleId,
        company_id: CompanyId,
        draft: CommissionRuleDraft,
    ) -> Result<Self, DomainError> {
        let rate = Percent::parse(draft.rate.trim()).ok_or(DomainError::InvalidPercent)?;
        let starts_on =
            CalendarDate::parse(&draft.starts_on).ok_or(DomainError::InvalidDate("starts_on"))?;
        let ends_on = draft
            .ends_on
            .as_deref()
            .map(|raw| CalendarDate::parse(raw).ok_or(DomainError::InvalidDate("ends_on")))
            .transpose()?;
        if ends_on.is_some_and(|end| end < starts_on) {
            return Err(DomainError::RecurrenceEndsBeforeItStarts);
        }
        Ok(Self {
            id,
            company_id,
            seller_id: draft.seller_id,
            product_id: draft.product_id,
            rate,
            starts_on,
            ends_on,
            active: draft.active,
        })
    }

    /// The same rule with every editable field replaced by the draft's.
    pub fn replaced_with(&self, draft: CommissionRuleDraft) -> Result<Self, DomainError> {
        Self::new(self.id, self.company_id, draft)
    }

    pub fn id(&self) -> CommissionRuleId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn seller_id(&self) -> Option<PartyId> {
        self.seller_id
    }

    pub fn product_id(&self) -> Option<ProductId> {
        self.product_id
    }

    pub fn rate(&self) -> Percent {
        self.rate
    }

    pub fn starts_on(&self) -> CalendarDate {
        self.starts_on
    }

    pub fn ends_on(&self) -> Option<CalendarDate> {
        self.ends_on
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Whether the rule covers a sale of this product by this seller on this day.
    fn covers(&self, seller: PartyId, product: ProductId, on: CalendarDate) -> bool {
        self.active
            && self.seller_id.is_none_or(|own| own == seller)
            && self.product_id.is_none_or(|own| own == product)
            && self.starts_on <= on
            && self.ends_on.is_none_or(|end| on <= end)
    }

    /// How narrow the rule is: one for a seller and one for a product, so that a rule for this seller and this
    /// product beats one for the seller only, which beats one for the product only, which beats the general one.
    fn specificity(&self) -> (bool, bool) {
        (self.seller_id.is_some(), self.product_id.is_some())
    }
}

/// The rule that applies to a sale of a product by a seller on a day: the narrowest that covers it, and among
/// equals the one that started last, then the one with the greater id, so the answer never depends on the
/// order the rules were read in.
pub fn applicable_rule(
    rules: &[CommissionRule],
    seller: PartyId,
    product: ProductId,
    on: CalendarDate,
) -> Option<&CommissionRule> {
    rules
        .iter()
        .filter(|rule| rule.covers(seller, product, on))
        .max_by_key(|rule| (rule.specificity(), rule.starts_on, rule.id.as_uuid()))
}

/// What a completed sale earns its seller: the net value of its lines less what was returned of each, and the
/// commission on it, each line at the rate of the rule that applies to it, rounded to the cent line by line
/// ([ADR 0037]). A line that no rule covers earns nothing but still counts in the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleCommission {
    pub base: Decimal,
    pub commission: Decimal,
}

/// `None` for a sale that is not completed or has no seller.
pub fn commission_of_sale(
    sale: &Sale,
    returns: &[SaleReturn],
    rules: &[CommissionRule],
) -> Option<SaleCommission> {
    let seller = sale.seller_id()?;
    if sale.status() != SaleStatus::Completed {
        return None;
    }
    let mut base = Decimal::ZERO;
    let mut commission = Decimal::ZERO;
    for (line, net) in sale.lines().iter().zip(net_of_lines(sale, returns)) {
        base += net;
        if let Some(rule) = applicable_rule(rules, seller, line.product_id(), sale.sold_on()) {
            commission += round_to_cents(net * rule.rate().as_decimal() / Decimal::ONE_HUNDRED);
        }
    }
    Some(SaleCommission {
        base: base.normalize(),
        commission: commission.normalize(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CategoryId, LineDraft, ReturnItemDraft, SaleDraft, SaleId, SaleReturnDraft, SaleReturnId,
    };

    fn day(text: &str) -> CalendarDate {
        CalendarDate::parse(text).unwrap()
    }

    fn rule(
        seller: Option<PartyId>,
        product: Option<ProductId>,
        rate: &str,
        starts_on: &str,
        ends_on: Option<&str>,
    ) -> CommissionRule {
        CommissionRule::new(
            CommissionRuleId::generate(),
            CompanyId::generate(),
            CommissionRuleDraft {
                seller_id: seller,
                product_id: product,
                rate: rate.into(),
                starts_on: starts_on.into(),
                ends_on: ends_on.map(Into::into),
                active: true,
            },
        )
        .unwrap()
    }

    #[test]
    fn a_rule_is_a_rate_between_two_days_for_a_seller_and_a_product_or_for_all() {
        for bad in ["0", "100.01", "abc", "", "1.234"] {
            let draft = CommissionRuleDraft {
                seller_id: None,
                product_id: None,
                rate: bad.into(),
                starts_on: "2026-01-01".into(),
                ends_on: None,
                active: true,
            };
            assert_eq!(
                CommissionRule::new(CommissionRuleId::generate(), CompanyId::generate(), draft)
                    .unwrap_err(),
                DomainError::InvalidPercent,
                "{bad:?}"
            );
        }
        let draft = CommissionRuleDraft {
            seller_id: None,
            product_id: None,
            rate: "5".into(),
            starts_on: "2026-02-01".into(),
            ends_on: Some("2026-01-31".into()),
            active: true,
        };
        assert_eq!(
            CommissionRule::new(CommissionRuleId::generate(), CompanyId::generate(), draft)
                .unwrap_err(),
            DomainError::RecurrenceEndsBeforeItStarts
        );
    }

    #[test]
    fn the_narrowest_rule_that_covers_the_day_wins_and_the_later_start_breaks_a_tie() {
        let (vera, bia) = (PartyId::generate(), PartyId::generate());
        let (coffee, bread) = (ProductId::generate(), ProductId::generate());
        let everyone = rule(None, None, "2", "2026-01-01", None);
        let for_coffee = rule(None, Some(coffee), "3", "2026-01-01", None);
        let for_vera = rule(Some(vera), None, "4", "2026-01-01", None);
        let for_both = rule(Some(vera), Some(coffee), "6", "2026-01-01", None);
        let newer = rule(Some(vera), Some(coffee), "7", "2026-06-01", None);
        let mut inactive = rule(Some(vera), Some(bread), "50", "2026-01-01", None);
        inactive = inactive
            .replaced_with(CommissionRuleDraft {
                seller_id: Some(vera),
                product_id: Some(bread),
                rate: "50".into(),
                starts_on: "2026-01-01".into(),
                ends_on: None,
                active: false,
            })
            .unwrap();
        let rules = vec![
            everyone.clone(),
            for_coffee.clone(),
            for_vera.clone(),
            for_both.clone(),
            newer.clone(),
            inactive,
        ];
        let rate = |seller, product, on: &str| {
            applicable_rule(&rules, seller, product, day(on)).map(|rule| rule.rate().to_string())
        };
        // This seller and this product, the one that started last once it has started.
        assert_eq!(rate(vera, coffee, "2026-03-01").as_deref(), Some("6"));
        assert_eq!(rate(vera, coffee, "2026-06-01").as_deref(), Some("7"));
        // The seller's, then the product's, then everyone's; never an inactive one or one that has not begun.
        assert_eq!(rate(vera, bread, "2026-03-01").as_deref(), Some("4"));
        assert_eq!(rate(bia, coffee, "2026-03-01").as_deref(), Some("3"));
        assert_eq!(rate(bia, bread, "2026-03-01").as_deref(), Some("2"));
        assert_eq!(rate(bia, bread, "2025-12-31"), None);
        // A rule that ended no longer covers the days after it.
        let ended = vec![rule(None, None, "9", "2026-01-01", Some("2026-01-31"))];
        assert!(applicable_rule(&ended, vera, coffee, day("2026-01-31")).is_some());
        assert!(applicable_rule(&ended, vera, coffee, day("2026-02-01")).is_none());
    }

    fn sale_with(seller: Option<PartyId>, lines: Vec<LineDraft>) -> Sale {
        Sale::new(
            SaleId::generate(),
            CompanyId::generate(),
            SaleDraft {
                customer_id: PartyId::generate(),
                seller_id: seller,
                sold_on: "2026-03-10".into(),
                payment_term_id: None,
                category_id: CategoryId::generate(),
                cost_center_id: None,
                notes: None,
                lines,
            },
        )
        .unwrap()
        .completed()
        .unwrap()
    }

    fn line(product: ProductId, quantity: &str, price: &str, discount: &str) -> LineDraft {
        LineDraft {
            product_id: product,
            description: "Item".into(),
            quantity: quantity.into(),
            unit_price: price.into(),
            discount: discount.into(),
        }
    }

    #[test]
    fn a_sale_earns_its_seller_line_by_line_net_of_discounts_and_returns() {
        let vera = PartyId::generate();
        let (coffee, bread) = (ProductId::generate(), ProductId::generate());
        let rules = vec![
            rule(None, None, "10", "2026-01-01", None),
            rule(Some(vera), Some(coffee), "5.5", "2026-01-01", None),
        ];
        // 3 x 10.10 less 0.30 is 30 for coffee, 1.5 x 3.33 rounds to 5 for bread.
        let sale = sale_with(
            Some(vera),
            vec![
                line(coffee, "3", "10.10", "0.30"),
                line(bread, "1.5", "3.33", "0"),
            ],
        );
        let earned = commission_of_sale(&sale, &[], &rules).unwrap();
        // 5.5% of 30 is 1.65 and 10% of 5 is 0.5.
        assert_eq!(
            (earned.base.to_string(), earned.commission.to_string()),
            ("35".to_owned(), "2.15".to_owned())
        );
        // One of the three coffees came back: its refund (10) comes off the base before the rate is applied.
        let made = SaleReturn::new(
            SaleReturnId::generate(),
            &sale,
            SaleReturnDraft {
                returned_on: "2026-03-12".into(),
                reason: None,
                items: vec![ReturnItemDraft {
                    line: 1,
                    quantity: "1".into(),
                }],
            },
            &[],
        )
        .unwrap();
        let after = commission_of_sale(&sale, &[made], &rules).unwrap();
        // 20 for coffee at 5.5% is 1.1, and bread as before.
        assert_eq!(
            (after.base.to_string(), after.commission.to_string()),
            ("25".to_owned(), "1.6".to_owned())
        );
        // A line no rule covers earns nothing but counts in the base; a sale without a seller earns none.
        let none = commission_of_sale(&sale, &[], &[]).unwrap();
        assert_eq!(
            (none.base.to_string(), none.commission.to_string()),
            ("35".to_owned(), "0".to_owned())
        );
        assert!(
            commission_of_sale(
                &sale_with(None, vec![line(coffee, "1", "1", "0")]),
                &[],
                &rules
            )
            .is_none()
        );
    }
}
