use std::future::Future;

/// Port for the cryptography behind authentication, so use cases stay free of hashing libraries.
pub trait SecretService: Send + Sync {
    fn hash_password(
        &self,
        password: &str,
    ) -> impl Future<Output = Result<String, SecretError>> + Send;

    /// With `stored_hash` absent it still does the work of a real check, so response time
    /// does not reveal whether an email is registered.
    fn verify_password(
        &self,
        password: &str,
        stored_hash: Option<&str>,
    ) -> impl Future<Output = bool> + Send;

    /// A fresh unguessable session token, safe to place in a cookie.
    fn new_session_token(&self) -> String;

    /// Deterministic digest under which a session token is stored.
    fn digest_session_token(&self, token: &str) -> String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretError(pub String);

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "secret service failure: {}", self.0)
    }
}

impl std::error::Error for SecretError {}
