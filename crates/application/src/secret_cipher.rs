use conta_verde_domain::{CompanyId, SecretText};

use crate::SecretError;

/// Port to what seals a secret before it is stored and opens it when it is used ([ADR 0085]): a sibling of [`SecretService`](crate::SecretService),
/// with the adapter in `infrastructure` and the key in the environment.
///
/// A secret is sealed **for** a company and a field: opening it under another company or another field fails, so a stored value
/// cannot be moved to where it was not written. An implementation does its own authentication (a changed byte is an error) and uses a
/// fresh nonce each time, so sealing the same value twice gives two different results.
pub trait SecretCipher: Send + Sync {
    /// Whether there is a key. Without one nothing can be sealed and nothing sealed can be opened: saving a secret is refused.
    fn is_available(&self) -> bool;

    fn seal(
        &self,
        company_id: CompanyId,
        field: &str,
        plaintext: &SecretText,
    ) -> Result<Vec<u8>, SecretError>;

    fn open(
        &self,
        company_id: CompanyId,
        field: &str,
        sealed: &[u8],
    ) -> Result<SecretText, SecretError>;
}
