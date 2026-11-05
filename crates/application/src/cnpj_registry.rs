use std::future::Future;

use conta_verde_domain::Cnpj;

/// What the federal registry says about a company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    pub legal_name: String,
    /// The registry's own wording, e.g. "ATIVA" or "BAIXADA".
    pub situation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CnpjLookupError {
    /// The registry answered and has no company with this CNPJ. Its data lags the source, so this
    /// does not prove the company does not exist.
    NotFound,
    /// No trustworthy answer: network failure, timeout, rate limit or server error. Adapters log
    /// the cause; callers only need to know the CNPJ could not be checked right now.
    Unavailable,
    /// The operator turned the lookup off, so nothing was asked and nothing is claimed.
    Disabled,
}

/// Port for looking a CNPJ up in Receita Federal's registry. Futures are `Send` so use cases can
/// run on a multi-threaded runtime.
pub trait CnpjRegistry: Send + Sync {
    fn lookup(
        &self,
        cnpj: Cnpj<'_>,
    ) -> impl Future<Output = Result<RegistryEntry, CnpjLookupError>> + Send;

    /// Like `lookup`, but never answered from a cache: used when the user asks to check again and
    /// expects fresh data. Registries without a cache need not override it.
    fn refresh(
        &self,
        cnpj: Cnpj<'_>,
    ) -> impl Future<Output = Result<RegistryEntry, CnpjLookupError>> + Send {
        self.lookup(cnpj)
    }

    /// Whether the operator left the lookup on. A bulk import does not ask the registry for each
    /// row, and needs to know whether that leaves the CNPJs unchecked or the lookup was never wanted.
    fn is_enabled(&self) -> bool {
        true
    }
}

/// `None` means the operator switched the lookup off.
impl<T: CnpjRegistry> CnpjRegistry for Option<T> {
    async fn lookup(&self, cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
        match self {
            Some(registry) => registry.lookup(cnpj).await,
            None => Err(CnpjLookupError::Disabled),
        }
    }

    async fn refresh(&self, cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
        match self {
            Some(registry) => registry.refresh(cnpj).await,
            None => Err(CnpjLookupError::Disabled),
        }
    }

    fn is_enabled(&self) -> bool {
        self.is_some()
    }
}
