use crate::{CalendarDate, CostCenterId, Decimal, DreGroup};

/// What each group of the managerial income statement added up to ([ADR 0012], [ADR 0070]): the amount of a receivable
/// counts positive and of a payable negative, so a group's figure is what it contributed to the result (revenue positive,
/// costs negative). The subtotals are worked out from the groups here and never typed. `investments` and
/// `outside_result` are shown below the line: they move cash and are not part of the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DreLines {
    pub gross_revenue: Decimal,
    pub deductions: Decimal,
    pub costs: Decimal,
    pub operating_expenses: Decimal,
    pub financial_result: Decimal,
    pub investments: Decimal,
    pub outside_result: Decimal,
}

impl DreLines {
    /// Adds a signed amount to its group. Transfers are never a result and are left out.
    pub fn add(&mut self, group: DreGroup, amount: Decimal) {
        match group {
            DreGroup::GrossRevenue => self.gross_revenue += amount,
            DreGroup::Deductions => self.deductions += amount,
            DreGroup::Costs => self.costs += amount,
            DreGroup::OperatingExpenses => self.operating_expenses += amount,
            DreGroup::FinancialResult => self.financial_result += amount,
            DreGroup::Investments => self.investments += amount,
            DreGroup::OutsideResult => self.outside_result += amount,
            DreGroup::Transfers => {}
        }
    }

    /// Adds every group of `other` to this one.
    pub fn merge(&mut self, other: &Self) {
        self.gross_revenue += other.gross_revenue;
        self.deductions += other.deductions;
        self.costs += other.costs;
        self.operating_expenses += other.operating_expenses;
        self.financial_result += other.financial_result;
        self.investments += other.investments;
        self.outside_result += other.outside_result;
    }

    /// Gross revenue less deductions (which are already negative).
    pub fn net_revenue(&self) -> Decimal {
        self.gross_revenue + self.deductions
    }

    pub fn gross_profit(&self) -> Decimal {
        self.net_revenue() + self.costs
    }

    pub fn operating_result(&self) -> Decimal {
        self.gross_profit() + self.operating_expenses
    }

    pub fn net_result(&self) -> Decimal {
        self.operating_result() + self.financial_result
    }
}

/// What a column of the statement stands for: a calendar month (its first day) or a cost center, `None` for the entries
/// that have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreColumnKey {
    Month(CalendarDate),
    CostCenter(Option<CostCenterId>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DreColumn {
    pub key: DreColumnKey,
    pub lines: DreLines,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amount(raw: &str) -> Decimal {
        raw.parse().unwrap()
    }

    #[test]
    fn the_subtotals_follow_from_the_groups() {
        let mut lines = DreLines::default();
        lines.add(DreGroup::GrossRevenue, amount("1000"));
        lines.add(DreGroup::Deductions, amount("-100"));
        lines.add(DreGroup::Costs, amount("-300"));
        lines.add(DreGroup::OperatingExpenses, amount("-200"));
        lines.add(DreGroup::FinancialResult, amount("-20.5"));
        lines.add(DreGroup::Investments, amount("-400"));
        lines.add(DreGroup::OutsideResult, amount("5000"));
        assert_eq!(lines.net_revenue(), amount("900"));
        assert_eq!(lines.gross_profit(), amount("600"));
        assert_eq!(lines.operating_result(), amount("400"));
        // Investments and loans are below the line: they are not in the result.
        assert_eq!(lines.net_result(), amount("379.5"));
    }

    #[test]
    fn transfers_are_never_added() {
        let mut lines = DreLines::default();
        lines.add(DreGroup::Transfers, amount("999"));
        assert_eq!(lines, DreLines::default());
    }

    #[test]
    fn merging_adds_group_by_group() {
        let mut first = DreLines::default();
        first.add(DreGroup::GrossRevenue, amount("10"));
        let mut second = DreLines::default();
        second.add(DreGroup::GrossRevenue, amount("5"));
        second.add(DreGroup::Costs, amount("-2"));
        first.merge(&second);
        assert_eq!(first.gross_revenue, amount("15"));
        assert_eq!(first.net_result(), amount("13"));
    }
}
