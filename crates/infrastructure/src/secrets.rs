use std::fmt::Write;
use std::sync::Arc;

use argon2::password_hash::phc::PasswordHash;
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use conta_verde_application::{SecretError, SecretService};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

const SESSION_TOKEN_BYTES: usize = 32;

/// Argon2id passwords and random session tokens stored as SHA-256 digests.
pub struct Argon2SecretService {
    argon2: Argon2<'static>,
    // Checked when the email is unknown, so unknown and known emails cost the same time.
    decoy_hash: Arc<str>,
    // Each Argon2 run holds ~19 MiB, so unbounded concurrent logins could exhaust memory.
    concurrency: Arc<Semaphore>,
}

impl Argon2SecretService {
    pub fn new() -> Result<Self, SecretError> {
        let argon2 = Argon2::default();
        let decoy_hash = hash(&argon2, "decoy password")?;
        let workers = std::thread::available_parallelism().map_or(1, usize::from);
        Ok(Self {
            argon2,
            decoy_hash: decoy_hash.into(),
            concurrency: Arc::new(Semaphore::new(workers)),
        })
    }

    async fn run_blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, SecretError> {
        let permit = Arc::clone(&self.concurrency)
            .acquire_owned()
            .await
            .map_err(|error| SecretError(error.to_string()))?;
        // Argon2 is deliberately slow; keep it off the async worker threads.
        let result = tokio::task::spawn_blocking(move || {
            let output = work();
            drop(permit);
            output
        })
        .await;
        result.map_err(|error| SecretError(error.to_string()))
    }
}

impl SecretService for Argon2SecretService {
    async fn hash_password(&self, password: &str) -> Result<String, SecretError> {
        let argon2 = self.argon2.clone();
        let password = password.to_owned();
        self.run_blocking(move || hash(&argon2, &password)).await?
    }

    async fn verify_password(&self, password: &str, stored_hash: Option<&str>) -> bool {
        let argon2 = self.argon2.clone();
        let password = password.to_owned();
        let known = stored_hash.is_some();
        let stored_hash = stored_hash.unwrap_or(&self.decoy_hash).to_owned();
        let matches = self
            .run_blocking(move || {
                PasswordHash::new(&stored_hash).is_ok_and(|parsed| {
                    argon2.verify_password(password.as_bytes(), &parsed).is_ok()
                })
            })
            .await
            .unwrap_or(false);
        known && matches
    }

    fn new_session_token(&self) -> String {
        hex(&rand::random::<[u8; SESSION_TOKEN_BYTES]>())
    }

    fn digest_session_token(&self, token: &str) -> String {
        hex(&Sha256::digest(token.as_bytes()))
    }
}

fn hash(argon2: &Argon2<'_>, password: &str) -> Result<String, SecretError> {
    argon2
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| SecretError(error.to_string()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verifies_only_the_matching_password() {
        let secrets = Argon2SecretService::new().unwrap();
        let stored = secrets.hash_password("correct horse").await.unwrap();

        assert!(
            secrets
                .verify_password("correct horse", Some(&stored))
                .await
        );
        assert!(!secrets.verify_password("wrong", Some(&stored)).await);
        assert!(!secrets.verify_password("correct horse", None).await);
    }

    #[test]
    fn tokens_are_unique_and_digests_are_stable() {
        let secrets = Argon2SecretService::new().unwrap();
        let first = secrets.new_session_token();
        assert_ne!(first, secrets.new_session_token());
        assert_eq!(first.len(), SESSION_TOKEN_BYTES * 2);
        assert_eq!(
            secrets.digest_session_token(&first),
            secrets.digest_session_token(&first)
        );
        assert_ne!(secrets.digest_session_token(&first), first);
    }
}
