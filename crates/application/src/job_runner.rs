use conta_verde_domain::{Email, Job, JobKind, NewJob};

use crate::{ApplicationError, EmailError, EmailMessage, EmailSender, JobQueue};

/// How long a worker holds a job before another may take it: longer than a send takes, short enough to recover soon.
const LEASE_SECONDS: i64 = 5 * 60;

/// What one pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JobReport {
    pub done: usize,
    /// Failed, and waiting for another try.
    pub retried: usize,
    /// Given up on.
    pub dead: usize,
}

/// Does the jobs of the queue ([ADR 0059]): takes what is due, runs it and says how it went. The time is the caller's,
/// so a test moves the clock.
pub struct JobRunner<Q, E> {
    queue: Q,
    emails: E,
}

impl<Q, E> JobRunner<Q, E>
where
    Q: JobQueue,
    E: EmailSender,
{
    pub fn new(queue: Q, emails: E) -> Self {
        Self { queue, emails }
    }

    /// Runs the jobs due at `now` (Unix seconds), at most `limit` of them, one after the other.
    pub async fn run_due(&self, now: i64, limit: usize) -> Result<JobReport, ApplicationError> {
        let mut report = JobReport::default();
        for _ in 0..limit {
            let Some(job) = self
                .queue
                .claim_next_of(now, LEASE_SECONDS, &[JobKind::SendEmail])
                .await?
            else {
                break;
            };
            // A worker that died on this job every time it tried never got to say so: it has had its tries.
            if job.attempts > job.kind.max_attempts() {
                self.queue
                    .fail(job.id, "gave up after repeated crashes", None)
                    .await?;
                report.dead += 1;
                continue;
            }
            match self.execute(&job).await {
                Outcome::Done => {
                    self.queue.complete(job.id).await?;
                    report.done += 1;
                }
                Outcome::Failed(error) => {
                    let retry_at = job.retry_after().map(|wait| now + wait);
                    self.queue.fail(job.id, error, retry_at).await?;
                    if retry_at.is_some() {
                        report.retried += 1;
                    } else {
                        report.dead += 1;
                    }
                }
                Outcome::Unrunnable(error) => {
                    self.queue.fail(job.id, error, None).await?;
                    report.dead += 1;
                }
            }
        }
        Ok(report)
    }

    async fn execute(&self, job: &Job) -> Outcome {
        match job.kind {
            // Webhook deliveries are done by the webhook workers, which claim only their kind.
            JobKind::DeliverWebhook => {
                Outcome::Unrunnable("a webhook delivery is not done by this worker")
            }
            JobKind::SendEmail => {
                let (to, subject, body, html) = match job.args.as_slice() {
                    [to, subject, body] => (to, subject, body, None),
                    [to, subject, body, html] => (to, subject, body, Some(html)),
                    _ => {
                        return Outcome::Unrunnable(
                            "a send_email job needs three or four arguments",
                        );
                    }
                };
                let Ok(to) = Email::parse(to) else {
                    return Outcome::Unrunnable("the address is not valid");
                };
                let message = EmailMessage {
                    to,
                    subject: subject.clone(),
                    body: body.clone(),
                    html: html.cloned(),
                };
                match self.emails.send(&message).await {
                    Ok(()) => Outcome::Done,
                    Err(EmailError) => Outcome::Failed("the mail server did not take the message"),
                }
            }
        }
    }
}

enum Outcome {
    Done,
    /// Worth trying again.
    Failed(&'static str),
    /// Trying again cannot help.
    Unrunnable(&'static str),
}

/// An [`EmailSender`] that does not send: it queues the message for a [`JobRunner`] and answers that it was taken.
/// What asked for the e-mail (an invitation, a quote) therefore never waits for, nor fails with, the mail server, and
/// a message the server refuses is tried again later.
///
/// With no transport behind it (`enabled` false, nothing configured to send) it queues nothing and says the message
/// was not sent, so the page shows the link to pass on, as it did before the queue.
pub struct QueuedEmailSender<Q> {
    queue: Q,
    enabled: bool,
}

impl<Q: JobQueue> QueuedEmailSender<Q> {
    pub fn new(queue: Q, enabled: bool) -> Self {
        Self { queue, enabled }
    }
}

impl<Q: JobQueue> EmailSender for QueuedEmailSender<Q> {
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn send(&self, message: &EmailMessage) -> Result<(), EmailError> {
        if !self.enabled {
            return Err(EmailError);
        }
        // Due at once: any moment already past is.
        let mut args = vec![
            message.to.as_str().to_owned(),
            message.subject.clone(),
            message.body.clone(),
        ];
        args.extend(message.html.clone());
        let job = NewJob::new(JobKind::SendEmail, args, 0).map_err(|_| EmailError)?;
        self.queue
            .enqueue(&job)
            .await
            .map(|_| ())
            .map_err(|_| EmailError)
    }
}
