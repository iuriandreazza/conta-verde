use conta_verde_domain::{CompanyId, Sale};

use crate::{ApplicationError, NumberCursor, NumberSort, SaleQuery, SaleRepository};

/// The most sales one statement or report adds up: they are pages a person reads, and a longer span is cut.
pub(crate) const MAX_REPORT_SALES: usize = 2000;
const PAGE: usize = 200;

/// Every sale that matches `query`, oldest number first, read page by page. More than [`MAX_REPORT_SALES`] are
/// refused rather than cut, so a total is never quietly short.
pub(crate) async fn read_sales<S: SaleRepository>(
    sales: &S,
    company_id: CompanyId,
    query: SaleQuery,
) -> Result<Vec<Sale>, ApplicationError> {
    let query = query.sorted(NumberSort::NumberAsc);
    let mut found: Vec<Sale> = Vec::new();
    let mut after: Option<NumberCursor> = None;
    loop {
        let page = sales
            .list_by_company(company_id, &query, after.as_ref(), PAGE + 1)
            .await?;
        let has_more = page.len() > PAGE;
        let kept = page.into_iter().take(PAGE).collect::<Vec<_>>();
        after = kept.last().map(NumberCursor::from);
        found.extend(kept);
        if found.len() > MAX_REPORT_SALES {
            return Err(ApplicationError::InvalidSpan(
                "more than 2000 sales: narrow the period",
            ));
        }
        if !has_more {
            return Ok(found);
        }
    }
}
