use std::future::Future;

use conta_verde_domain::{Job, JobId, JobKind, NewJob};

use crate::RepositoryError;

/// Port for work that is done later and once ([ADR 0059]): a table of jobs a worker takes from. It is operational,
/// not a register of the business: no company, no audit trail, and a finished job leaves nothing behind.
///
/// Delivery is **at least once**: a worker that dies holding a job lets it be taken again when its lease ends, so what a
/// job does must be harmless to repeat or rare enough not to matter (an e-mail twice).
pub trait JobQueue: Send + Sync {
    fn enqueue(&self, job: &NewJob) -> impl Future<Output = Result<JobId, RepositoryError>> + Send;

    /// Takes the next job **of one of these kinds** that is due at `now` (Unix seconds): one waiting whose `run_at` has come, or
    /// one a worker took and did not finish before its lease ran out. The job is held for `lease_seconds` and the try is
    /// counted. Two workers never get the same job. A worker that does only some kinds (the one that sends webhooks must not
    /// wait behind a slow mail server, nor the other way round) says which.
    fn claim_next_of(
        &self,
        now: i64,
        lease_seconds: i64,
        kinds: &[JobKind],
    ) -> impl Future<Output = Result<Option<Job>, RepositoryError>> + Send;

    /// Takes the next due job of any kind.
    fn claim_next(
        &self,
        now: i64,
        lease_seconds: i64,
    ) -> impl Future<Output = Result<Option<Job>, RepositoryError>> + Send {
        self.claim_next_of(now, lease_seconds, &JobKind::ALL)
    }

    /// The job is done: nothing of it is kept.
    fn complete(&self, id: JobId) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// A try failed: the job waits until `retry_at`, or is given up on when that is `None` (it is kept, without its
    /// arguments, so that someone can see what failed). `error` is for the operator and never reaches a client.
    fn fail(
        &self,
        id: JobId,
        error: &str,
        retry_at: Option<i64>,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
}
