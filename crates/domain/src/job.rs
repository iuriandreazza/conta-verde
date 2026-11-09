use crate::{DomainError, JobId};

/// What a job asks to be done. The key is stored, so it does not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// Sends one e-mail: `to`, `subject`, `body`, and the `html` of the layout when it has one (a job queued before the
    /// layout has only the first three).
    SendEmail,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SendEmail => "send_email",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "send_email" => Some(Self::SendEmail),
            _ => None,
        }
    }

    /// How many arguments the kind takes, at least and at most.
    fn arity(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Self::SendEmail => 3..=4,
        }
    }
}

/// How many times a job is tried before it is given up on.
pub const MAX_ATTEMPTS: u32 = 5;
/// The longest a failed job waits before it is tried again.
const MAX_RETRY_DELAY_SECONDS: i64 = 60 * 60;
/// What one argument may weigh: a message is text, never a file.
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;

/// Work to do later, once: a kind and the text it needs. Arguments are plain text (no serialization format sits in the
/// domain) and Postgres refuses a NUL byte, so one is refused here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewJob {
    kind: JobKind,
    args: Vec<String>,
    run_at: i64,
}

impl NewJob {
    /// `run_at` is a moment in Unix seconds: the job is not tried before it.
    pub fn new(kind: JobKind, args: Vec<String>, run_at: i64) -> Result<Self, DomainError> {
        if !kind.arity().contains(&args.len()) {
            return Err(DomainError::InvalidJob("wrong number of arguments"));
        }
        if args
            .iter()
            .any(|arg| arg.contains('\0') || arg.len() > MAX_ARGUMENT_BYTES)
        {
            return Err(DomainError::InvalidJob(
                "argument is too large or has a NUL",
            ));
        }
        Ok(Self { kind, args, run_at })
    }

    pub fn kind(&self) -> JobKind {
        self.kind
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn run_at(&self) -> i64 {
        self.run_at
    }
}

/// A job a worker has taken: it is on its `attempts`th try, counting this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    pub args: Vec<String>,
    pub attempts: u32,
}

impl Job {
    /// When to try again after a failed try, or `None` when it has been tried enough: the wait doubles from a
    /// minute and stops growing at an hour.
    pub fn retry_after(&self) -> Option<i64> {
        (self.attempts < MAX_ATTEMPTS).then(|| {
            let doubled = 60_i64.saturating_mul(1_i64 << (self.attempts.saturating_sub(1)).min(20));
            doubled.min(MAX_RETRY_DELAY_SECONDS)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(attempts: u32) -> Job {
        Job {
            id: JobId::generate(),
            kind: JobKind::SendEmail,
            args: vec![],
            attempts,
        }
    }

    #[test]
    fn a_job_takes_the_arguments_of_its_kind_without_a_nul() {
        let args = |n: usize| vec!["x".to_owned(); n];
        assert!(NewJob::new(JobKind::SendEmail, args(3), 0).is_ok());
        assert!(NewJob::new(JobKind::SendEmail, args(2), 0).is_err());
        assert!(
            NewJob::new(
                JobKind::SendEmail,
                vec!["a".into(), "b\0".into(), "c".into()],
                0
            )
            .is_err()
        );
        assert!(
            NewJob::new(
                JobKind::SendEmail,
                vec!["a".into(), "b".into(), "c".repeat(300_000)],
                0
            )
            .is_err()
        );
    }

    #[test]
    fn a_failed_job_waits_longer_each_time_and_is_given_up_after_the_last_try() {
        let waits: Vec<Option<i64>> = (1..=6).map(|n| job(n).retry_after()).collect();
        assert_eq!(
            waits,
            [Some(60), Some(120), Some(240), Some(480), None, None]
        );
    }

    #[test]
    fn the_keys_round_trip() {
        assert_eq!(
            JobKind::parse(JobKind::SendEmail.as_str()),
            Some(JobKind::SendEmail)
        );
        assert_eq!(JobKind::parse("other"), None);
    }
}
