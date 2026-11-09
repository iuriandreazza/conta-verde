use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CalendarDate, CompanyId, PeriodLock, period_lock_changes,
    period_lock_creation_changes, reopen_reason,
};

use crate::audit::audit_entry;
use crate::{ApplicationError, PeriodLockRepository};

/// The day a write of a dated record is made on, checked against the lock before the write ([ADR 0068]). A lock
/// is `through` a day: everything on or before it is closed.
pub(crate) fn ensure_open(
    through: Option<CalendarDate>,
    day: CalendarDate,
) -> Result<(), ApplicationError> {
    match through {
        Some(through) if day <= through => Err(ApplicationError::PeriodLocked { through }),
        _ => Ok(()),
    }
}

/// Closing and reopening a company's months ([ADR 0067], [ADR 0068]).
pub struct PeriodLockService<R> {
    repository: R,
}

impl<R: PeriodLockRepository> PeriodLockService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// The company's lock; a company that never closed a month has none.
    pub async fn get(&self, company_id: CompanyId) -> Result<PeriodLock, ApplicationError> {
        Ok(self
            .repository
            .find(company_id)
            .await?
            .unwrap_or_else(|| PeriodLock::open(company_id)))
    }

    /// Closes through `through` (the last day of a month) or, when that is earlier than the lock or `None`,
    /// reopens: a reopening needs a reason, which the trail keeps. The same date again writes nothing.
    pub async fn replace(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        through: Option<CalendarDate>,
        reason: Option<&str>,
    ) -> Result<PeriodLock, ApplicationError> {
        let stored = self.repository.find(company_id).await?;
        let before = stored.unwrap_or_else(|| PeriodLock::open(company_id));
        let after = PeriodLock::through(company_id, through)?;
        if before == after {
            return Ok(after);
        }
        let reason = if before.is_reopened_by(&after) {
            Some(reopen_reason(reason.unwrap_or_default())?)
        } else {
            None
        };
        let (action, changes) = match stored {
            Some(_) => (
                AuditAction::Updated,
                period_lock_changes(&before, &after, reason.as_deref()),
            ),
            None => (AuditAction::Created, period_lock_creation_changes(&after)),
        };
        let audit = audit_entry(&after, actor, action, changes);
        self.repository.save(&after, &audit).await?;
        Ok(after)
    }

    /// What was recorded about the company's lock, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        Ok(self.repository.history(company_id).await?)
    }
}
