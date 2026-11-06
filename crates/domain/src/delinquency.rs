use std::collections::BTreeMap;

use crate::{CalendarDate, Decimal, PartyId};

/// How late an installment is, in the four ranges the report adds up by: 1 to 30 days, 31 to 60, 61 to 90 and
/// more than 90. The first day of each; the last range has no end.
pub const BUCKET_STARTS: [i64; 4] = [1, 31, 61, 91];

/// An installment of a receivable that is late on the day of the report, with what is still owed on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LateInstallment {
    pub party_id: Option<PartyId>,
    pub due_date: CalendarDate,
    pub owed: Decimal,
}

/// What one customer owes late.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelinquentCustomer {
    /// `None` for what has no customer.
    pub party_id: Option<PartyId>,
    pub owed: Decimal,
    pub installments: u64,
    pub oldest_due: CalendarDate,
    /// What is owed in each range of lateness.
    pub by_bucket: [Decimal; 4],
}

/// Who owes what late on a day, and how late ([ADR 0051]): a function of the day and the late installments,
/// with nothing stored. Exact decimal throughout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelinquencyReport {
    pub on: CalendarDate,
    pub owed: Decimal,
    pub installments: u64,
    pub by_bucket: [Decimal; 4],
    pub installments_by_bucket: [u64; 4],
    /// The biggest debt first, then by party (what has no customer after the ones that have).
    pub customers: Vec<DelinquentCustomer>,
}

/// Which range an installment that is `days_late` days late falls in; `None` for one that is not late.
fn bucket_of(days_late: i64) -> Option<usize> {
    BUCKET_STARTS.iter().rposition(|start| days_late >= *start)
}

impl DelinquencyReport {
    /// Adds up the installments that are late on `on` (due before it, with something owed); the others are left
    /// out, so the caller can hand over a wider list.
    pub fn build(on: CalendarDate, installments: &[LateInstallment]) -> Self {
        let mut report = Self {
            on,
            owed: Decimal::ZERO,
            installments: 0,
            by_bucket: [Decimal::ZERO; 4],
            installments_by_bucket: [0; 4],
            customers: Vec::new(),
        };
        let mut by_party: BTreeMap<Option<uuid::Uuid>, DelinquentCustomer> = BTreeMap::new();
        for item in installments {
            let Some(bucket) = bucket_of(on.to_unix_days() - item.due_date.to_unix_days()) else {
                continue;
            };
            if item.owed <= Decimal::ZERO {
                continue;
            }
            report.owed += item.owed;
            report.installments += 1;
            report.by_bucket[bucket] += item.owed;
            report.installments_by_bucket[bucket] += 1;
            let customer = by_party
                .entry(item.party_id.map(|id| id.as_uuid()))
                .or_insert_with(|| DelinquentCustomer {
                    party_id: item.party_id,
                    owed: Decimal::ZERO,
                    installments: 0,
                    oldest_due: item.due_date,
                    by_bucket: [Decimal::ZERO; 4],
                });
            customer.owed += item.owed;
            customer.installments += 1;
            customer.oldest_due = customer.oldest_due.min(item.due_date);
            customer.by_bucket[bucket] += item.owed;
        }
        report.owed = report.owed.normalize();
        report.by_bucket = report.by_bucket.map(|amount| amount.normalize());
        let mut customers: Vec<DelinquentCustomer> = by_party
            .into_values()
            .map(|mut customer| {
                customer.owed = customer.owed.normalize();
                customer.by_bucket = customer.by_bucket.map(|amount| amount.normalize());
                customer
            })
            .collect();
        customers.sort_by(|a, b| {
            b.owed.cmp(&a.owed).then_with(|| {
                // `None` sorts first in an `Option`; here it goes last.
                match (a.party_id, b.party_id) {
                    (Some(x), Some(y)) => x.as_uuid().cmp(&y.as_uuid()),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            })
        });
        report.customers = customers;
        report
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn day(text: &str) -> CalendarDate {
        CalendarDate::parse(text).unwrap()
    }

    fn late(party: Option<PartyId>, due: &str, owed: &str) -> LateInstallment {
        LateInstallment {
            party_id: party,
            due_date: day(due),
            owed: Decimal::from_str(owed).unwrap(),
        }
    }

    #[test]
    fn puts_each_installment_in_its_range_by_how_many_days_late() {
        for (days, bucket) in [
            (0, None),
            (1, Some(0)),
            (30, Some(0)),
            (31, Some(1)),
            (60, Some(1)),
            (61, Some(2)),
            (90, Some(2)),
            (91, Some(3)),
            (4000, Some(3)),
            (-5, None),
        ] {
            assert_eq!(bucket_of(days), bucket, "{days}");
        }
    }

    #[test]
    fn adds_up_by_range_and_by_customer_and_leaves_out_what_is_not_late() {
        let (ana, bia) = (PartyId::generate(), PartyId::generate());
        let report = DelinquencyReport::build(
            day("2027-01-31"),
            &[
                late(Some(ana), "2027-01-20", "100"),
                late(Some(ana), "2026-12-20", "30.50"),
                late(Some(ana), "2026-10-01", "10"),
                late(Some(bia), "2027-01-30", "5"),
                late(None, "2027-01-01", "7"),
                // Due on the day, or after it, or with nothing owed: not late.
                late(Some(bia), "2027-01-31", "99"),
                late(Some(bia), "2027-02-10", "99"),
                late(Some(bia), "2027-01-01", "0"),
            ],
        );
        assert_eq!(report.owed.to_string(), "152.5");
        assert_eq!(report.installments, 5);
        assert_eq!(
            report.by_bucket.map(|amount| amount.to_string()),
            ["112", "30.5", "0", "10"]
        );
        assert_eq!(report.installments_by_bucket, [3, 1, 0, 1]);
        // The biggest first; what has no customer is a row of its own.
        let owed: Vec<String> = report
            .customers
            .iter()
            .map(|c| c.owed.to_string())
            .collect();
        assert_eq!(owed, ["140.5", "7", "5"]);
        assert_eq!(report.customers[0].party_id, Some(ana));
        assert_eq!(report.customers[0].installments, 3);
        assert_eq!(report.customers[0].oldest_due, day("2026-10-01"));
        assert_eq!(
            report.customers[0]
                .by_bucket
                .map(|amount| amount.to_string()),
            ["100", "30.5", "0", "10"]
        );
        assert_eq!(report.customers[1].party_id, None);
        assert_eq!(report.customers[2].party_id, Some(bia));
    }

    #[test]
    fn ties_go_by_party_and_the_one_with_no_customer_last() {
        let (a, b) = (PartyId::generate(), PartyId::generate());
        let report = DelinquencyReport::build(
            day("2027-01-31"),
            &[
                late(None, "2027-01-01", "10"),
                late(Some(a), "2027-01-01", "10"),
                late(Some(b), "2027-01-01", "10"),
            ],
        );
        let ids: Vec<Option<PartyId>> = report.customers.iter().map(|c| c.party_id).collect();
        let (low, high) = if a.as_uuid() < b.as_uuid() {
            (a, b)
        } else {
            (b, a)
        };
        assert_eq!(ids, [Some(low), Some(high), None]);
    }

    #[test]
    fn nothing_late_is_an_empty_report() {
        let report = DelinquencyReport::build(day("2027-01-31"), &[]);
        assert_eq!(report.owed, Decimal::ZERO);
        assert!(report.customers.is_empty());
    }
}
