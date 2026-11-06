use std::future::Future;

use conta_verde_domain::{
    AuditEntry, AuditRecord, CalendarDate, CompanyId, EntryKind, Recurrence, RecurrenceId,
};

use crate::list_query::clean_search;
use crate::{NameCursor, NameSort, RepositoryError};

impl From<&Recurrence> for NameCursor {
    fn from(recurrence: &Recurrence) -> Self {
        Self {
            name: recurrence.description().to_owned(),
            id: recurrence.id().as_uuid(),
        }
    }
}

/// How a list of recurrences is narrowed and ordered, following ADR 0018. Paused ones are listed
/// unless asked otherwise, so that they can be found and brought back. They are ordered by
/// description.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecurrenceQuery {
    search: Option<String>,
    kind: Option<EntryKind>,
    active: Option<bool>,
    sort: NameSort,
}

impl RecurrenceQuery {
    /// Matches the description anywhere in it, ignoring case. Blank text means no search.
    pub fn searching(mut self, text: &str) -> Self {
        self.search = clean_search(text);
        self
    }

    pub fn of_kind(mut self, kind: EntryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = Some(active);
        self
    }

    pub fn sorted(mut self, sort: NameSort) -> Self {
        self.sort = sort;
        self
    }

    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    pub fn kind(&self) -> Option<EntryKind> {
        self.kind
    }

    pub fn active(&self) -> Option<bool> {
        self.active
    }

    pub fn sort(&self) -> NameSort {
        self.sort
    }

    /// The same rules the SQL adapter applies in the database.
    pub fn matches(&self, recurrence: &Recurrence) -> bool {
        self.kind.is_none_or(|kind| recurrence.kind() == kind)
            && self
                .active
                .is_none_or(|active| recurrence.is_active() == active)
            && self.search().is_none_or(|text| {
                recurrence
                    .description()
                    .to_lowercase()
                    .contains(&text.to_lowercase())
            })
    }
}

/// Port implemented by storage adapters. Futures are `Send` so use cases can run on a multi-threaded runtime.
///
/// Every write of the register takes the audit entry that describes it and stores both
/// **atomically** ([ADR 0008]). The day a recurrence was generated through is bookkeeping, not part
/// of the register, and is written without one.
pub trait RecurrenceRepository: Send + Sync {
    fn save(
        &self,
        recurrence: &Recurrence,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The recurrence, only if it belongs to the company.
    fn find(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> impl Future<Output = Result<Option<Recurrence>, RepositoryError>> + Send;

    /// Replaces the editable fields of a stored recurrence, and the day it was generated through,
    /// which starts over when its schedule changed. `NotFound` when the company has no such one.
    fn update(
        &self,
        recurrence: &Recurrence,
        audit: &AuditEntry,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Records the day the recurrence was generated through. Written only forward: an older day
    /// than the stored one changes nothing.
    fn mark_generated(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
        through: CalendarDate,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// What was recorded about the recurrence, oldest first. Empty for another company's.
    fn history(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> impl Future<Output = Result<Vec<AuditRecord>, RepositoryError>> + Send;

    /// At most `limit` recurrences of the company that match `query`, in its order, starting right
    /// after `after` when given.
    fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &RecurrenceQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Recurrence>, RepositoryError>> + Send;

    /// At most `limit` active recurrences of **any company** that have something left to generate
    /// up to `until`: not generated through `until` yet, and not through their last day. For the
    /// scheduled job, which is the one reader that is not about one company.
    fn list_to_generate(
        &self,
        until: CalendarDate,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Recurrence>, RepositoryError>> + Send;
}
