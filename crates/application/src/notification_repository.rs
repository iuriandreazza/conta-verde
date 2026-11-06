use std::future::Future;

use conta_verde_domain::{CompanyId, EntryId, Notification, NotificationId, NotificationStatus};

use crate::RepositoryError;

/// What claiming a reminder found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// Nobody tried this reminder before: it is stored, pending, on its first attempt.
    New(Notification),
    /// An earlier attempt failed and there are attempts left: it is pending again, one attempt more.
    Retry(Notification),
    /// Sent, being sent, or out of attempts: not to be sent now.
    Done,
}

/// Port for the reminders sent to customers ([ADR 0048]): a log with a unique key per installment, rule
/// days and channel. Nothing is audited here.
pub trait NotificationRepository: Send + Sync {
    /// Takes the reminder to send it. The store decides, atomically, so two passes at once cannot both get
    /// it: `New` when the key was never claimed, `Retry` when it failed and has attempts left, otherwise
    /// `Done`.
    fn claim(
        &self,
        notification: &Notification,
    ) -> impl Future<Output = Result<Claim, RepositoryError>> + Send;

    /// Says how the try ended: `Sent`, or `Failed` with a short reason. `NotFound` when the company has no
    /// such reminder.
    fn finish(
        &self,
        company_id: CompanyId,
        id: NotificationId,
        status: NotificationStatus,
        note: Option<&str>,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The reminders about an entry of the company, oldest first. Empty for another company's.
    fn list_by_entry(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> impl Future<Output = Result<Vec<Notification>, RepositoryError>> + Send;
}
