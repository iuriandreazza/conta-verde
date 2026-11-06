use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CalendarDate, CompanyId, EntryOrigin, Recurrence,
    RecurrenceDraft, RecurrenceId, recurrence_changes, recurrence_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, EntryReferences, FinancialEntryRepository, FinancialEntryService, NameCursor,
    RecurrenceQuery, RecurrenceRepository, RepositoryError,
};

/// How many days ahead of today the occurrences are generated ([ADR 0033]).
pub const GENERATION_HORIZON_DAYS: u32 = 60;
/// The most occurrences one pass makes for one recurrence: a recurrence that starts years back
/// catches up over a few passes.
const MAX_PER_PASS: usize = 120;
/// The most recurrences one pass of the job takes up.
const MAX_PER_JOB_PASS: usize = 500;

/// What a pass of the scheduled job did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GenerationReport {
    /// Entries made.
    pub made: usize,
    /// Recurrences that could not be generated and will be tried again.
    pub failed: usize,
}

#[derive(Debug, Clone)]
pub struct RecurrencePage {
    pub items: Vec<Recurrence>,
    /// Present when more recurrences exist after this page.
    pub next: Option<NameCursor>,
}

/// Entries that come back every week, month or year, generated ahead ([ADR 0033]).
pub struct RecurrenceService<R, E, F> {
    repository: R,
    entries: FinancialEntryService<E, F>,
    references: F,
}

impl<R, E, F> RecurrenceService<R, E, F>
where
    R: RecurrenceRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
{
    /// `references` is asked about what a recurrence points at; the entries it generates are made
    /// through `entries`, which asks again, since a category can be deactivated afterwards.
    pub fn new(repository: R, entries: FinancialEntryService<E, F>, references: F) -> Self {
        Self {
            repository,
            entries,
            references,
        }
    }

    /// Validates the draft and what it points at, stores the recurrence with its creation in the
    /// audit trail, and generates what falls due in the next two months.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: RecurrenceDraft,
        today: CalendarDate,
    ) -> Result<Recurrence, ApplicationError> {
        let recurrence =
            Recurrence::new(RecurrenceId::generate(), company_id, draft, actor.clone())?;
        self.check_references(&recurrence, None).await?;
        let audit = audit_entry(
            &recurrence,
            actor,
            AuditAction::Created,
            recurrence_creation_changes(&recurrence),
        );
        self.repository.save(&recurrence, &audit).await?;
        self.generate_one(recurrence, today).await
    }

    /// Replaces the editable fields. A draft that changes nothing writes nothing and records
    /// nothing, and generates nothing new. What was generated is never touched.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: RecurrenceId,
        draft: RecurrenceDraft,
        today: CalendarDate,
    ) -> Result<Recurrence, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft, actor.clone())?;
        let changes = recurrence_changes(&before, &after);
        if changes.is_empty() {
            return Ok(before);
        }
        self.check_references(&after, Some(&before)).await?;
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(&after, &audit)
            .await
            .map_err(|error| match error {
                // It vanished between the read and the write.
                RepositoryError::NotFound => ApplicationError::RecurrenceNotFound,
                other => other.into(),
            })?;
        self.generate_one(after, today).await
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> Result<Recurrence, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the recurrence, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's recurrence answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &RecurrenceQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<RecurrencePage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(RecurrencePage { items, next })
    }

    /// The scheduled job: generates what falls due in the next two months for every active
    /// recurrence of every company. A recurrence whose entries cannot be made (its category was
    /// deactivated) is left as it was and tried again on the next pass; the others go on.
    pub async fn generate_all_due(
        &self,
        today: CalendarDate,
    ) -> Result<GenerationReport, ApplicationError> {
        let mut report = GenerationReport::default();
        let due = self
            .repository
            .list_to_generate(horizon(today), MAX_PER_JOB_PASS)
            .await?;
        for recurrence in due {
            match self.generate(&recurrence, today).await {
                Ok(count) => report.made += count,
                Err(_) => report.failed += 1,
            }
        }
        Ok(report)
    }

    /// Makes the entries of the occurrences between where the recurrence was generated through and
    /// the horizon. An occurrence that already has its entry is skipped, which is what makes a pass
    /// safe to repeat and to run in two places at once. Returns how many entries were made.
    async fn generate(
        &self,
        recurrence: &Recurrence,
        today: CalendarDate,
    ) -> Result<usize, ApplicationError> {
        if !recurrence.is_active() {
            return Ok(0);
        }
        let until = horizon(today);
        let days =
            recurrence.occurrences_after(recurrence.generated_through(), until, MAX_PER_PASS);
        let mut made = 0;
        for day in &days {
            let draft = recurrence.entry_draft(*day);
            let origin = EntryOrigin {
                recurrence_id: recurrence.id(),
                occurrence_on: *day,
            };
            match self
                .entries
                .register_generated(
                    recurrence.company_id(),
                    recurrence.saved_by(),
                    draft,
                    origin,
                )
                .await
            {
                Ok(_) => made += 1,
                Err(ApplicationError::Repository(RepositoryError::DuplicateOccurrence)) => {}
                Err(error) => return Err(error),
            }
        }
        // A full pass was made when the days ran out before the cap: everything up to the horizon,
        // or up to the last day of the recurrence, is done. Otherwise it goes on from the last day.
        let reached = if days.len() == MAX_PER_PASS {
            days.last().copied()
        } else {
            Some(recurrence.ends_on().map_or(until, |end| end.min(until)))
        };
        if let Some(through) = reached {
            self.repository
                .mark_generated(recurrence.company_id(), recurrence.id(), through)
                .await?;
        }
        Ok(made)
    }

    async fn generate_one(
        &self,
        recurrence: Recurrence,
        today: CalendarDate,
    ) -> Result<Recurrence, ApplicationError> {
        self.generate(&recurrence, today).await?;
        self.find(recurrence.company_id(), recurrence.id()).await
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: RecurrenceId,
    ) -> Result<Recurrence, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::RecurrenceNotFound)
    }

    /// What a recurrence points at has to be usable now; one that is kept as it was is not asked
    /// again, so a recurrence can still be paused after its category was deactivated.
    async fn check_references(
        &self,
        recurrence: &Recurrence,
        before: Option<&Recurrence>,
    ) -> Result<(), ApplicationError> {
        let company = recurrence.company_id();
        if before.is_none_or(|stored| stored.category_id() != recurrence.category_id())
            && !self
                .references
                .category_is_usable(company, recurrence.category_id())
                .await?
        {
            return Err(ApplicationError::UnusableReference("category"));
        }
        if let Some(id) = recurrence.cost_center_id()
            && before.is_none_or(|stored| stored.cost_center_id() != recurrence.cost_center_id())
            && !self.references.cost_center_is_usable(company, id).await?
        {
            return Err(ApplicationError::UnusableReference("cost center"));
        }
        if let Some(id) = recurrence.party_id()
            && before.is_none_or(|stored| stored.party_id() != recurrence.party_id())
            && !self.references.party_exists(company, id).await?
        {
            return Err(ApplicationError::UnusableReference("party"));
        }
        Ok(())
    }
}

/// The last day occurrences are generated for: two months ahead of today.
fn horizon(today: CalendarDate) -> CalendarDate {
    (0..GENERATION_HORIZON_DAYS)
        .try_fold(today, |day, _| day.succ())
        .unwrap_or(today)
}
