use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use conta_verde_application::{CnpjLookupError, CnpjRegistry, RegistryEntry};
use conta_verde_domain::Cnpj;

// A cache that only grows is a memory leak; past this many companies it is emptied and refilled.
const MAX_ENTRIES: usize = 10_000;

// After the registry fails, calls are held back this long instead of piling onto it.
const DEFAULT_COOLDOWN: Duration = Duration::from_secs(30);

/// Protects the registry's strict, installation-wide rate limit in two ways.
///
/// It remembers companies the registry has already answered for, so a repeated CNPJ (a retry, the
/// same supplier in two companies) does not spend another call. Only found companies are cached:
/// a miss is asked again, so a company that appears later is never hidden.
///
/// And it is a circuit breaker: once a lookup fails (rate limit, server error, timeout), further
/// lookups answer "unavailable" at once for a short cooldown, without calling the registry. That
/// stops a burst of registrations from hammering a service that already refused, and from holding
/// a request for the full timeout each time.
pub struct CachedCnpjRegistry<C> {
    inner: C,
    ttl: Duration,
    cooldown: Duration,
    entries: Mutex<HashMap<String, (Instant, RegistryEntry)>>,
    resting_until: Mutex<Option<Instant>>,
}

impl<C: CnpjRegistry> CachedCnpjRegistry<C> {
    pub fn new(inner: C, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            cooldown: DEFAULT_COOLDOWN,
            entries: Mutex::default(),
            resting_until: Mutex::default(),
        }
    }

    /// How long lookups are answered "unavailable" without calling the registry after it failed.
    pub fn with_cooldown(mut self, cooldown: Duration) -> Self {
        self.cooldown = cooldown;
        self
    }

    fn resting(&self) -> bool {
        let until = *self
            .resting_until
            .lock()
            .expect("registry rest lock poisoned");
        until.is_some_and(|until| Instant::now() < until)
    }

    fn rest(&self) {
        *self
            .resting_until
            .lock()
            .expect("registry rest lock poisoned") = Some(Instant::now() + self.cooldown);
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
        if self.resting() {
            return Err(CnpjLookupError::Unavailable);
        }
        match self.inner.lookup(cnpj).await {
            Ok(entry) => {
                self.remember(cnpj.as_str(), &entry);
                Ok(entry)
            }
            Err(CnpjLookupError::Unavailable) => {
                self.rest();
                Err(CnpjLookupError::Unavailable)
            }
            Err(other) => Err(other),
        }
    }

    /// Asks the registry and updates what is remembered: a fresh answer replaces the old one, and
    /// an authoritative "not found" drops it. An outage leaves it alone, since it proves nothing,
    /// but it starts the cooldown, and a refresh during the cooldown is not sent either: the user
    /// asked for fresh data, but the registry has just refused and its limit is shared.
    async fn refresh(&self, cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
        if self.resting() {
            return Err(CnpjLookupError::Unavailable);
        }
        match self.inner.refresh(cnpj).await {
            Ok(entry) => {
                self.remember(cnpj.as_str(), &entry);
                Ok(entry)
            }
            Err(CnpjLookupError::NotFound) => {
                self.forget(cnpj.as_str());
                Err(CnpjLookupError::NotFound)
            }
            Err(CnpjLookupError::Unavailable) => {
                self.rest();
                Err(CnpjLookupError::Unavailable)
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
    async fn misses_and_outages_are_asked_again_once_the_cooldown_is_over() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![
                Err(CnpjLookupError::Unavailable),
                Err(CnpjLookupError::NotFound),
                Ok(padaria()),
            ]),
            Duration::from_secs(60),
        )
        .with_cooldown(Duration::ZERO);
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
        )
        .with_cooldown(Duration::ZERO);
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::Unavailable));
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::Disabled));
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::NotFound));
    }

    #[tokio::test]
    async fn a_refresh_is_not_sent_while_the_registry_is_resting() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![Err(CnpjLookupError::Unavailable), Ok(padaria())]),
            Duration::from_secs(60),
        );
        // The first refresh fails and starts the cooldown; the second is held back without a call.
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::Unavailable));
        assert_eq!(refresh(&cached).await, Err(CnpjLookupError::Unavailable));
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn after_a_failure_the_registry_is_left_alone_for_the_cooldown() {
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![Err(CnpjLookupError::Unavailable), Ok(padaria())]),
            Duration::from_secs(60),
        );
        assert_eq!(lookup(&cached).await, Err(CnpjLookupError::Unavailable));
        // Answered at once, without another call to a service that just refused.
        for _ in 0..3 {
            assert_eq!(lookup(&cached).await, Err(CnpjLookupError::Unavailable));
        }
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_miss_does_not_start_a_cooldown() {
        // "Not found" is an answer, not a failure: the next CNPJ is asked right away.
        let cached = CachedCnpjRegistry::new(
            Scripted::new(vec![Err(CnpjLookupError::NotFound), Ok(padaria())]),
            Duration::from_secs(60),
        );
        assert_eq!(lookup(&cached).await, Err(CnpjLookupError::NotFound));
        assert_eq!(lookup(&cached).await, Ok(padaria()));
        assert_eq!(cached.inner.calls.load(Ordering::SeqCst), 2);
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
