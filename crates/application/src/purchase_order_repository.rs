use std::future::Future;

use conta_verde_domain::{
    AccessKey, AuditEntry, AuditRecord, CompanyId, PartyId, PurchaseOrder, PurchaseOrderId,
    PurchaseReceipt, PurchaseReceiptId, PurchaseStatus,
};

use crate::list_query::clean_search;
use crate::{NumberCursor, NumberSort, RepositoryError};

impl From<&PurchaseOrder> for NumberCursor {
    fn from(order: &PurchaseOrder) -> Self {
        Self {
            number: order.number(),
            id: order.id().as_uuid(),
        }
    }
}

/// How a list of purchase orders is narrowed and ordered, following ADR 0018.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PurchaseOrderQuery {
    search: Option<String>,
    status: Option<PurchaseStatus>,
    supplier: Option<PartyId>,
    sort: NumberSort,
}

impl PurchaseOrderQuery {
    /// Matches the number, the notes or the description of one of the lines, ignoring case.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn with_status(mut self, status: PurchaseStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_supplier(mut self, supplier: PartyId) -> Self {
        self.supplier = Some(supplier);
        self
    }

    pub fn sorted(mut self, sort: NumberSort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn status(&self) -> Option<PurchaseStatus> {
        self.status
    }

    pub fn supplier(&self) -> Option<PartyId> {
        self.supplier
    }

    pub fn sort(&self) -> NumberSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, order: &PurchaseOrder) -> bool {
        self.status.is_none_or(|status| order.status() == status)
            && self
                .supplier
                .is_none_or(|supplier| order.supplier_id() == supplier)
            && self.search().is_none_or(|text| {
                let text = text.to_lowercase();
                order.number().to_string() == text
                    || order
                        .notes()
                        .is_some_and(|notes| notes.to_lowercase().contains(&text))
                    || order
                        .lines()
                        .iter()
                        .any(|line| line.description().to_lowercase().contains(&text))
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]).
pub trait PurchaseOrderRepository: Send + Sync {
    /// Stores a new order, giving it the next number of the company, which it returns. `DuplicateInvoice` when it
    /// says it was entered from an NF-e that already has an order.
    fn save(
        &self,
        order: &PurchaseOrder,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<i64, RepositoryError>> + Send;

    /// The order, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> impl Future<Output = Result<Option<PurchaseOrder>, RepositoryError>> + Send;

    /// The order made from this NF-e, if one was.
    fn find_by_invoice_key(
        &self,
        company_id: CompanyId,
        key: &AccessKey,
    ) -> impl Future<Output = Result<Option<PurchaseOrder>, RepositoryError>> + Send;

    /// Replaces the content of a stored order, only if it still has the status `expected`: `NotFound` when
    /// the company has no such order, `PurchaseChanged` when it is in another status.
    fn update(
        &self,
        order: &PurchaseOrder,
        expected: PurchaseStatus,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Changes only the status of a stored order (approving, cancelling, closing), only if it still has the
    /// status `expected`: `NotFound` when the company has no such order, `PurchaseChanged` when it is in another
    /// status. The content is not written, so an edit that landed meanwhile is never overwritten.
    fn set_status(
        &self,
        order: &PurchaseOrder,
        expected: PurchaseStatus,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the order, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` orders of the company that match `query`, in its order, starting right after `after`.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &PurchaseOrderQuery,
        after: Option<&NumberCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<PurchaseOrder>, RepositoryError>> + Send;

    /// Stores a receipt with its audit entry, atomically, and moves the order to the status `order_after` says
    /// (with `order_audit`, when the status changes), after checking, with the order locked, that it can still
    /// receive (`PurchaseChanged`), that it has the `known_receipts` the use case worked the values out from
    /// (`PurchaseChanged`: a receipt landed meanwhile and the cumulative values would be wrong) and that no line
    /// takes in more than was ordered (`ReceiptTooMuch`).
    fn save_receipt(
        &self,
        receipt: &PurchaseReceipt,
        order_after: &PurchaseOrder,
        known_receipts: usize,
        receipt_audit: &AuditEntry,
        order_audit: Option<&AuditEntry>,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The receipts of an order of the company, oldest first.
    fn receipts_of(
        &self,
        company_id: CompanyId,
        order_id: PurchaseOrderId,
    ) -> impl Future<Output = Result<Vec<PurchaseReceipt>, RepositoryError>> + Send;

    /// One receipt of an order of the company.
    fn find_receipt(
        &self,
        company_id: CompanyId,
        order_id: PurchaseOrderId,
        id: PurchaseReceiptId,
    ) -> impl Future<Output = Result<Option<PurchaseReceipt>, RepositoryError>> + Send;
}
