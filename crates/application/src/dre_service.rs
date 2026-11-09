use std::collections::HashMap;

use conta_verde_domain::{
    CalendarDate, CategoryId, CompanyId, CostCenterId, Decimal, DreColumn, DreColumnKey, DreGroup,
    DreLines,
};

use crate::{ApplicationError, CategoryRepository, FinancialEntryRepository};

/// The most calendar months a statement covers.
pub const MAX_DRE_MONTHS: u32 = 24;

/// What the columns of a statement are ([ADR 0069]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreBy {
    Month,
    CostCenter,
}

/// Which entries a statement counts, by cost center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CostCenterFilter {
    #[default]
    All,
    /// Only entries that have no cost center.
    Without,
    Only(CostCenterId),
}

/// The managerial income statement of a span: a column per month or per cost center, and the total of all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dre {
    pub from: CalendarDate,
    pub to: CalendarDate,
    pub by: DreBy,
    pub columns: Vec<DreColumn>,
    pub total: DreLines,
}

/// The income statement ([ADR 0012], [ADR 0069]): derived on every read from the entries by **competence date**, each
/// counted by its total in the month of that date, cancelled ones left out, a receivable positive and a payable negative.
/// It composes the entries' totals with the categories (which say each one's DRE group), so both adapters give the same.
pub struct DreService<E, C> {
    entries: E,
    categories: C,
}

impl<E: FinancialEntryRepository, C: CategoryRepository> DreService<E, C> {
    pub fn new(entries: E, categories: C) -> Self {
        Self {
            entries,
            categories,
        }
    }

    pub async fn statement(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
        by: DreBy,
        cost_center: CostCenterFilter,
    ) -> Result<Dre, ApplicationError> {
        if from > to {
            return Err(ApplicationError::InvalidSpan("from must not be after to"));
        }
        let months = months_between(from, to);
        if months.len() > MAX_DRE_MONTHS as usize {
            return Err(ApplicationError::InvalidSpan(
                "a statement covers at most 24 months",
            ));
        }
        let groups: HashMap<CategoryId, DreGroup> = self
            .categories
            .list_by_company(company_id, None)
            .await?
            .iter()
            .map(|category| (category.id(), category.dre_group()))
            .collect();
        let totals = self.entries.competence_totals(company_id, from, to).await?;

        let mut columns: Vec<DreColumn> = match by {
            DreBy::Month => months
                .into_iter()
                .map(|month| DreColumn {
                    key: DreColumnKey::Month(month),
                    lines: DreLines::default(),
                })
                .collect(),
            DreBy::CostCenter => Vec::new(),
        };
        let mut total = DreLines::default();
        for row in totals {
            let wanted = match cost_center {
                CostCenterFilter::All => true,
                CostCenterFilter::Without => row.cost_center_id.is_none(),
                CostCenterFilter::Only(id) => row.cost_center_id == Some(id),
            };
            // A category the statement does not know is one that was removed from the books: it has no group.
            let Some(group) = groups.get(&row.category_id).copied().filter(|_| wanted) else {
                continue;
            };
            let amount: Decimal = row.receivable - row.payable;
            let key = match by {
                DreBy::Month => DreColumnKey::Month(row.month),
                DreBy::CostCenter => DreColumnKey::CostCenter(row.cost_center_id),
            };
            let column = match columns.iter_mut().position(|column| column.key == key) {
                Some(index) => &mut columns[index],
                None => {
                    columns.push(DreColumn {
                        key,
                        lines: DreLines::default(),
                    });
                    columns.last_mut().expect("a column was just added")
                }
            };
            column.lines.add(group, amount);
            total.add(group, amount);
        }
        if by == DreBy::CostCenter {
            // By cost center in a fixed order, the entries that have none last: the web app names and sorts them.
            columns.sort_by_key(|column| match column.key {
                DreColumnKey::CostCenter(Some(id)) => (false, Some(id.as_uuid())),
                _ => (true, None),
            });
        }
        Ok(Dre {
            from,
            to,
            by,
            columns,
            total,
        })
    }
}

/// The first day of every calendar month the span touches, in order.
fn months_between(from: CalendarDate, to: CalendarDate) -> Vec<CalendarDate> {
    let first = from.first_of_month();
    let mut months = Vec::new();
    let mut step = 0;
    while let Some(month) = first.plus_months(step) {
        if month > to {
            break;
        }
        months.push(month);
        step += 1;
        // Longer than any statement: the caller refuses it, so there is no need to count past it.
        if step > MAX_DRE_MONTHS {
            break;
        }
    }
    months
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(raw: &str) -> CalendarDate {
        CalendarDate::parse(raw).unwrap()
    }

    #[test]
    fn a_span_touches_every_month_it_covers() {
        let months = months_between(day("2026-10-15"), day("2027-01-02"));
        let shown: Vec<String> = months.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            ["2026-10-01", "2026-11-01", "2026-12-01", "2027-01-01"]
        );
        assert_eq!(
            months_between(day("2026-10-31"), day("2026-10-31")).len(),
            1
        );
    }
}
