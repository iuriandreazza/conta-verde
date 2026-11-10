use std::sync::{Arc, Mutex};

use conta_verde_application::{JobQueue, RepositoryError};
use conta_verde_domain::{Job, JobId, JobKind, NewJob};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pending,
    Running { until: i64 },
    Dead,
}

struct Row {
    id: JobId,
    job: NewJob,
    run_at: i64,
    status: Status,
    attempts: u32,
}

/// Keeps the jobs in process memory, for tests and for running the API without a database. Clones share the same
/// queue.
#[derive(Default, Clone)]
pub struct InMemoryJobQueue {
    rows: Arc<Mutex<Vec<Row>>>,
}

impl InMemoryJobQueue {
    /// The jobs given up on, for a test to look at.
    pub fn dead(&self) -> usize {
        let rows = self.rows.lock().expect("queue lock poisoned");
        rows.iter().filter(|row| row.status == Status::Dead).count()
    }

    /// The jobs waiting or running.
    pub fn open(&self) -> usize {
        let rows = self.rows.lock().expect("queue lock poisoned");
        rows.iter().filter(|row| row.status != Status::Dead).count()
    }
}

impl JobQueue for InMemoryJobQueue {
    async fn enqueue(&self, job: &NewJob) -> Result<JobId, RepositoryError> {
        let id = JobId::generate();
        let mut rows = self.rows.lock().expect("queue lock poisoned");
        rows.push(Row {
            id,
            job: job.clone(),
            run_at: job.run_at(),
            status: Status::Pending,
            attempts: 0,
        });
        Ok(id)
    }

    async fn claim_next_of(
        &self,
        now: i64,
        lease_seconds: i64,
        kinds: &[JobKind],
    ) -> Result<Option<Job>, RepositoryError> {
        let mut rows = self.rows.lock().expect("queue lock poisoned");
        let due = |row: &&mut Row| {
            kinds.contains(&row.job.kind())
                && match row.status {
                    Status::Pending => row.run_at <= now,
                    Status::Running { until } => until <= now,
                    Status::Dead => false,
                }
        };
        let Some(row) = rows
            .iter_mut()
            .filter(due)
            .min_by_key(|row| (row.run_at, row.id.as_uuid()))
        else {
            return Ok(None);
        };
        row.attempts += 1;
        row.status = Status::Running {
            until: now + lease_seconds,
        };
        Ok(Some(Job {
            id: row.id,
            kind: row.job.kind(),
            args: row.job.args().to_vec(),
            attempts: row.attempts,
        }))
    }

    async fn complete(&self, id: JobId) -> Result<(), RepositoryError> {
        let mut rows = self.rows.lock().expect("queue lock poisoned");
        rows.retain(|row| row.id != id);
        Ok(())
    }

    async fn fail(
        &self,
        id: JobId,
        _error: &str,
        retry_at: Option<i64>,
    ) -> Result<(), RepositoryError> {
        let mut rows = self.rows.lock().expect("queue lock poisoned");
        if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
            match retry_at {
                Some(at) => {
                    row.status = Status::Pending;
                    row.run_at = at;
                }
                None => row.status = Status::Dead,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::{
        EmailMessage, EmailSender, JobReport, JobRunner, QueuedEmailSender,
    };
    use conta_verde_domain::{Email, JobKind};

    use super::*;
    use crate::{InMemoryEmailSender, contract};

    #[tokio::test]
    async fn claims_jobs_by_kind() {
        crate::contract_integrations::claims_jobs_by_kind(&InMemoryJobQueue::default()).await;
    }

    #[tokio::test]
    async fn runs_the_job_queue() {
        contract::runs_the_job_queue(&InMemoryJobQueue::default()).await;
    }

    fn message() -> EmailMessage {
        EmailMessage {
            to: Email::parse("bia@example.com").unwrap(),
            subject: "Convite".into(),
            body: "link".into(),
            html: None,
        }
    }

    const DONE: JobReport = JobReport {
        done: 1,
        retried: 0,
        dead: 0,
    };
    const RETRIED: JobReport = JobReport {
        done: 0,
        retried: 1,
        dead: 0,
    };
    const DEAD: JobReport = JobReport {
        done: 0,
        retried: 0,
        dead: 1,
    };

    #[tokio::test]
    async fn a_queued_message_is_sent_by_the_runner_once() {
        let (queue, mail) = (InMemoryJobQueue::default(), InMemoryEmailSender::default());
        QueuedEmailSender::new(queue.clone(), true)
            .send(&message())
            .await
            .unwrap();
        assert!(mail.sent().is_empty(), "queuing sends nothing");

        let runner = JobRunner::new(queue.clone(), mail.clone());
        assert_eq!(runner.run_due(10, 10).await.unwrap(), DONE);
        assert_eq!(mail.sent(), [message()]);
        assert_eq!(runner.run_due(10, 10).await.unwrap(), JobReport::default());
        assert_eq!(queue.open(), 0);
    }

    #[tokio::test]
    async fn a_message_keeps_its_html_through_the_queue() {
        let (queue, mail) = (InMemoryJobQueue::default(), InMemoryEmailSender::default());
        let message = EmailMessage {
            html: Some("<p>layout</p>".into()),
            ..message()
        };
        QueuedEmailSender::new(queue.clone(), true)
            .send(&message)
            .await
            .unwrap();

        JobRunner::new(queue, mail.clone())
            .run_due(10, 10)
            .await
            .unwrap();

        assert_eq!(mail.sent(), [message]);
    }

    #[tokio::test]
    async fn a_job_queued_before_the_layout_has_three_arguments_and_still_runs() {
        let (queue, mail) = (InMemoryJobQueue::default(), InMemoryEmailSender::default());
        let old = conta_verde_domain::NewJob::new(
            JobKind::SendEmail,
            vec!["bia@example.com".into(), "Convite".into(), "link".into()],
            0,
        )
        .unwrap();
        queue.enqueue(&old).await.unwrap();

        assert_eq!(
            JobRunner::new(queue, mail.clone())
                .run_due(1, 10)
                .await
                .unwrap(),
            DONE
        );
        assert_eq!(mail.sent(), [message()]);
    }

    #[tokio::test]
    async fn a_refused_message_is_tried_again_later_and_given_up_after_the_last_try() {
        let queue = InMemoryJobQueue::default();
        QueuedEmailSender::new(queue.clone(), true)
            .send(&message())
            .await
            .unwrap();
        let runner = JobRunner::new(queue.clone(), InMemoryEmailSender::broken());

        // Each failure waits longer: 1, 2, 4 and 8 minutes, and the fifth ends it.
        let mut now = 0;
        for wait in [60, 120, 240, 480] {
            assert_eq!(runner.run_due(now, 10).await.unwrap(), RETRIED);
            assert_eq!(
                runner.run_due(now + wait - 1, 10).await.unwrap(),
                JobReport::default(),
                "not before its time"
            );
            now += wait;
        }
        assert_eq!(runner.run_due(now, 10).await.unwrap(), DEAD);
        assert_eq!((queue.dead(), queue.open()), (1, 0));
        assert_eq!(
            runner.run_due(now + 10_000, 10).await.unwrap(),
            JobReport::default()
        );
    }

    #[tokio::test]
    async fn a_message_goes_through_once_the_server_is_back() {
        let (queue, mail) = (InMemoryJobQueue::default(), InMemoryEmailSender::default());
        QueuedEmailSender::new(queue.clone(), true)
            .send(&message())
            .await
            .unwrap();
        let down = JobRunner::new(queue.clone(), InMemoryEmailSender::broken());
        assert_eq!(down.run_due(0, 10).await.unwrap(), RETRIED);
        let up = JobRunner::new(queue, mail.clone());
        assert_eq!(up.run_due(60, 10).await.unwrap(), DONE);
        assert_eq!(mail.sent().len(), 1);
    }

    #[tokio::test]
    async fn without_a_mail_server_nothing_is_queued_and_the_message_is_said_not_sent() {
        let queue = InMemoryJobQueue::default();
        let sender = QueuedEmailSender::new(queue.clone(), false);
        assert!(!sender.is_enabled());
        assert!(sender.send(&message()).await.is_err());
        assert_eq!(queue.open(), 0);
    }

    #[tokio::test]
    async fn a_job_that_cannot_run_is_given_up_without_trying() {
        let (queue, mail) = (InMemoryJobQueue::default(), InMemoryEmailSender::default());
        let broken = conta_verde_domain::NewJob::new(
            JobKind::SendEmail,
            vec!["not an address".into(), "s".into(), "b".into()],
            0,
        )
        .unwrap();
        queue.enqueue(&broken).await.unwrap();
        let runner = JobRunner::new(queue, mail.clone());
        assert_eq!(runner.run_due(1, 10).await.unwrap(), DEAD);
        assert!(mail.sent().is_empty());
    }
}
