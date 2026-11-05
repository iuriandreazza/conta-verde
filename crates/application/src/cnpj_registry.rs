use std::future::Future;

/// What the federal registry says about a company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    pub legal_name: String,
    /// The registry's own wording, e.g. "ATIVA" or "BAIXADA".
    pub situation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CnpjLookupError {
    /// The registry answered and has no company with this CNPJ.
    NotFound,
    /// No trustworthy answer: network failure, timeout, rate limit, server error or lookup disabled.
    /// Adapters log the cause; callers only need to know the CNPJ could not be checked.
    Unavailable,
}

/// Port for looking a CNPJ up in Receita Federal's registry. Futures are `Send` so use cases can
/// run on a multi-threaded runtime. `cnpj` is the stored form: 14 uppercase characters, no punctuation.
pub trait CnpjRegistry: Send + Sync {
    fn lookup(
        &self,
        cnpj: &str,
    ) -> impl Future<Output = Result<RegistryEntry, CnpjLookupError>> + Send;
}

/// `None` means the lookup is switched off: it is always unavailable, so parties stay unverified.
impl<T: CnpjRegistry> CnpjRegistry for Option<T> {
    async fn lookup(&self, cnpj: &str) -> Result<RegistryEntry, CnpjLookupError> {
        match self {
            Some(registry) => registry.lookup(cnpj).await,
            None => Err(CnpjLookupError::Unavailable),
        }
    }
}
