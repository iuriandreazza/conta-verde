use conta_verde_application::{JobQueue, RepositoryError};
use conta_verde_domain::{Job, JobId, JobKind, NewJob};
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;

/// Keeps the jobs in Postgres ([ADR 0059]). A worker takes a job in one statement that skips what another worker has
/// locked, so two workers never get the same one.
#[derive(Clone)]
pub struct PostgresJobQueue {
    pool: PgPool,
}

impl PostgresJobQueue {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl JobQueue for PostgresJobQueue {
    async fn enqueue(&self, job: &NewJob) -> Result<JobId, RepositoryError> {
        let id = JobId::generate();
        sqlx::query(
            "INSERT INTO jobs (id, kind, args, status, run_at) VALUES ($1, $2, $3, 'pending', $4)",
        )
        .bind(id.as_uuid())
        .bind(job.kind().as_str())
        .bind(job.args())
        .bind(job.run_at())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(id)
    }

    async fn claim_next(
        &self,
        now: i64,
        lease_seconds: i64,
    ) -> Result<Option<Job>, RepositoryError> {
        let row = sqlx::query(
            "UPDATE jobs SET status = 'running', attempts = attempts + 1, locked_until = $1 + $2 \
             WHERE id = (SELECT id FROM jobs \
                         WHERE (status = 'pending' AND run_at <= $1) \
                            OR (status = 'running' AND locked_until <= $1) \
                         ORDER BY run_at, id FOR UPDATE SKIP LOCKED LIMIT 1) \
             RETURNING id, kind, args, attempts",
        )
        .bind(now)
        .bind(lease_seconds)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.map(|row| {
            let kind: String = row.get("kind");
            let attempts: i32 = row.get("attempts");
            Ok(Job {
                id: JobId::from_uuid(row.get("id")),
                kind: JobKind::parse(&kind).ok_or_else(|| {
                    RepositoryError::Storage(format!("stored job of kind {kind:?}"))
                })?,
                args: row.get("args"),
                attempts: u32::try_from(attempts).unwrap_or(u32::MAX),
            })
        })
        .transpose()
    }

    async fn complete(&self, id: JobId) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM jobs WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn fail(
        &self,
        id: JobId,
        error: &str,
        retry_at: Option<i64>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE jobs SET status = CASE WHEN $3::bigint IS NULL THEN 'dead' ELSE 'pending' END, \
                 run_at = COALESCE($3, run_at), locked_until = NULL, last_error = $2, \
                 args = CASE WHEN $3::bigint IS NULL THEN '{}' ELSE args END \
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(error.replace('\0', ""))
        .bind(retry_at)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_the_job_queue() {
        let pool = contract::test_pool().await;
        contract::runs_the_job_queue(&PostgresJobQueue::new(pool)).await;
    }
}
