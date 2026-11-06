use crate::{CompanyId, DomainError};

/// How many reminder rules a company has at most.
pub const MAX_DUNNING_RULES: usize = 20;
/// How many days before the due date a reminder can be set to go out (the rule's `days` is negative).
pub const MAX_DAYS_BEFORE: i16 = 60;
/// How many days after the due date.
pub const MAX_DAYS_AFTER: i16 = 180;

/// Where a reminder is sent ([ADR 0048]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DunningChannel {
    Email,
    Whatsapp,
}

impl DunningChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Whatsapp => "whatsapp",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "email" => Some(Self::Email),
            "whatsapp" => Some(Self::Whatsapp),
            _ => None,
        }
    }
}

/// One reminder: on the due date moved by `days` (negative before it, 0 on it, positive after it), by one
/// channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DunningRule {
    pub days: i16,
    pub channel: DunningChannel,
}

/// A company's cadence of reminders for the installments of its receivables ([ADR 0048]): a switch and a
/// list of rules, replaced as a whole. The rules are kept in the order they fall (earliest first), each
/// `(days, channel)` once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DunningPlan {
    company_id: CompanyId,
    enabled: bool,
    rules: Vec<DunningRule>,
}

impl DunningPlan {
    pub fn new(
        company_id: CompanyId,
        enabled: bool,
        mut rules: Vec<DunningRule>,
    ) -> Result<Self, DomainError> {
        if rules.len() > MAX_DUNNING_RULES {
            return Err(DomainError::TooManyDunningRules);
        }
        if rules
            .iter()
            .any(|rule| rule.days < -MAX_DAYS_BEFORE || rule.days > MAX_DAYS_AFTER)
        {
            return Err(DomainError::InvalidDunningDays);
        }
        rules.sort();
        if rules.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(DomainError::DuplicateDunningRule);
        }
        Ok(Self {
            company_id,
            enabled,
            rules,
        })
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn rules(&self) -> &[DunningRule] {
        &self.rules
    }

    /// The rules as the audit trail writes them, one per line.
    pub fn rules_text(&self) -> String {
        self.rules
            .iter()
            .map(|rule| format!("{} {}", rule.days, rule.channel.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(days: i16, channel: DunningChannel) -> DunningRule {
        DunningRule { days, channel }
    }

    #[test]
    fn the_rules_are_kept_in_the_order_they_fall() {
        let plan = DunningPlan::new(
            CompanyId::generate(),
            true,
            vec![
                rule(3, DunningChannel::Email),
                rule(-3, DunningChannel::Whatsapp),
                rule(-3, DunningChannel::Email),
                rule(0, DunningChannel::Email),
            ],
        )
        .unwrap();
        assert_eq!(plan.rules_text(), "-3 email\n-3 whatsapp\n0 email\n3 email");
    }

    #[test]
    fn refuses_a_rule_twice_one_too_far_and_too_many() {
        let company = CompanyId::generate();
        let twice = vec![
            rule(1, DunningChannel::Email),
            rule(1, DunningChannel::Email),
        ];
        assert_eq!(
            DunningPlan::new(company, true, twice),
            Err(DomainError::DuplicateDunningRule)
        );
        for days in [-MAX_DAYS_BEFORE - 1, MAX_DAYS_AFTER + 1] {
            assert_eq!(
                DunningPlan::new(company, true, vec![rule(days, DunningChannel::Email)]),
                Err(DomainError::InvalidDunningDays)
            );
        }
        assert!(
            DunningPlan::new(
                company,
                true,
                vec![rule(-MAX_DAYS_BEFORE, DunningChannel::Email)]
            )
            .is_ok()
        );
        let many: Vec<DunningRule> = (0..=MAX_DUNNING_RULES as i16)
            .map(|days| rule(days, DunningChannel::Email))
            .collect();
        assert_eq!(
            DunningPlan::new(company, true, many),
            Err(DomainError::TooManyDunningRules)
        );
    }

    #[test]
    fn a_plan_may_be_off_or_empty() {
        let plan = DunningPlan::new(CompanyId::generate(), false, vec![]).unwrap();
        assert!(!plan.is_enabled());
        assert!(plan.rules().is_empty());
    }
}
