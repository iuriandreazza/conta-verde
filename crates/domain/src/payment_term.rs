use rust_decimal::{Decimal, RoundingStrategy};

use crate::name::clean_name;
use crate::{Amount, CalendarDate, CompanyId, DomainError, EntryInstallmentDraft, PaymentTermId};

/// How many installments a term can have.
pub const MAX_INSTALLMENTS: usize = 24;
/// The longest wait for an installment: ten years, a bound and not a business rule.
pub const MAX_INSTALLMENT_DAYS: u32 = 3650;

/// A share of an amount: more than 0 and at most 100, with at most two decimals. A value with more
/// is refused, not rounded: splitting an amount into shares that do not come out even is the rule
/// of whoever splits it ("the remainder goes to the last installment", ADR 0003), and the term only
/// insists that its shares add up to the whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Percent(Decimal);

impl Percent {
    pub fn parse(raw: &str) -> Option<Self> {
        let value = Amount::parse(raw)?.as_decimal();
        // `Amount` keeps no trailing zeros, so the scale is the number of decimals that matter.
        (value > Decimal::ZERO && value <= Decimal::ONE_HUNDRED && value.scale() <= 2)
            .then_some(Self(value))
    }

    pub fn as_decimal(&self) -> Decimal {
        self.0
    }
}

impl std::fmt::Display for Percent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// One installment as it is asked for: unvalidated, the percentage as decimal text.
#[derive(Debug, Clone)]
pub struct InstallmentDraft {
    pub days: u32,
    pub percent: String,
}

/// One installment of a term: due `days` after the date the term is applied to, for `percent` of
/// the amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Installment {
    days: u32,
    percent: Percent,
}

impl Installment {
    pub fn days(&self) -> u32 {
        self.days
    }

    pub fn percent(&self) -> Percent {
        self.percent
    }
}

/// Unvalidated input for registering or changing a payment term.
#[derive(Debug, Clone)]
pub struct PaymentTermDraft {
    pub name: String,
    pub installments: Vec<InstallmentDraft>,
    pub active: bool,
}

/// A way of splitting a payment over time, such as "30/60/90" or "50% now, 50% in 30 days": a list of
/// installments, each due some days after the date the term is applied to, for a share of the
/// amount. Applying a term to an amount and a date is the entries' job (Phase 2); this is the
/// register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentTerm {
    id: PaymentTermId,
    company_id: CompanyId,
    name: String,
    installments: Vec<Installment>,
    active: bool,
}

impl PaymentTerm {
    pub fn new(
        id: PaymentTermId,
        company_id: CompanyId,
        draft: PaymentTermDraft,
    ) -> Result<Self, DomainError> {
        let name = clean_name(&draft.name)?;
        let installments = validated_installments(&draft.installments)?;
        Ok(Self {
            id,
            company_id,
            name,
            installments,
            active: draft.active,
        })
    }

    /// The same payment term with every editable field replaced by the draft's.
    pub fn replaced_with(&self, draft: PaymentTermDraft) -> Result<Self, DomainError> {
        Self::new(self.id, self.company_id, draft)
    }

    pub fn id(&self) -> PaymentTermId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn installments(&self) -> &[Installment] {
        &self.installments
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The installments of an entry for `total` (in cents, more than 0) counted from the day `from`: each is
    /// the total times its percentage **cut to the cent** and the last takes what is left, so they add up to
    /// the total exactly ([ADR 0037]; 33.33, 33.33 and 33.34 of 100). Nothing is rounded up: an installment of
    /// less than a cent is refused.
    pub fn split(
        &self,
        total: Decimal,
        from: CalendarDate,
    ) -> Result<Vec<EntryInstallmentDraft>, DomainError> {
        let mut left = total;
        let mut parts = Vec::with_capacity(self.installments.len());
        for (index, installment) in self.installments.iter().enumerate() {
            let last = index + 1 == self.installments.len();
            let share = if last {
                left
            } else {
                (total * installment.percent.as_decimal() / Decimal::ONE_HUNDRED)
                    .round_dp_with_strategy(2, RoundingStrategy::ToZero)
            };
            if share <= Decimal::ZERO {
                return Err(DomainError::InstallmentTooSmall);
            }
            left -= share;
            let due = from
                .plus_days(installment.days)
                .ok_or(DomainError::InvalidDate("due_date"))?;
            parts.push(EntryInstallmentDraft {
                due_date: due.to_string(),
                amount: share.normalize().to_string(),
            });
        }
        Ok(parts)
    }

    /// The installments in one canonical text, `days:percent` separated by commas (`0:50,30:50`),
    /// which is what the audit trail records: the same installments always read the same, however
    /// they were written, so an edit that changes nothing compares equal.
    pub fn installments_text(&self) -> String {
        self.installments
            .iter()
            .map(|installment| format!("{}:{}", installment.days, installment.percent))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// The rules that span the rows of a term, which no single-row constraint can state: there is at
/// least one installment and at most [`MAX_INSTALLMENTS`], the days increase strictly, and the
/// percentages add up to exactly 100.
fn validated_installments(drafts: &[InstallmentDraft]) -> Result<Vec<Installment>, DomainError> {
    if drafts.is_empty() {
        return Err(DomainError::NoInstallments);
    }
    if drafts.len() > MAX_INSTALLMENTS {
        return Err(DomainError::TooManyInstallments);
    }
    let mut installments = Vec::with_capacity(drafts.len());
    for draft in drafts {
        if draft.days > MAX_INSTALLMENT_DAYS {
            return Err(DomainError::InvalidInstallmentDays);
        }
        let percent = Percent::parse(draft.percent.trim()).ok_or(DomainError::InvalidPercent)?;
        installments.push(Installment {
            days: draft.days,
            percent,
        });
    }
    if installments
        .windows(2)
        .any(|pair| pair[0].days >= pair[1].days)
    {
        return Err(DomainError::InstallmentDaysNotIncreasing);
    }
    let total: Decimal = installments
        .iter()
        .map(|item| item.percent.as_decimal())
        .sum();
    if total != Decimal::ONE_HUNDRED {
        return Err(DomainError::PercentsDoNotSumTo100);
    }
    Ok(installments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(parts: &[(u32, &str)]) -> PaymentTermDraft {
        PaymentTermDraft {
            name: "  30/60/90 ".into(),
            installments: parts
                .iter()
                .map(|(days, percent)| InstallmentDraft {
                    days: *days,
                    percent: (*percent).into(),
                })
                .collect(),
            active: true,
        }
    }

    fn build(parts: &[(u32, &str)]) -> Result<PaymentTerm, DomainError> {
        PaymentTerm::new(
            PaymentTermId::generate(),
            CompanyId::generate(),
            draft(parts),
        )
    }

    #[test]
    fn keeps_the_installments_in_one_canonical_text() {
        let term = build(&[(0, "50.00"), (30, " 50 ")]).unwrap();
        assert_eq!(term.name(), "30/60/90");
        assert_eq!(term.installments_text(), "0:50,30:50");
        assert_eq!(term.installments().len(), 2);
        assert_eq!(term.installments()[1].days(), 30);
        let thirds = build(&[(30, "33.33"), (60, "33.33"), (90, "33.34")]).unwrap();
        assert_eq!(thirds.installments_text(), "30:33.33,60:33.33,90:33.34");
        // However they are written, the same installments read the same.
        let written = |percent: &str| build(&[(10, percent)]).unwrap().installments_text();
        assert_eq!(written("100.00"), written("100"));
        assert_eq!(written("100.0"), "10:100");
    }

    #[test]
    fn the_percentages_add_up_to_exactly_100() {
        assert!(build(&[(0, "100")]).is_ok());
        assert!(build(&[(0, "0.01"), (30, "99.99")]).is_ok());
        for parts in [
            vec![(0, "50"), (30, "49.99")],
            vec![(0, "50"), (30, "50.01")],
            vec![(30, "33.33"), (60, "33.33"), (90, "33.33")],
        ] {
            assert_eq!(
                build(&parts).unwrap_err(),
                DomainError::PercentsDoNotSumTo100,
                "{parts:?}"
            );
        }
    }

    #[test]
    fn a_percentage_is_a_plain_decimal_between_zero_and_a_hundred() {
        for bad in [
            "", "0", "0.00", "-10", "+10", "100.01", "101", "33.333", "1e2", "50,5", " ", "abc",
            "50 %",
        ] {
            assert_eq!(
                build(&[(0, bad), (30, "50")]).unwrap_err(),
                DomainError::InvalidPercent,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_days_start_anywhere_from_zero_and_strictly_increase() {
        assert!(build(&[(0, "100")]).is_ok());
        assert!(build(&[(3650, "100")]).is_ok());
        assert_eq!(
            build(&[(3651, "100")]).unwrap_err(),
            DomainError::InvalidInstallmentDays
        );
        assert_eq!(
            build(&[(30, "50"), (30, "50")]).unwrap_err(),
            DomainError::InstallmentDaysNotIncreasing
        );
        assert_eq!(
            build(&[(60, "50"), (30, "50")]).unwrap_err(),
            DomainError::InstallmentDaysNotIncreasing
        );
    }

    #[test]
    fn has_at_least_one_installment_and_at_most_twenty_four() {
        assert_eq!(build(&[]).unwrap_err(), DomainError::NoInstallments);
        // Twenty-five installments are one too many, whatever they add up to.
        let parts: Vec<(u32, &str)> = (1..=25).map(|day| (day, "4")).collect();
        assert_eq!(build(&parts).unwrap_err(), DomainError::TooManyInstallments);
        let mut even: Vec<(u32, &str)> = (1..=23).map(|day| (day, "4")).collect();
        even.push((24, "8"));
        assert!(build(&even).is_ok());
    }

    #[test]
    fn refuses_a_blank_or_unstorable_name() {
        let named = |name: &str| {
            PaymentTerm::new(
                PaymentTermId::generate(),
                CompanyId::generate(),
                PaymentTermDraft {
                    name: name.into(),
                    ..draft(&[(0, "100")])
                },
            )
        };
        assert_eq!(named("  ").unwrap_err(), DomainError::EmptyName);
        assert_eq!(named("À\0vista").unwrap_err(), DomainError::InvalidName);
    }

    #[test]
    fn replacing_keeps_the_identity() {
        let term = build(&[(0, "100")]).unwrap();
        let replaced = term
            .replaced_with(PaymentTermDraft {
                name: "Entrada e 30".into(),
                installments: vec![
                    InstallmentDraft {
                        days: 0,
                        percent: "40".into(),
                    },
                    InstallmentDraft {
                        days: 30,
                        percent: "60".into(),
                    },
                ],
                active: false,
            })
            .unwrap();
        assert_eq!(replaced.id(), term.id());
        assert_eq!(replaced.company_id(), term.company_id());
        assert_eq!(replaced.installments_text(), "0:40,30:60");
        assert!(!replaced.is_active());
    }

    #[test]
    fn splits_an_amount_with_the_remainder_on_the_last_installment() {
        let term = PaymentTerm::new(
            PaymentTermId::generate(),
            CompanyId::generate(),
            PaymentTermDraft {
                name: "30/60/90".into(),
                installments: vec![
                    InstallmentDraft {
                        days: 30,
                        percent: "33.33".into(),
                    },
                    InstallmentDraft {
                        days: 60,
                        percent: "33.33".into(),
                    },
                    InstallmentDraft {
                        days: 90,
                        percent: "33.34".into(),
                    },
                ],
                active: true,
            },
        )
        .unwrap();
        let from = CalendarDate::parse("2026-11-30").unwrap();
        let parts = term.split(Decimal::from(100), from).unwrap();
        let shown: Vec<(&str, &str)> = parts
            .iter()
            .map(|p| (p.due_date.as_str(), p.amount.as_str()))
            .collect();
        // 30 days after 30 November is 30 December; the shares are cut, the last takes the cent that is left.
        assert_eq!(
            shown,
            [
                ("2026-12-30", "33.33"),
                ("2027-01-29", "33.33"),
                ("2027-02-28", "33.34")
            ]
        );
        let odd = term.split("100.01".parse().unwrap(), from).unwrap();
        let sum: Decimal = odd
            .iter()
            .map(|p| p.amount.parse::<Decimal>().unwrap())
            .sum();
        assert_eq!(sum.to_string(), "100.01");
        assert_eq!(odd[2].amount, "33.35");
        // A cent split three ways leaves an installment of nothing.
        assert_eq!(
            term.split("0.01".parse().unwrap(), from).unwrap_err(),
            DomainError::InstallmentTooSmall
        );
    }
}
