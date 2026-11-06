use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CalendarDate, CompanyId, ContractCycle, ContractId, PartyId, QuoteId,
    Sale, SaleId, SaleReturn, SaleStatus,
};

use crate::list_query::clean_search;
use crate::{NumberCursor, NumberSort, RepositoryError};

impl From<&Sale> for NumberCursor {
    fn from(sale: &Sale) -> Self {
        Self {
            number: sale.number(),
            id: sale.id().as_uuid(),
        }
    }
}

/// How a list of sales is narrowed and ordered, following ADR 0018.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaleQuery {
    search: Option<String>,
    status: Option<SaleStatus>,
    customer: Option<PartyId>,
    seller: Option<PartyId>,
    contract: Option<ContractId>,
    sold_between: Option<(CalendarDate, CalendarDate)>,
    sort: NumberSort,
}

impl SaleQuery {
    /// Matches the number, the notes or the description of one of the lines, ignoring case.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn with_status(mut self, status: SaleStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_customer(mut self, customer: PartyId) -> Self {
        self.customer = Some(customer);
        self
    }

    pub fn with_seller(mut self, seller: PartyId) -> Self {
        self.seller = Some(seller);
        self
    }

    pub fn of_contract(mut self, contract: ContractId) -> Self {
        self.contract = Some(contract);
        self
    }

    /// Sold from a day through a day, both included.
    pub fn sold_between(mut self, from: CalendarDate, to: CalendarDate) -> Self {
        self.sold_between = Some((from, to));
        self
    }

    pub fn sorted(mut self, sort: NumberSort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn status(&self) -> Option<SaleStatus> {
        self.status
    }

    pub fn customer(&self) -> Option<PartyId> {
        self.customer
    }

    pub fn seller(&self) -> Option<PartyId> {
        self.seller
    }

    pub fn contract(&self) -> Option<ContractId> {
        self.contract
    }

    pub fn span(&self) -> Option<(CalendarDate, CalendarDate)> {
        self.sold_between
    }

    pub fn sort(&self) -> NumberSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, sale: &Sale) -> bool {
        self.status.is_none_or(|status| sale.status() == status)
            && self
                .customer
                .is_none_or(|customer| sale.customer_id() == customer)
            && self
                .seller
                .is_none_or(|seller| sale.seller_id() == Some(seller))
            && self
                .sold_between
                .is_none_or(|(from, to)| (from..=to).contains(&sale.sold_on()))
            && self.contract.is_none_or(|contract| {
                sale.cycle()
                    .is_some_and(|cycle| cycle.contract_id == contract)
            })
            && self.search().is_none_or(|text| {
                let text = text.to_lowercase();
                sale.number().to_string() == text
                    || sale
                        .notes()
                        .is_some_and(|notes| notes.to_lowercase().contains(&text))
                    || sale
                        .lines()
                        .iter()
                        .any(|line| line.description().to_lowercase().contains(&text))
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait SaleRepository: Send + Sync {
    /// Stores a new sale, giving it the next number of the company, which it returns. `DuplicateQuoteSale`
    /// when it says it came from a quote that already has a sale, and `DuplicateCycle` when it bills a cycle
    /// of a contract that already has one.
    fn save(
        &self,
        sale: &Sale,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<i64, RepositoryError>> + Send;

    /// The sale, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: SaleId,
    ) -> impl Future<Output = Result<Option<Sale>, RepositoryError>> + Send;

    /// The sale of this cycle of a contract, if one was made.
    fn find_by_cycle(
        &self,
        company_id: CompanyId,
        cycle: ContractCycle,
    ) -> impl Future<Output = Result<Option<Sale>, RepositoryError>> + Send;

    /// The sale made from this quote, if one was.
    fn find_by_quote(
        &self,
        company_id: CompanyId,
        quote_id: QuoteId,
    ) -> impl Future<Output = Result<Option<Sale>, RepositoryError>> + Send;

    /// Replaces the content and the status of a stored sale, only if it still has the status `expected`:
    /// `NotFound` when the company has no such sale, `SaleChanged` when it is in another status.
    fn update(
        &self,
        sale: &Sale,
        expected: SaleStatus,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Changes only the status of a stored sale (completing, cancelling), only if it still has the status
    /// `expected`: `NotFound` when the company has no such sale, `SaleChanged` when it is in another status.
    /// The content is not written, so an edit that landed meanwhile is never overwritten with an older copy.
    /// Cancelling is refused with `SaleHasReturns` if the sale has returns, checked with the sale locked.
    fn set_status(
        &self,
        sale: &Sale,
        expected: SaleStatus,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the sale, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: SaleId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` sales of the company that match `query`, in its order, starting right after `after`.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &SaleQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Sale>, RepositoryError>> + Send;

    /// Stores a return with its audit entry, atomically, after checking, with the sale locked, that the sale is
    /// still completed (`SaleChanged`), that it has the `known_returns` the use case worked its refund out
    /// from (`SaleChanged`: a return landed meanwhile, and the cumulative refund would be wrong), and that it
    /// takes back no more of a line than was sold and not returned yet (`ReturnTooMuch`).
    fn save_return(
        &self,
        sale_return: &SaleReturn,
        known_returns: usize,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The returns of these sales of the company, oldest first.
    fn returns_of_sales(
        &self,
        company_id: CompanyId,
        sale_ids: &[SaleId],
    ) -> impl Future<Output = Result<Vec<SaleReturn>, RepositoryError>> + Send;

    /// The returns of a sale of the company, oldest first.
    fn returns_of(
        &self,
        company_id: CompanyId,
        sale_id: SaleId,
    ) -> impl Future<Output = Result<Vec<SaleReturn>, RepositoryError>> + Send;
}
