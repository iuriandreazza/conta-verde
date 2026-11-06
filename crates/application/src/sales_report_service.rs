use conta_verde_domain::{
    CalendarDate, CompanyId, Decimal, SaleStatus, SalesGroup, SalesReportRow, sales_report,
};

use crate::sale_reading::read_sales;
use crate::{ApplicationError, SaleQuery, SaleRepository};

/// What the completed sales added up to over a span, by customer, product or seller ([ADR 0043]).
#[derive(Debug, Clone)]
pub struct SalesReport {
    pub from: CalendarDate,
    pub to: CalendarDate,
    pub group: SalesGroup,
    /// How many completed sales the span holds.
    pub sales: usize,
    pub total: Decimal,
    pub rows: Vec<SalesReportRow>,
}

/// The sales reports: sums made when they are read, from the completed sales of a span and what came back of
/// them, by the same calculation as the commissions (nothing is stored).
pub struct SalesReportService<S> {
    sales: S,
}

impl<S: SaleRepository> SalesReportService<S> {
    pub fn new(sales: S) -> Self {
        Self { sales }
    }

    /// The sales sold from `from` through `to`, grouped and ranked. More than 2000 sales are refused: narrow
    /// the span.
    pub async fn report(
        &self,
        company_id: CompanyId,
        group: SalesGroup,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<SalesReport, ApplicationError> {
        if from > to {
            return Err(ApplicationError::InvalidSpan("from must not be after to"));
        }
        let query = SaleQuery::default()
            .with_status(SaleStatus::Completed)
            .sold_between(from, to);
        let sales = read_sales(&self.sales, company_id, query).await?;
        let ids: Vec<_> = sales.iter().map(|sale| sale.id()).collect();
        let returns = self.sales.returns_of_sales(company_id, &ids).await?;
        let with_returns: Vec<_> = sales
            .into_iter()
            .map(|sale| {
                let made = returns
                    .iter()
                    .filter(|made| made.sale_id() == sale.id())
                    .cloned()
                    .collect();
                (sale, made)
            })
            .collect();
        let (total, rows) = sales_report(group, &with_returns);
        Ok(SalesReport {
            from,
            to,
            group,
            sales: with_returns.len(),
            total,
            rows,
        })
    }
}
