use std::collections::BTreeMap;

use rust_decimal::{Decimal, RoundingStrategy};
use uuid::Uuid;

use crate::{Sale, SaleReturn, SaleStatus, round_to_cents};

/// What a sales report adds up by ([ADR 0043]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesGroup {
    Customer,
    Product,
    Seller,
}

/// The ABC curve: A is what makes up the first 80% of the sold value, B the next 15%, C the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbcClass {
    A,
    B,
    C,
}

impl AbcClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
        }
    }
}

/// One row of a sales report: what a customer, product or seller sold for, net of returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesReportRow {
    /// The customer, product or seller; none for the sales that name no seller.
    pub key: Option<Uuid>,
    /// How many completed sales it is in.
    pub sales: usize,
    pub net: Decimal,
    /// Its part of the whole, in percent, to two decimals.
    pub share: Decimal,
    /// The share of this row and of those before it.
    pub cumulative: Decimal,
    pub class: AbcClass,
}

/// The value of each line of a sale net of what came back of it, in the order of the lines: the line's total
/// less the cumulative refund of what was returned, rounded as the refund itself is (ADR 0039). The one
/// calculation that the commissions and the sales reports share, so the base of a commission and the value
/// in a report can never disagree.
pub fn net_of_lines(sale: &Sale, returns: &[SaleReturn]) -> Vec<Decimal> {
    sale.lines()
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let number = u16::try_from(index + 1).unwrap_or(u16::MAX);
            let returned: Decimal = returns
                .iter()
                .flat_map(|made| made.items())
                .filter(|(returned_line, _)| *returned_line == number)
                .map(|(_, quantity)| quantity.as_decimal())
                .sum();
            let refunded = round_to_cents(line.total() * returned / line.quantity().as_decimal());
            line.total() - refunded
        })
        .collect()
}

/// What the completed sales add up to by customer, product or seller, biggest first (the sales with no seller
/// last), each row with its share of the whole and its ABC class. A class is given by the share **before** the
/// row, so the row that crosses 80% is still an A and the one that crosses 95% still a B. The shares are
/// percentages cut to the cent by rounding half away from zero; the classes are worked out from the exact
/// values, never from those. Sales that are not completed are left out; `returns` holds what came back of them.
pub fn sales_report(
    group: SalesGroup,
    sales: &[(Sale, Vec<SaleReturn>)],
) -> (Decimal, Vec<SalesReportRow>) {
    // BTreeMap on the id: the order of equal rows never depends on how the sales were read.
    let mut by_key: BTreeMap<Option<Uuid>, (Decimal, usize)> = BTreeMap::new();
    for (sale, returns) in sales {
        if sale.status() != SaleStatus::Completed {
            continue;
        }
        let net = net_of_lines(sale, returns);
        match group {
            SalesGroup::Customer | SalesGroup::Seller => {
                let key = if group == SalesGroup::Customer {
                    Some(sale.customer_id().as_uuid())
                } else {
                    sale.seller_id().map(|id| id.as_uuid())
                };
                let entry = by_key.entry(key).or_default();
                entry.0 += net.iter().copied().sum::<Decimal>();
                entry.1 += 1;
            }
            SalesGroup::Product => {
                let mut seen = Vec::new();
                for (line, value) in sale.lines().iter().zip(net) {
                    let product = line.product_id().as_uuid();
                    let entry = by_key.entry(Some(product)).or_default();
                    entry.0 += value;
                    if !seen.contains(&product) {
                        seen.push(product);
                        entry.1 += 1;
                    }
                }
            }
        }
    }
    let total: Decimal = by_key.values().map(|(net, _)| *net).sum();
    let mut ranked: Vec<(Option<Uuid>, Decimal, usize)> = by_key
        .into_iter()
        .map(|(key, (net, count))| (key, net, count))
        .collect();
    // Biggest first; what has no key (no seller) last; the id breaks a tie.
    ranked.sort_by(|a, b| {
        a.0.is_none()
            .cmp(&b.0.is_none())
            .then(b.1.cmp(&a.1))
            .then(a.0.cmp(&b.0))
    });
    let hundred = Decimal::ONE_HUNDRED;
    let mut before = Decimal::ZERO;
    let rows = ranked
        .into_iter()
        .map(|(key, net, sales)| {
            let class = if total <= Decimal::ZERO || before * hundred >= total * Decimal::from(95) {
                AbcClass::C
            } else if before * hundred >= total * Decimal::from(80) {
                AbcClass::B
            } else {
                AbcClass::A
            };
            before += net;
            let percent = |part: Decimal| {
                if total <= Decimal::ZERO {
                    Decimal::ZERO
                } else {
                    (part * hundred / total)
                        .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
                        .normalize()
                }
            };
            SalesReportRow {
                key,
                sales,
                net: net.normalize(),
                share: percent(net),
                cumulative: percent(before),
                class,
            }
        })
        .collect();
    (total.normalize(), rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CategoryId, CompanyId, LineDraft, PartyId, ProductId, ReturnItemDraft, SaleDraft, SaleId,
        SaleReturnDraft, SaleReturnId,
    };

    fn sale(
        customer: PartyId,
        seller: Option<PartyId>,
        lines: &[(ProductId, &str, &str)],
        completed: bool,
    ) -> Sale {
        let drafts = lines
            .iter()
            .map(|(product, quantity, price)| LineDraft {
                product_id: *product,
                description: "Item".into(),
                quantity: (*quantity).into(),
                unit_price: (*price).into(),
                discount: "0".into(),
            })
            .collect();
        let made = Sale::new(
            SaleId::generate(),
            CompanyId::generate(),
            SaleDraft {
                customer_id: customer,
                seller_id: seller,
                sold_on: "2026-11-10".into(),
                payment_term_id: None,
                category_id: CategoryId::generate(),
                cost_center_id: None,
                notes: None,
                lines: drafts,
            },
        )
        .unwrap();
        if completed {
            made.completed().unwrap()
        } else {
            made
        }
    }

    fn returned(sale: &Sale, line: u16, quantity: &str) -> SaleReturn {
        SaleReturn::new(
            SaleReturnId::generate(),
            sale,
            SaleReturnDraft {
                returned_on: "2026-11-12".into(),
                items: vec![ReturnItemDraft {
                    line,
                    quantity: quantity.into(),
                }],
                reason: None,
            },
            &[],
        )
        .unwrap()
    }

    #[test]
    fn a_return_comes_off_the_line_it_was_made_on() {
        let (customer, coffee) = (PartyId::generate(), ProductId::generate());
        let sold = sale(customer, None, &[(coffee, "3", "10.10")], true);
        let back = returned(&sold, 1, "1");
        assert_eq!(net_of_lines(&sold, &[]), [Decimal::new(3030, 2)]);
        assert_eq!(net_of_lines(&sold, &[back]), [Decimal::new(2020, 2)]);
    }

    #[test]
    fn groups_by_customer_product_and_seller_net_of_returns_and_skips_what_is_not_completed() {
        let (ana, bia) = (PartyId::generate(), PartyId::generate());
        let (vera, coffee, bread) = (
            PartyId::generate(),
            ProductId::generate(),
            ProductId::generate(),
        );
        let first = sale(
            ana,
            Some(vera),
            &[(coffee, "3", "10"), (bread, "1", "5")],
            true,
        );
        let second = sale(bia, None, &[(coffee, "1", "10")], true);
        let order = sale(bia, Some(vera), &[(bread, "100", "5")], false);
        let back = returned(&first, 1, "1");
        let sales = vec![(first, vec![back]), (second, vec![]), (order, vec![])];

        let (total, customers) = sales_report(SalesGroup::Customer, &sales);
        assert_eq!(total, Decimal::from(35));
        let by_customer: Vec<_> = customers
            .iter()
            .map(|row| (row.key, row.net, row.sales))
            .collect();
        assert_eq!(
            by_customer,
            [
                (Some(ana.as_uuid()), Decimal::from(25), 1),
                (Some(bia.as_uuid()), Decimal::from(10), 1)
            ]
        );

        let (_, products) = sales_report(SalesGroup::Product, &sales);
        let by_product: Vec<_> = products
            .iter()
            .map(|row| (row.key, row.net, row.sales))
            .collect();
        assert_eq!(
            by_product,
            [
                (Some(coffee.as_uuid()), Decimal::from(30), 2),
                (Some(bread.as_uuid()), Decimal::from(5), 1)
            ]
        );

        // The sale with no seller is last, whatever it is worth.
        let (_, sellers) = sales_report(SalesGroup::Seller, &sales);
        assert_eq!(sellers.len(), 2);
        assert_eq!(sellers[0].key, Some(vera.as_uuid()));
        assert_eq!((sellers[1].key, sellers[1].net), (None, Decimal::from(10)));
    }

    #[test]
    fn the_abc_curve_gives_the_row_that_crosses_a_line_to_the_class_it_started_in() {
        let customer = PartyId::generate();
        // 80, 15, 4 and 1 of 100: the first ends exactly on 80%, so the second still starts at it and is a B;
        // the second ends exactly on 95%, so the third starts at it and is a C.
        let products: Vec<_> = (0..4).map(|_| ProductId::generate()).collect();
        let prices = ["80", "15", "4", "1"];
        let sales: Vec<_> = products
            .iter()
            .zip(prices)
            .map(|(product, price)| {
                (
                    sale(customer, None, &[(*product, "1", price)], true),
                    vec![],
                )
            })
            .collect();
        let (total, rows) = sales_report(SalesGroup::Product, &sales);
        assert_eq!(total, Decimal::from(100));
        let classes: Vec<_> = rows.iter().map(|row| row.class).collect();
        assert_eq!(
            classes,
            [AbcClass::A, AbcClass::B, AbcClass::C, AbcClass::C]
        );
        let shares: Vec<_> = rows.iter().map(|row| (row.share, row.cumulative)).collect();
        assert_eq!(
            shares,
            [
                (Decimal::from(80), Decimal::from(80)),
                (Decimal::from(15), Decimal::from(95)),
                (Decimal::from(4), Decimal::from(99)),
                (Decimal::from(1), Decimal::from(100)),
            ]
        );

        // One row over the line is an A as well: 85, 10, 5.
        let over: Vec<_> = products
            .iter()
            .zip(["85", "10", "5"])
            .map(|(product, price)| {
                (
                    sale(customer, None, &[(*product, "1", price)], true),
                    vec![],
                )
            })
            .collect();
        let (_, rows) = sales_report(SalesGroup::Product, &over);
        let classes: Vec<_> = rows.iter().map(|row| row.class).collect();
        assert_eq!(classes, [AbcClass::A, AbcClass::B, AbcClass::C]);
    }

    #[test]
    fn nothing_sold_has_no_rows_and_a_total_of_zero_has_no_shares() {
        let (total, rows) = sales_report(SalesGroup::Customer, &[]);
        assert_eq!((total, rows.len()), (Decimal::ZERO, 0));
        let (customer, coffee) = (PartyId::generate(), ProductId::generate());
        let sold = sale(customer, None, &[(coffee, "1", "10")], true);
        let back = returned(&sold, 1, "1");
        let (total, rows) = sales_report(SalesGroup::Customer, &[(sold, vec![back])]);
        assert_eq!(total, Decimal::ZERO);
        assert_eq!((rows[0].share, rows[0].class), (Decimal::ZERO, AbcClass::C));
    }
}
