use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, BankAccountId, CalendarDate, CategoryId, CompanyId, CostCenterId,
    Decimal, EntryId, EntryKind, EntryOrigin, FinancialEntry, PartyId, PaymentMethodId, Settlement,
    SettlementId,
};
use uuid::Uuid;

use crate::list_query::clean_search;
use crate::{AccountMovement, ApplicationError, DayMovement, RepositoryError};

/// Position of the last row of a list ordered by a day, then by id (both reversed for a descending
/// sort): the day and id of the last row are enough to resume right after it in either direction.
/// Entries are listed by competence date and transfers by the day they occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateCursor {
    pub date: CalendarDate,
    pub id: Uuid,
}

/// The name an entry's list uses for it.
pub type EntryCursor = DateCursor;

impl From<&FinancialEntry> for DateCursor {
    fn from(entry: &FinancialEntry) -> Self {
        Self {
            date: entry.competence_date(),
            id: entry.id().as_uuid(),
        }
    }
}

/// What is still open on installments that fall due on one day: 0 or more, exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayOpen {
    pub date: CalendarDate,
    pub receivable: Decimal,
    pub payable: Decimal,
}

/// What the entries are grouped by in a report ([ADR 0035]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportGroup {
    Category,
    CostCenter,
    Party,
}

/// Which day puts an entry in the span of a report, and what is added up: the entries whose competence
/// date is in it, by their total, or the installments whose due date is in it, by their amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportBasis {
    #[default]
    Competence,
    Due,
}

/// One group of a report: what was entered as receivable and as payable, and in how many entries.
/// `id` is `None` for what has no cost center or no party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportRow {
    pub id: Option<Uuid>,
    pub receivable: Decimal,
    pub payable: Decimal,
    pub entries: u64,
}

/// What the entries whose competence date falls in a calendar month added up to for one category and cost center,
/// by their total ([ADR 0070]): the rows the income statement is made of. `cost_center_id` is `None` for entries that
/// have none; `month` is the month's first day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompetenceTotal {
    pub month: CalendarDate,
    pub category_id: CategoryId,
    pub cost_center_id: Option<CostCenterId>,
    pub receivable: Decimal,
    pub payable: Decimal,
}

/// An installment that fell due and still has something left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverdueItem {
    pub entry_id: EntryId,
    pub kind: EntryKind,
    pub description: String,
    pub party_id: Option<PartyId>,
    pub category_id: CategoryId,
    pub installment: u16,
    pub due_date: CalendarDate,
    pub amount: Decimal,
    /// What the settlements that are not reversed settled; what is open is `amount - settled`.
    pub settled: Decimal,
}

/// The overdue installments, oldest due day first and cut at a limit, with the totals of **all** of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverdueReport {
    pub items: Vec<OverdueItem>,
    /// What is open on all the overdue installments of receivables, and of payables.
    pub receivable: Decimal,
    pub payable: Decimal,
    /// How many overdue installments there are, whatever the limit.
    pub count: u64,
}

/// The order of the list of entries. Newest competence first is what a person looks at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EntrySort {
    #[default]
    CompetenceDesc,
    CompetenceAsc,
}

/// How a list of entries is narrowed and ordered, following ADR 0018.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FinancialEntryQuery {
    search: Option<String>,
    kind: Option<EntryKind>,
    category: Option<CategoryId>,
    voided: Option<bool>,
    competence_from: Option<CalendarDate>,
    competence_until: Option<CalendarDate>,
    sort: EntrySort,
}

impl FinancialEntryQuery {
    /// Only the entries whose competence date is from `from` through `until`, both included ([ADR 0071]).
    pub fn competing_between(mut self, from: CalendarDate, until: CalendarDate) -> Self {
        self.competence_from = Some(from);
        self.competence_until = Some(until);
        self
    }

    pub fn competence_span(&self) -> Option<(CalendarDate, CalendarDate)> {
        self.competence_from.zip(self.competence_until)
    }

    /// Matches the description anywhere in it, ignoring case. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn with_kind(mut self, kind: EntryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_category(mut self, category: CategoryId) -> Self {
        self.category = Some(category);
        self
    }

    /// Only the cancelled entries (`true`) or only the others (`false`); both are listed when not said.
    pub fn with_voided(mut self, voided: bool) -> Self {
        self.voided = Some(voided);
        self
    }

    pub fn sorted(mut self, sort: EntrySort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn kind(&self) -> Option<EntryKind> {
        self.kind
    }

    pub fn category(&self) -> Option<CategoryId> {
        self.category
    }

    pub fn voided(&self) -> Option<bool> {
        self.voided
    }

    pub fn sort(&self) -> EntrySort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, entry: &FinancialEntry) -> bool {
        self.kind.is_none_or(|kind| entry.kind() == kind)
            && self
                .category
                .is_none_or(|category| entry.category_id() == category)
            && self.voided.is_none_or(|voided| entry.is_voided() == voided)
            && self
                .competence_span()
                .is_none_or(|(from, until)| (from..=until).contains(&entry.competence_date()))
            && self.search().is_none_or(|text| {
                entry
                    .description()
                    .to_lowercase()
                    .contains(&text.to_lowercase())
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write takes the audit entry that describes it and stores both **atomically** ([ADR 0008]):
/// if the write fails, nothing is recorded, and an entry is never stored without its write. The
/// installments are stored with the entry and replaced with it, in the same transaction.
pub trait FinancialEntryRepository: Send + Sync {
    fn save(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The entry, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> impl Future<Output = Result<Option<FinancialEntry>, RepositoryError>> + Send;

    /// Replaces the editable fields and the installments of a stored entry. `NotFound` when the
    /// company has no such one, and `EntrySettled` when it has settlements that are not reversed
    /// and the installments or the kind would change: nothing is written then. The rule is the
    /// adapter's, inside the write, so a settlement that races the edit cannot slip past it.
    fn update(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Stores a settlement of one installment with its audit entry, atomically, after checking that
    /// the entry has the installment and what is left of it: `NotFound` for an entry the company
    /// does not have or an installment it does not have, and `Overpaid` when what is applied, with
    /// what its non-reversed settlements already settled, is more than the installment is worth.
    /// Two settlements of the same installment at the same moment cannot both pass.
    fn settle(
        &self,
        settlement: &Settlement,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Marks a stored settlement reversed, with its audit entry, atomically. `NotFound` when the
    /// company's entry has no such settlement or it is reversed already.
    fn reverse(
        &self,
        reversed: &Settlement,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The entries of the company with these ids, installments included, by competence date and then id; an id the
    /// company does not have is not in the answer ([ADR 0071]).
    fn find_many(
        &self,
        company_id: CompanyId,
        ids: &[EntryId],
    ) -> impl Future<Output = Result<Vec<FinancialEntry>, RepositoryError>> + Send;

    /// The settlements that are not reversed, settled from `since` through `until`, by day settled and then id, at
    /// most `limit` ([ADR 0071]). An entry with such a settlement is never cancelled, so none is asked.
    fn settlements_between(
        &self,
        company_id: CompanyId,
        since: CalendarDate,
        until: CalendarDate,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Settlement>, RepositoryError>> + Send;

    /// The settlement, reversed or not, only if it belongs to the company's entry.
    fn settlement(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: SettlementId,
    ) -> impl Future<Output = Result<Option<Settlement>, RepositoryError>> + Send;

    /// Every settlement of these entries of the company, reversed ones included, by day settled
    /// and then by id.
    fn settlements_of(
        &self,
        company_id: CompanyId,
        entry_ids: &[EntryId],
    ) -> impl Future<Output = Result<Vec<Settlement>, RepositoryError>> + Send;

    /// What the settlements that are not reversed moved in the account, on the days from `since`
    /// (every day when `None`) up to and including `until`: `received` by receivables, `paid` by
    /// payables, each the **cash** of the settlement ([ADR 0030]).
    fn cash_of_account(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: Option<CalendarDate>,
        until: CalendarDate,
    ) -> impl Future<Output = Result<AccountMovement, RepositoryError>> + Send;

    /// The settlements of the account that are not reversed, of entries of this `kind`, whose **cash**
    /// is exactly `cash`, settled from `since` through `until`: what a bank statement line of that
    /// amount may be ([ADR 0034]). Oldest day first, then by id.
    fn settlement_candidates(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        kind: EntryKind,
        cash: Decimal,
        since: CalendarDate,
        until: CalendarDate,
    ) -> impl Future<Output = Result<Vec<Settlement>, RepositoryError>> + Send;

    /// The same, on each day from `since` through `until` that had any, oldest day first.
    fn cash_by_day(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> impl Future<Output = Result<Vec<DayMovement>, RepositoryError>> + Send;

    /// What is left of the installments **of the entries that are not cancelled**, their amount less what the
    /// settlements that are not reversed settled, summed by due day from `since` through `until`, oldest day first, only for
    /// the days that have something left: `receivable` of receivables and `payable` of payables.
    fn open_by_due_day(
        &self,
        company_id: CompanyId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> impl Future<Output = Result<Vec<DayOpen>, RepositoryError>> + Send;

    /// The entries added up by `group` over the days from `since` through `until`, on `basis`: one row
    /// per group that has any, the biggest of its two totals first, then by id with the rows that have
    /// none last ([ADR 0035]).
    fn report_entries(
        &self,
        company_id: CompanyId,
        group: ReportGroup,
        basis: ReportBasis,
        since: CalendarDate,
        until: CalendarDate,
    ) -> impl Future<Output = Result<Vec<ReportRow>, RepositoryError>> + Send;

    /// The entries that are not cancelled with a competence date from `since` through `until`, by their total,
    /// added up by month, category and cost center: one row per combination that has any, by month, then category,
    /// then cost center with the entries that have none first ([ADR 0070]).
    fn competence_totals(
        &self,
        company_id: CompanyId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> impl Future<Output = Result<Vec<CompetenceTotal>, RepositoryError>> + Send;

    /// The installments due **before** `before` that still have something left, of entries of `kind` when
    /// given, oldest due day first (then entry id, then number), at most `limit`, and the totals of all.
    fn overdue_installments(
        &self,
        company_id: CompanyId,
        before: CalendarDate,
        kind: Option<EntryKind>,
        limit: usize,
    ) -> impl Future<Output = Result<OverdueReport, RepositoryError>> + Send;

    /// The installments of **receivables that are not cancelled** that fall due from `since` through `until`
    /// and still have something left, oldest due day first (then entry id, then number): who a reminder is
    /// for ([ADR 0048]).
    fn receivable_installments_due(
        &self,
        company_id: CompanyId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> impl Future<Output = Result<Vec<OverdueItem>, RepositoryError>> + Send;

    /// Cancels a stored entry, with its audit entry, atomically ([ADR 0037]). `NotFound` when the company has
    /// no such entry or it is cancelled already, and `EntrySettled` when it has settlements that are not
    /// reversed: the rule is the adapter's, inside the write, so a settlement that races it cannot slip past.
    fn void(
        &self,
        voided: &FinancialEntry,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The entry made by this origin (a sale, a return), if one was.
    fn find_by_origin(
        &self,
        company_id: CompanyId,
        origin: EntryOrigin,
    ) -> impl Future<Output = Result<Option<FinancialEntry>, RepositoryError>> + Send;

    /// What was recorded about the entry, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` entries of the company that match `query`, in its order, starting right
    /// after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &FinancialEntryQuery,
        after: Option<&EntryCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<FinancialEntry>, RepositoryError>> + Send;
}

/// Whether what an entry points at can be used, asked before it is written ([ADR 0029]). A
/// reference to another company's record answers like one that does not exist.
pub trait EntryReferences: Send + Sync {
    /// The category exists in the company, is active and is not the one for transfers.
    fn category_is_usable(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The cost center exists in the company and is active.
    fn cost_center_is_usable(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The party exists in the company.
    fn party_exists(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The bank account exists in the company and is active.
    fn bank_account_is_usable(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The payment method exists in the company and is active.
    fn payment_method_is_usable(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> impl Future<Output = Result<bool, RepositoryError>> + Send;

    /// The last day that is closed, if the company closed a month ([ADR 0069]).
    fn locked_through(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<Option<CalendarDate>, RepositoryError>> + Send;

    /// Refuses, with `PeriodLocked`, a day that is closed: what every write of a dated record asks before it
    /// writes anything.
    fn ensure_open(
        &self,
        company_id: CompanyId,
        day: CalendarDate,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send {
        async move {
            crate::period_lock_service::ensure_open(self.locked_through(company_id).await?, day)
        }
    }
}
