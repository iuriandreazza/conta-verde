use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use conta_verde_application::{CnpjLookupError, CnpjRegistry, RegistryEntry};
use conta_verde_domain::Cnpj;

// A cache that only grows is a memory leak; past this many companies it is emptied and refilled.
const MAX_ENTRIES: usize = 10_000;

/// Remembers companies the registry has already answered for. The free registry has a strict rate
/// limit shared by everyone on the installation, so a repeated CNPJ (a retry, the same supplier in
/// two companies) should not spend another call. Only found companies are cached: a miss or an
/// outage is asked again, so a later success is never hidden.
pub struct CachedCnpjRegistry<C> {
    inner: C,
    ttl: Duration,
    entries: Mutex<HashMap<String, (Instant, RegistryEntry)>>,
}

impl<C: CnpjRegistry> CachedCnpjRegistry<C> {
    pub fn new(inner: C, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            entries: Mutex::default(),
        }
    }

    fn fresh(&self, cnpj: &str) -> Option<RegistryEntry> {
        let entries = self.entries.lock().expect("registry cache lock poisoned");
        entries
            .get(cnpj)
            .filter(|(stored_at, _)| stored_at.elapsed() < self.ttl)
            .map(|(_, entry)| entry.clone())
    }

    fn forget(&self, cnpj: &str) {
        self.entries
            .lock()
            .expect("registry cache lock poisoned")
            .remove(cnpj);
    }

    fn remember(&self, cnpj: &str, entry: &RegistryEntry) {
        let mut entries = self.entries.lock().expect("registry cache lock poisoned");
        if entries.len() >= MAX_ENTRIES {
            entries.clear();
        }
        entries.insert(cnpj.to_owned(), (Instant::now(), entry.clone()));
    }
}

impl<C: CnpjRegistry> CnpjRegistry for CachedCnpjRegistry<C> {
    async fn lookup(&self, cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
        if let Some(entry) = self.fresh(cnpj.as_str()) {
            return Ok(entry);
        }
        let entry = self.inner.lookup(cnpj).await?;
        self.remember(cnpj.as_str(), &entry);
        Ok(entry)
    }

    /// Asks the registry and updates what is remembered: a fresh answer replaces the old one, and
    /// an authoritative "not found" drops it. An outage leaves it alone, since it proves nothing.
    async fn refresh(&self, cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
        match self.inner.refresh(cnpj).await {
            Ok(entry) => {
                self.remember(cnpj.as_str(), &entry);
                Ok(entry)
            }
            Err(CnpjLookupError::NotFound) => {
                self.forget(cnpj.as_str());
                Err(CnpjLookupError::NotFound)
            }
            Err(other) => Err(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use conta_verde_domain::Document;

    use super::*;

    /// Counts how many lookups reach it and answers with the next scripted result.
    struct Scripted {
        calls: AtomicUsize,
        answers: Mutex<Vec<Result<RegistryEntry, CnpjLookupError>>>,
    }

    impl Scripted {
        fn new(answers: Vec<Result<RegistryEntry, CnpjLookupError>>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                answers: Mutex::new(answers),
            }
        }
    }

    impl CnpjRegistry for Scripted {
        async fn lookup(&self, _cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answers.lock().unwrap().remove(0)
        }
    }

    fn padaria() -> RegistryEntry {
        RegistryEntry {
            legal_name: "PADARIA CENTRAL LTDA".into(),
            situation: "ATIVA".into(),
        }
    }

    async fn lookup<C: CnpjRegistry>(registry: &C) -> Result<RegistryEntry, CnpjLookupError> {
        let document = Document::parse("11.222.333/0001-81").unwrap();
        registry.lookup(document.cnpj().unwrap()).await
    }

    #[tokio::test]
    async fn a_found_company_is_asked_for_only_once() {
        let cached =
            CachedCnpjRegistry::new(Scripted::new(vec![Ok(padaria())]), Duration::from_secs(60));
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn misses_and_outages_are_asked_again() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![
                Err(CnpjLookupError::Unavailable),
                Err(CnpjLookupError::NotFound),
                Ok(padaria()),
            ]),
            Duration::from_secs(60),
        );
        assert_eq!(lookup(&cached).await, Err(CnpjLookupError::Unavailable));
        assert_eq!(lookup(&cached).await, Err(CnpjLookupError::NotFound));
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 3);
    }

    fn closed() -> RegistryEntry {
        RegistryEntry {
            situation: "BAIXADA".into(),
            ..padaria()
        }
    }

    async fn refresh<C: CnpjRegistry>(registry: &C) -> Result<RegistryEntry, CnpjLookupError> {
        let document = Document::parse("11.222.333/0001-81").unwrap();
        registry.refresh(document.cnpj().unwrap()).await
    }

    #[tokio::test]
    async fn a_refresh_skips_the_cache_and_replaces_what_is_remembered() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![Ok(padaria()), Ok(closed())]),
            Duration::from_secs(60),
        );
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        // The company closed meanwhile: a plain lookup would keep saying ATIVA.
        assert_eq!(refresh(&cached).await, Ok(closed()));
        assert_eq!(lookup(&cached).await, Ok(closed()));
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_refresh_that_finds_nothing_forgets_the_company() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![
                Ok(padaria()),
                Err(CnpjLookupError::NotFound),
                Ok(padaria()),
            ]),
            Duration::from_secs(60),
        );
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::NotFound));
        // Not served from the cache any more: the registry is asked again.
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn a_refresh_during_an_outage_keeps_what_is_remembered() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![Ok(padaria()), Err(CnpjLookupError::Unavailable)]),
            Duration::from_secs(60),
        );
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::Unavailable));
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_refresh_with_nothing_remembered_passes_every_answer_through() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![
                Err(CnpjLookupError::Unavailable),
                Err(CnpjLookupError::Disabled),
                Err(CnpjLookupError::NotFound),
            ]),
            Duration::from_secs(60),
        );
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::Unavailable));
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::Disabled));
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::NotFound));
    }

    #[tokio::test]
    async fn an_expired_entry_is_asked_again() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![Ok(padaria()), Ok(padaria())]),
            Duration::ZERO,
        );
        lookup(&cached).await.unwrap();
        lookup(&cached).await.unwrap();
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 2);
    }
}
