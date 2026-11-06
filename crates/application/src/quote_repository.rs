use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CompanyId, PartyId, Quote, QuoteId, QuoteStatus,
};
use uuid::Uuid;

use crate::RepositoryError;
use crate::list_query::clean_search;

/// Position of the last row of a list of numbered documents: their number is unique in the company,
/// and the id ends the order like in every list ([ADR 0018]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberCursor {
    pub number: i64,
    pub id: Uuid,
}

impl From<&Quote> for NumberCursor {
    fn from(quote: &Quote) -> Self {
        Self {
            number: quote.number(),
            id: quote.id().as_uuid(),
        }
    }
}

/// The order of a list of numbered documents. Newest first is what a person looks at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NumberSort {
    #[default]
    NumberDesc,
    NumberAsc,
}

/// How a list of quotes is narrowed and ordered, following ADR 0018.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuoteQuery {
    search: Option<String>,
    status: Option<QuoteStatus>,
    customer: Option<PartyId>,
    sort: NumberSort,
}

impl QuoteQuery {
    /// Matches the number or the notes of the quote, or the description of one of its lines, ignoring
    /// case. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn with_status(mut self, status: QuoteStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_customer(mut self, customer: PartyId) -> Self {
        self.customer = Some(customer);
        self
    }

    pub fn sorted(mut self, sort: NumberSort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn status(&self) -> Option<QuoteStatus> {
        self.status
    }

    pub fn customer(&self) -> Option<PartyId> {
        self.customer
    }

    pub fn sort(&self) -> NumberSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, quote: &Quote) -> bool {
        self.status.is_none_or(|status| quote.status() == status)
            && self
                .customer
                .is_none_or(|customer| quote.customer_id() == customer)
            && self.search().is_none_or(|text| {
                let text = text.to_lowercase();
                quote.number().to_string() == text
                    || quote
                        .notes()
                        .is_some_and(|notes| notes.to_lowercase().contains(&text))
                    || quote
                        .lines()
                        .iter()
                        .any(|line| line.description().to_lowercase().contains(&text))
            })
    }
}

/// What a write does to the link that lets the customer answer a quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkChange {
    Keep,
    /// The link stops working: what it showed is not what the quote is any more.
    Revoke,
    /// A new link, of which only the digest of the token is kept.
    Set(String),
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait QuoteRepository: Send + Sync {
    /// Stores a new quote, giving it the next number of the company, which it returns.
    fn save(
        &self,
        quote: &Quote,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<i64, RepositoryError>> + Send;

    /// The quote, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: QuoteId,
    ) -> impl Future<Output = Result<Option<Quote>, RepositoryError>> + Send;

    /// The quote whose link has this digest.
    fn find_by_link(
        &self,
        digest: &str,
    ) -> impl Future<Output = Result<Option<Quote>, RepositoryError>> + Send;

    /// Replaces the content, the status and who sent it of a stored quote, and changes its link as
    /// asked, only if it still has the status `expected`: `NotFound` when the company has no such
    /// quote, `QuoteChanged` when it is in another status (the customer answered, or someone else
    /// changed it).
    fn update(
        &self,
        quote: &Quote,
        expected: QuoteStatus,
        link: LinkChange,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the quote, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: QuoteId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` quotes of the company that match `query`, in its order, starting right after
    /// `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &QuoteQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Quote>, RepositoryError>> + Send;
}
