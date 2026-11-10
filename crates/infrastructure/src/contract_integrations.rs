//! Behavior every adapter of the public API's ports must share ([ADR 0087]), written once and run against each of them.

use conta_verde_application::{
    ApiKeyRepository, ApiKeyService, ApplicationError, AuthError, DocumentReferences,
    DocumentStock, EntryReferences, FinancialEntryRepository, JobQueue, QuoteRepository,
    SaleClaimRepository, SaleRepository, SaleService, SecretCipher, SecretService, SendOutcome,
    WebhookRepository, WebhookService, envelope,
};
use conta_verde_application::{
    BankConnectionRepository, BankSyncService, ItemState, ReconciliationService, RepositoryError,
    StatementLineQuery, StatementLineRepository, TransferRepository,
};
use conta_verde_application::{ConnectorRepository, ConnectorService};
use conta_verde_domain::{
    Access, Actor, ApiKeyDraft, ApiKeyId, Area, AuditAction, BankAccountId, CalendarDate,
    CompanyId, ConnectionStatus, DeliveryStatus, DomainError, MAX_API_KEYS,
    MAX_WEBHOOK_SUBSCRIPTIONS, SECRET_GRACE_MILLIS, SaleId, SaleStatus, WebhookEventKind,
    WebhookSubscriptionDraft,
};
use uuid::Uuid;

use crate::{FakeBankAggregator, InMemoryWebhookSender, webhook_signature};

use crate::contract::{EntrySeed, SalesSeed, actor, sale_draft};

const NOW: i64 = 1_800_000_000_000;
const HOUR: i64 = 3_600_000;

fn day() -> CalendarDate {
    CalendarDate::parse("2026-11-10").unwrap()
}

fn draft(name: &str, scopes: &[&str], hours: Option<u32>) -> ApiKeyDraft {
    ApiKeyDraft {
        name: name.into(),
        scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
        expires_in_hours: hours,
    }
}

fn refused() -> ApplicationError {
    AuthError::Unauthenticated.into()
}

/// A key is found by the digest of its secret and by nothing else; the secret is shown once and never stored or
/// written to the trail; a key of one company is nobody else's; one that has expired is refused.
pub async fn keeps_api_keys_by_the_digest_of_their_secret<R, S>(
    service: &ApiKeyService<R, S>,
    company: CompanyId,
    other_company: CompanyId,
) where
    R: ApiKeyRepository,
    S: SecretService,
{
    let ana = actor();
    let issued = service
        .create(
            company,
            &ana,
            draft(
                " Loja online ",
                &["sales:write", "parties:read", "sales:read"],
                None,
            ),
            NOW,
        )
        .await
        .unwrap();
    assert!(issued.secret.starts_with("cvk_") && issued.secret.len() > 40);
    assert!(issued.secret.starts_with(issued.key.label()));
    assert_eq!(issued.key.name(), "Loja online");
    assert_eq!(issued.key.scope_keys(), ["parties:read", "sales:write"]);

    // Found by its secret, in its own company, and the use is noted once a day.
    let found = service
        .authenticate(&issued.secret, NOW, day())
        .await
        .unwrap();
    assert_eq!(found.id(), issued.key.id());
    assert_eq!(found.company_id(), company);
    assert!(found.allows(Area::Sales, Access::Write));
    assert!(!found.allows(Area::Finance, Access::Read));
    let listed = service.list(company).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].last_used_on(), Some(day()));
    assert!(service.list(other_company).await.unwrap().is_empty());

    // Nothing but the exact secret opens it.
    let mut wrong = issued.secret.clone();
    wrong.pop();
    wrong.push(if issued.secret.ends_with('0') {
        '1'
    } else {
        '0'
    });
    for secret in [wrong.as_str(), "cvk_", "", "not a key", &issued.secret[4..]] {
        assert_eq!(
            service.authenticate(secret, NOW, day()).await.unwrap_err(),
            refused(),
            "{secret}"
        );
    }

    // The trail says who made it and what it can do, and never the secret or its digest.
    let trail = service.history(company, issued.key.id()).await.unwrap();
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].entry.action, AuditAction::Created);
    assert_eq!(trail[0].entry.actor, ana);
    let written = format!("{:?}", trail[0].entry.changes);
    assert!(written.contains("parties:read,sales:write"));
    assert!(!written.contains(&issued.secret[4..]));
    // Another company's key reads like an unknown one.
    assert_eq!(
        service
            .history(other_company, issued.key.id())
            .await
            .unwrap_err(),
        ApplicationError::ApiKeyNotFound
    );

    // A key that expires is refused from that moment on.
    let short = service
        .create(
            company,
            &ana,
            draft("Provisória", &["parties:read"], Some(1)),
            NOW,
        )
        .await
        .unwrap();
    assert!(
        service
            .authenticate(&short.secret, NOW + HOUR - 1, day())
            .await
            .is_ok()
    );
    assert_eq!(
        service
            .authenticate(&short.secret, NOW + HOUR, day())
            .await
            .unwrap_err(),
        refused()
    );

    // A rotation of a key that was made to expire gives the new one the same life, never a permanent one.
    let rotated = service
        .rotate(company, &ana, short.key.id(), 0, NOW)
        .await
        .unwrap();
    assert_eq!(rotated.key.expires_at(), Some(NOW + HOUR));
    service
        .revoke(company, &ana, rotated.key.id())
        .await
        .unwrap();

    // A scope a key may not have, and a key with none, are refused and leave nothing.
    for scopes in [vec!["fiscal:read"], vec!["members:write"], vec![]] {
        assert!(
            service
                .create(company, &ana, draft("Ruim", &scopes, None), NOW)
                .await
                .is_err(),
            "{scopes:?}"
        );
    }
    assert_eq!(service.list(company).await.unwrap().len(), 3);

    // A company keeps a bounded number of keys, revoked ones included.
    for number in 1..MAX_API_KEYS {
        service
            .create(
                other_company,
                &ana,
                draft(&format!("Chave {number}"), &["parties:read"], None),
                NOW,
            )
            .await
            .unwrap();
    }
    service
        .create(
            other_company,
            &ana,
            draft("A última", &["parties:read"], None),
            NOW,
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .create(
                other_company,
                &ana,
                draft("Demais", &["parties:read"], None),
                NOW
            )
            .await
            .unwrap_err(),
        DomainError::TooManyApiKeys.into()
    );
    // A revoked key is no credential: it stays listed but no longer takes a place.
    let listed = service.list(other_company).await.unwrap();
    service
        .revoke(other_company, &ana, listed[0].id())
        .await
        .unwrap();
    service
        .create(
            other_company,
            &ana,
            draft("De novo", &["parties:read"], None),
            NOW,
        )
        .await
        .unwrap();
}

/// A rotation makes a new key and lets the old one go on for a while; a revocation is at once; both are in the trail of the
/// keys they touched, and a key that is not active is not rotated.
pub async fn rotates_and_revokes_api_keys<R, S>(
    service: &ApiKeyService<R, S>,
    company: CompanyId,
    other_company: CompanyId,
) where
    R: ApiKeyRepository,
    S: SecretService,
{
    let ana = actor();
    let old = service
        .create(
            company,
            &ana,
            draft("Loja", &["sales:write", "products:read"], None),
            NOW,
        )
        .await
        .unwrap();

    // Another company's key cannot be rotated or revoked, and a grace beyond a week is refused.
    assert_eq!(
        service
            .rotate(other_company, &ana, old.key.id(), 1, NOW)
            .await
            .unwrap_err(),
        ApplicationError::ApiKeyNotFound
    );
    assert_eq!(
        service
            .revoke(other_company, &ana, old.key.id())
            .await
            .unwrap_err(),
        ApplicationError::ApiKeyNotFound
    );
    assert_eq!(
        service
            .rotate(company, &ana, old.key.id(), 169, NOW)
            .await
            .unwrap_err(),
        DomainError::InvalidApiKeyGrace.into()
    );

    let new = service
        .rotate(company, &ana, old.key.id(), 2, NOW)
        .await
        .unwrap();
    assert_ne!(new.secret, old.secret);
    assert_eq!(new.key.name(), "Loja");
    assert_eq!(new.key.scope_keys(), old.key.scope_keys());
    assert_eq!(new.key.rotated_from(), Some(old.key.id()));
    assert_eq!(new.key.expires_at(), None);
    // Both work during the grace; only the new one afterwards.
    assert!(
        service
            .authenticate(&old.secret, NOW + 2 * HOUR - 1, day())
            .await
            .is_ok()
    );
    assert!(
        service
            .authenticate(&new.secret, NOW + 2 * HOUR, day())
            .await
            .is_ok()
    );
    assert_eq!(
        service
            .authenticate(&old.secret, NOW + 2 * HOUR, day())
            .await
            .unwrap_err(),
        refused()
    );
    // An expired key is not rotated again.
    assert_eq!(
        service
            .rotate(company, &ana, old.key.id(), 1, NOW + 3 * HOUR)
            .await
            .unwrap_err(),
        DomainError::ApiKeyIsNotActive.into()
    );
    let before = service.history(company, old.key.id()).await.unwrap();
    assert_eq!(before.len(), 2);
    assert_eq!(before[1].entry.action, AuditAction::Updated);
    assert!(
        before[1]
            .entry
            .changes
            .iter()
            .any(|change| change.field == "expires_at")
    );
    assert_eq!(
        service.history(company, new.key.id()).await.unwrap().len(),
        1
    );

    // A revocation is at once, is in the trail, and asking again changes and records nothing.
    let revoked = service.revoke(company, &ana, new.key.id()).await.unwrap();
    assert!(revoked.is_revoked());
    assert_eq!(
        service
            .authenticate(&new.secret, NOW, day())
            .await
            .unwrap_err(),
        refused()
    );
    service.revoke(company, &ana, new.key.id()).await.unwrap();
    let trail = service.history(company, new.key.id()).await.unwrap();
    assert_eq!(trail.len(), 2);
    assert!(
        trail[1]
            .entry
            .changes
            .iter()
            .any(|change| change.field == "revoked")
    );
    // Both stay listed.
    assert_eq!(service.list(company).await.unwrap().len(), 2);
    assert!(
        service
            .rotate(company, &ana, new.key.id(), 1, NOW)
            .await
            .is_err()
    );
}

/// A reference is claimed once per company, by whoever asks first, and every later asker is told who holds it, also when
/// they ask at the same moment.
pub async fn claims_a_sale_reference_once_per_company<C: SaleClaimRepository>(
    claims: &C,
    company: CompanyId,
    other_company: CompanyId,
) {
    let (first, second) = (SaleId::generate(), SaleId::generate());
    let held = claims
        .claim(company, "pedido-1", first, "request one")
        .await
        .unwrap();
    assert_eq!(
        (held.sale_id, held.request.as_str()),
        (first, "request one")
    );
    // Asking again, with another id and another request, gives the claim that holds.
    assert_eq!(
        claims
            .claim(company, "pedido-1", second, "request two")
            .await
            .unwrap(),
        held
    );
    assert_eq!(
        claims.find(company, "pedido-1").await.unwrap(),
        Some(held.clone())
    );
    assert_eq!(claims.find(company, "pedido-2").await.unwrap(), None);
    assert_eq!(
        claims
            .reference_of(company, first)
            .await
            .unwrap()
            .as_deref(),
        Some("pedido-1")
    );
    assert_eq!(
        claims.reference_of(other_company, first).await.unwrap(),
        None
    );
    // Another company has its own.
    assert_eq!(claims.find(other_company, "pedido-1").await.unwrap(), None);
    let theirs = claims
        .claim(other_company, "pedido-1", second, "request two")
        .await
        .unwrap();
    assert_eq!(theirs.sale_id, second);
    // At the same moment, one wins and all are told so.
    let ids = [SaleId::generate(), SaleId::generate(), SaleId::generate()];
    let (a, b, c) = tokio::join!(
        claims.claim(company, "pedido-3", ids[0], "r"),
        claims.claim(company, "pedido-3", ids[1], "r"),
        claims.claim(company, "pedido-3", ids[2], "r"),
    );
    let (a, b, c) = (a.unwrap(), b.unwrap(), c.unwrap());
    assert!(a == b && b == c && ids.contains(&a.sale_id));
}

/// An order named by a reference makes one sale: asking again gives it, also after a crash between the claim and the sale,
/// and another request under the same reference is refused.
#[allow(clippy::too_many_arguments)]
pub async fn registers_an_order_once_by_its_reference<S, Q, E, F, R, T, C>(
    sales: &SaleService<S, Q, E, F, R, T>,
    claims: &C,
    company: CompanyId,
    other_company: CompanyId,
    entry_seed: &EntrySeed,
    seed: &SalesSeed,
) where
    S: SaleRepository,
    Q: QuoteRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
    C: SaleClaimRepository,
{
    // The order is made by a key, and the trail must read back as one (kind and all) on every adapter.
    let ana = Actor::ApiKey {
        id: ApiKeyId::generate(),
        name: "Loja online".into(),
    };
    let (sale, made) = sales
        .register_external(
            claims,
            company,
            &ana,
            " loja-1042 ",
            "request A",
            sale_draft(entry_seed, seed),
        )
        .await
        .unwrap();
    assert!(made);
    assert_eq!(sale.status(), SaleStatus::Open);
    // The same request again is the same sale, written once.
    let (again, made) = sales
        .register_external(
            claims,
            company,
            &ana,
            "loja-1042",
            "request A",
            sale_draft(entry_seed, seed),
        )
        .await
        .unwrap();
    assert!(!made);
    assert_eq!(again, sale);
    let trail = sales.history(company, sale.id()).await.unwrap();
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].entry.actor, ana);
    // Another request under it is refused, and nothing changes.
    assert_eq!(
        sales
            .register_external(
                claims,
                company,
                &ana,
                "loja-1042",
                "request B",
                sale_draft(entry_seed, seed)
            )
            .await
            .unwrap_err(),
        ApplicationError::IdempotencyMismatch
    );

    // A crash after the claim and before the sale: the retry makes the sale the claim names.
    let promised = SaleId::generate();
    claims
        .claim(company, "loja-1043", promised, "request C")
        .await
        .unwrap();
    let (finished, made) = sales
        .register_external(
            claims,
            company,
            &ana,
            "loja-1043",
            "request C",
            sale_draft(entry_seed, seed),
        )
        .await
        .unwrap();
    assert!(made);
    assert_eq!(finished.id(), promised);

    // A reference that is not one is refused before anything is claimed.
    for bad in ["", "   ", "uma\nlinha", &"x".repeat(101)] {
        assert_eq!(
            sales
                .register_external(
                    claims,
                    company,
                    &ana,
                    bad,
                    "r",
                    sale_draft(entry_seed, seed)
                )
                .await
                .unwrap_err(),
            DomainError::InvalidExternalReference.into(),
            "{bad:?}"
        );
    }
    // The reference belongs to the company: another company's sale cannot be reached through it.
    assert!(
        sales
            .register_external(
                claims,
                other_company,
                &ana,
                "loja-1042",
                "request A",
                sale_draft(entry_seed, seed)
            )
            .await
            .is_err_and(|error| error != ApplicationError::IdempotencyMismatch)
    );
}

/// Held by every test that works the job queue of a real database: the queue is one for the whole database, so two tests that
/// claim jobs at once would take each other's.
pub static QUEUE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const WEBHOOK_URL: &str = "https://hooks.example.com/conta-verde";

fn subscription_draft(url: &str, events: &[&str], active: bool) -> WebhookSubscriptionDraft {
    WebhookSubscriptionDraft {
        url: url.into(),
        events: events.iter().map(|event| (*event).to_owned()).collect(),
        active,
    }
}

/// Subscriptions are the company's, audited without their secret, bounded, and a secret is generated or typed, sealed, and
/// signs a delivery from the moment it exists; a delivery is made once for an event however many times it is published, sent
/// signed, tried again with the waits of the kind, given up after the last try, sent again by hand, and kept for 30 days.
#[allow(clippy::too_many_lines)]
pub async fn runs_webhooks<R, C, Q, S>(
    service: &WebhookService<R, C, Q, S>,
    company: CompanyId,
    other_company: CompanyId,
) where
    R: WebhookRepository,
    C: SecretCipher,
    Q: JobQueue,
    S: SecretService,
{
    let ana = actor();
    let created = service
        .create(
            company,
            &ana,
            subscription_draft(WEBHOOK_URL, &["sale.completed", "sale.completed"], true),
            NOW,
        )
        .await
        .unwrap();
    let subscription = created.subscription.clone();
    assert!(created.secret.starts_with("whsec_") && created.secret.len() > 40);
    assert_eq!(subscription.events_text(), "sale.completed");
    assert_eq!(
        service.list(company).await.unwrap(),
        vec![subscription.clone()]
    );
    assert!(service.list(other_company).await.unwrap().is_empty());
    // The trail says what was asked and that a secret was set, never the secret.
    let trail = service.history(company, subscription.id()).await.unwrap();
    assert_eq!(trail.len(), 1);
    let written = format!("{:?}", trail[0].entry.changes);
    assert!(
        written.contains("secret")
            && written.contains("set")
            && !written.contains(&created.secret[6..])
    );
    assert_eq!(
        service
            .history(other_company, subscription.id())
            .await
            .unwrap_err(),
        ApplicationError::WebhookNotFound
    );

    // An address that is not https with a public host is refused, and an event that does not exist.
    for (url, events) in [
        ("http://hooks.example.com/x", vec![]),
        ("https://127.0.0.1/x", vec![]),
        ("https://169.254.169.254/latest", vec![]),
        ("https://user:pw@hooks.example.com/x", vec![]),
        (WEBHOOK_URL, vec!["sale.sold"]),
        (WEBHOOK_URL, vec!["webhook.test"]),
    ] {
        assert!(
            service
                .create(company, &ana, subscription_draft(url, &events, true), NOW)
                .await
                .is_err(),
            "{url} {events:?}"
        );
    }
    // A change that changes nothing writes nothing; one that does is in the trail.
    service
        .update(
            company,
            &ana,
            subscription.id(),
            subscription_draft(WEBHOOK_URL, &["sale.completed"], true),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .history(company, subscription.id())
            .await
            .unwrap()
            .len(),
        1
    );

    // An event is made once per subscription that wants it, however many times it is published.
    let event = Uuid::new_v4();
    let body = envelope(
        event,
        WebhookEventKind::SaleCompleted,
        company,
        NOW,
        r#"{"id":"s1"}"#,
    );
    assert_eq!(
        service
            .publish(company, event, WebhookEventKind::SaleCompleted, &body, NOW)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        service
            .publish(company, event, WebhookEventKind::SaleCompleted, &body, NOW)
            .await
            .unwrap(),
        0
    );
    // What it did not ask for, and another company's events, make none.
    let unwanted = Uuid::new_v4();
    assert_eq!(
        service
            .publish(company, unwanted, WebhookEventKind::SaleCreated, &body, NOW)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        service
            .publish(
                other_company,
                event,
                WebhookEventKind::SaleCompleted,
                &body,
                NOW
            )
            .await
            .unwrap(),
        0
    );

    // It is sent signed, with the secret that was shown, and the log says how it went.
    let _lock = QUEUE.lock().await;
    let sender = InMemoryWebhookSender::default();
    let report = service.run_due(NOW / 1000, 50, &sender).await.unwrap();
    assert_eq!(report.done, 1);
    let sent = sender.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].url, WEBHOOK_URL);
    assert_eq!(
        (sent[0].event_id, sent[0].timestamp, sent[0].body.as_str()),
        (event, NOW / 1000, body.as_str())
    );
    assert_eq!(
        sent[0].signature,
        webhook_signature(&created.secret, &event.to_string(), NOW / 1000, &body)
    );
    let log = service
        .deliveries(company, subscription.id(), None, None)
        .await
        .unwrap();
    assert_eq!(log.items.len(), 1);
    let delivery = log.items[0].clone();
    assert_eq!(
        (delivery.status, delivery.attempts, delivery.last_status),
        (DeliveryStatus::Delivered, 1, Some(200))
    );
    assert_eq!(delivery.delivered_at, Some(NOW));
    assert!(
        service
            .deliveries(other_company, subscription.id(), None, None)
            .await
            .is_err()
    );

    // A subscriber that is down: the waits double from a minute, and the answer is only its status and a short reason.
    let failing = InMemoryWebhookSender::answering(vec![
        SendOutcome::answered(500),
        SendOutcome::failed("timed out"),
    ]);
    let flaky = Uuid::new_v4();
    service
        .publish(company, flaky, WebhookEventKind::SaleCompleted, &body, NOW)
        .await
        .unwrap();
    assert_eq!(
        service
            .run_due(NOW / 1000, 50, &failing)
            .await
            .unwrap()
            .retried,
        1
    );
    assert_eq!(
        service
            .run_due(NOW / 1000 + 59, 50, &failing)
            .await
            .unwrap()
            .retried,
        0
    );
    assert_eq!(
        service
            .run_due(NOW / 1000 + 60, 50, &failing)
            .await
            .unwrap()
            .retried,
        1
    );
    let log = service
        .deliveries(company, subscription.id(), None, None)
        .await
        .unwrap();
    let pending = log.items.iter().find(|d| d.event_id == flaky).unwrap();
    assert_eq!(
        (pending.status, pending.attempts, pending.last_status),
        (DeliveryStatus::Pending, 2, None)
    );
    assert_eq!(pending.last_error.as_deref(), Some("timed out"));
    // The third try (a wait of two minutes) is answered with success.
    assert_eq!(
        service
            .run_due(NOW / 1000 + 60 + 119, 50, &failing)
            .await
            .unwrap()
            .done,
        0
    );
    assert_eq!(
        service
            .run_due(NOW / 1000 + 60 + 120, 50, &failing)
            .await
            .unwrap()
            .done,
        1
    );
    let log = service
        .deliveries(company, subscription.id(), None, None)
        .await
        .unwrap();
    let healed = log.items.iter().find(|d| d.event_id == flaky).unwrap();
    assert_eq!(
        (healed.status, healed.attempts),
        (DeliveryStatus::Delivered, 3)
    );

    // After the last try (the fifteenth, for a webhook) the delivery is given up on, and a person may send it again.
    let dead = Uuid::new_v4();
    service
        .publish(company, dead, WebhookEventKind::SaleCompleted, &body, NOW)
        .await
        .unwrap();
    let hopeless = InMemoryWebhookSender::answering(vec![SendOutcome::answered(503); 20]);
    let mut at = NOW / 1000;
    for _ in 0..15 {
        service.run_due(at, 50, &hopeless).await.unwrap();
        at += 3_600;
    }
    assert_eq!(hopeless.sent().len(), 15);
    let log = service
        .deliveries(company, subscription.id(), None, None)
        .await
        .unwrap();
    let failed = log
        .items
        .iter()
        .find(|d| d.event_id == dead)
        .unwrap()
        .clone();
    assert_eq!(
        (failed.status, failed.attempts, failed.last_status),
        (DeliveryStatus::Failed, 15, Some(503))
    );
    assert_eq!(
        service
            .run_due(at + 100_000, 50, &hopeless)
            .await
            .unwrap()
            .done,
        0
    );
    assert_eq!(
        service
            .redeliver(other_company, subscription.id(), failed.id, NOW)
            .await
            .unwrap_err(),
        ApplicationError::WebhookDeliveryNotFound
    );
    let again = service
        .redeliver(company, subscription.id(), failed.id, at * 1000)
        .await
        .unwrap();
    assert_eq!((again.status, again.attempts), (DeliveryStatus::Pending, 0));
    let healthy = InMemoryWebhookSender::default();
    assert_eq!(service.run_due(at, 50, &healthy).await.unwrap().done, 1);
    assert_eq!(healthy.sent()[0].event_id, dead);

    // A delivery that is pending has a job of its own: it cannot be sent again by hand, and one that was stored without a job (a
    // failure between making it and queuing it) is queued again once it is old enough, and only once.
    let waiting = Uuid::new_v4();
    service
        .publish(
            company,
            waiting,
            WebhookEventKind::SaleCompleted,
            &body,
            at * 1000,
        )
        .await
        .unwrap();
    let log = service
        .deliveries(company, subscription.id(), None, None)
        .await
        .unwrap();
    let pending_id = log.items.iter().find(|d| d.event_id == waiting).unwrap().id;
    assert_eq!(
        service
            .redeliver(company, subscription.id(), pending_id, at * 1000)
            .await
            .unwrap_err(),
        ApplicationError::WebhookDeliveryNotFound
    );
    // The job it has is the queue's to run; one the queue lost is made again by the pass that looks for old pending ones.
    assert_eq!(service.requeue_stale(at * 1000 + 60_000).await.unwrap(), 0);
    assert!(
        service
            .requeue_stale(at * 1000 + 3 * 3_600_000)
            .await
            .unwrap()
            >= 1
    );
    assert_eq!(
        service
            .requeue_stale(at * 1000 + 3 * 3_600_000)
            .await
            .unwrap(),
        0
    );
    let drained = InMemoryWebhookSender::default();
    service.run_due(at + 3 * 3_600, 50, &drained).await.unwrap();
    assert!(drained.sent().iter().any(|sent| sent.event_id == waiting));

    // The secret may be typed (and is never given back) or made again: the one it replaces signs for a day more.
    let typed = "my-own-secret-123";
    assert_eq!(
        service
            .replace_secret(company, &ana, subscription.id(), Some(typed), NOW)
            .await
            .unwrap(),
        None
    );
    assert!(
        service
            .replace_secret(company, &ana, subscription.id(), Some("short"), NOW)
            .await
            .is_err()
    );
    assert!(
        service
            .replace_secret(
                company,
                &ana,
                subscription.id(),
                Some("has a space inside"),
                NOW
            )
            .await
            .is_err()
    );
    let next = Uuid::new_v4();
    service
        .publish(company, next, WebhookEventKind::SaleCompleted, &body, NOW)
        .await
        .unwrap();
    let both = InMemoryWebhookSender::default();
    service.run_due(NOW / 1000, 50, &both).await.unwrap();
    let expected = [
        webhook_signature(typed, &next.to_string(), NOW / 1000, &body),
        webhook_signature(&created.secret, &next.to_string(), NOW / 1000, &body),
    ];
    assert_eq!(both.sent()[0].signature, expected.join(" "));
    let after_a_day = NOW + SECRET_GRACE_MILLIS;
    let later = Uuid::new_v4();
    service
        .publish(
            company,
            later,
            WebhookEventKind::SaleCompleted,
            &body,
            after_a_day,
        )
        .await
        .unwrap();
    let only_new = InMemoryWebhookSender::default();
    service
        .run_due(after_a_day / 1000, 50, &only_new)
        .await
        .unwrap();
    assert_eq!(
        only_new.sent()[0].signature,
        webhook_signature(typed, &later.to_string(), after_a_day / 1000, &body)
    );
    let generated = service
        .replace_secret(company, &ana, subscription.id(), None, NOW)
        .await
        .unwrap()
        .unwrap();
    assert!(generated.starts_with("whsec_") && generated != typed);
    let trail = service.history(company, subscription.id()).await.unwrap();
    let written = format!(
        "{:?}",
        trail.iter().map(|r| &r.entry.changes).collect::<Vec<_>>()
    );
    assert!(
        written.contains("changed")
            && !written.contains(typed)
            && !written.contains(&generated[6..])
    );

    // A test goes to a subscription that is off too; an off one is told nothing else.
    service
        .update(
            company,
            &ana,
            subscription.id(),
            subscription_draft(WEBHOOK_URL, &["sale.completed"], false),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .publish(
                company,
                Uuid::new_v4(),
                WebhookEventKind::SaleCompleted,
                &body,
                NOW
            )
            .await
            .unwrap(),
        0
    );
    let test = service
        .send_test(company, subscription.id(), NOW)
        .await
        .unwrap();
    assert_eq!(test.kind, WebhookEventKind::Test);
    let test_sender = InMemoryWebhookSender::default();
    service.run_due(NOW / 1000, 50, &test_sender).await.unwrap();
    assert!(
        test_sender.sent()[0]
            .body
            .contains(r#""type":"webhook.test""#)
    );
    assert_eq!(
        service
            .send_test(other_company, subscription.id(), NOW)
            .await
            .unwrap_err(),
        ApplicationError::WebhookNotFound
    );

    // The log is paged newest first and is kept for 30 days.
    let all = service
        .deliveries(company, subscription.id(), None, Some(1000))
        .await
        .unwrap();
    assert!(all.items.len() >= 5);
    let page = service
        .deliveries(company, subscription.id(), None, Some(2))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    let rest = service
        .deliveries(company, subscription.id(), page.next.as_ref(), Some(1000))
        .await
        .unwrap();
    assert_eq!(page.items.len() + rest.items.len(), all.items.len());
    assert!(
        all.items
            .windows(2)
            .all(|pair| pair[0].created_at >= pair[1].created_at)
    );
    assert!(
        service
            .prune(NOW + 31 * 24 * 3_600_000 + at * 1000)
            .await
            .unwrap()
            >= 5
    );
    assert!(
        service
            .deliveries(company, subscription.id(), None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );

    // A company keeps a bounded number of subscriptions.
    for number in 1..MAX_WEBHOOK_SUBSCRIPTIONS {
        service
            .create(
                other_company,
                &ana,
                subscription_draft(&format!("{WEBHOOK_URL}/{number}"), &[], true),
                NOW,
            )
            .await
            .unwrap();
    }
    service
        .create(
            other_company,
            &ana,
            subscription_draft(WEBHOOK_URL, &[], true),
            NOW,
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .create(
                other_company,
                &ana,
                subscription_draft(WEBHOOK_URL, &[], true),
                NOW
            )
            .await
            .unwrap_err(),
        DomainError::TooManyWebhookSubscriptions.into()
    );
}

/// A worker that does one kind of job never takes another's, whatever is due.
pub async fn claims_jobs_by_kind<Q: JobQueue>(queue: &Q) {
    use conta_verde_domain::{JobKind, NewJob};
    let _lock = QUEUE.lock().await;
    let far = i64::MAX / 2;
    // The queue of a real database is shared: what other runs left is taken out first.
    while queue.claim_next(far, 1).await.unwrap().is_some() {}
    let mail = NewJob::new(
        JobKind::SendEmail,
        vec!["a@x.com".into(), "s".into(), "b".into()],
        0,
    )
    .unwrap();
    let hook = NewJob::new(JobKind::DeliverWebhook, vec![Uuid::new_v4().to_string()], 0).unwrap();
    let mail_id = queue.enqueue(&mail).await.unwrap();
    let hook_id = queue.enqueue(&hook).await.unwrap();
    let taken = queue
        .claim_next_of(10, 60, &[JobKind::DeliverWebhook])
        .await
        .unwrap()
        .unwrap();
    assert_eq!((taken.id, taken.kind), (hook_id, JobKind::DeliverWebhook));
    assert!(
        queue
            .claim_next_of(10, 60, &[JobKind::DeliverWebhook])
            .await
            .unwrap()
            .is_none()
    );
    let taken = queue
        .claim_next_of(10, 60, &[JobKind::SendEmail])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(taken.id, mail_id);
    assert!(
        queue
            .claim_next_of(10, 60, &JobKind::ALL)
            .await
            .unwrap()
            .is_none()
    );
    queue.complete(hook_id).await.unwrap();
    queue.complete(mail_id).await.unwrap();
}

/// What the lifecycle of sales raised, after the writes that made it: each sale is told as created, and the ones that were
/// completed and cancelled as that, never twice for one step; a write that was refused raised nothing.
pub fn sales_tell_what_happens_to_them(events: &[conta_verde_application::DomainEvent]) {
    use conta_verde_application::DomainEvent;
    let kinds = |wanted: WebhookEventKind| {
        events
            .iter()
            .filter(|event| matches!(event, DomainEvent::Sale { kind, .. } if *kind == wanted))
            .count()
    };
    assert!(kinds(WebhookEventKind::SaleCreated) >= 2);
    assert!(kinds(WebhookEventKind::SaleCompleted) >= 1);
    assert!(kinds(WebhookEventKind::SaleCancelled) >= 1);
    // A sale is created once, however many things happen to it afterwards.
    let mut created: Vec<SaleId> = events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::Sale {
                kind: WebhookEventKind::SaleCreated,
                sale,
            } => Some(sale.id()),
            _ => None,
        })
        .collect();
    let all = created.len();
    created.sort_by_key(|id| id.as_uuid());
    created.dedup();
    assert_eq!(created.len(), all);
    // What was told of a completed sale says it is completed.
    assert!(events.iter().all(|event| match event {
        DomainEvent::Sale {
            kind: WebhookEventKind::SaleCompleted,
            sale,
        } => sale.status() == SaleStatus::Completed,
        DomainEvent::Sale {
            kind: WebhookEventKind::SaleCancelled,
            sale,
        } => sale.status() == SaleStatus::Cancelled,
        DomainEvent::Sale {
            kind: WebhookEventKind::SaleCreated,
            sale,
        } => sale.number() >= 1,
        _ => true,
    }));
}

/// A pull that was half way when its connection was ended writes its status late: the connection stays ended, and the bank account
/// stays free for another one.
pub async fn an_ended_connection_stays_ended<C: BankConnectionRepository>(
    connections: &C,
    company: CompanyId,
    bank_account: BankAccountId,
) {
    let ana = actor();
    let entry =
        |connection: &conta_verde_domain::BankConnection, action| conta_verde_domain::AuditEntry {
            company_id: company,
            actor: ana.clone(),
            entity: conta_verde_domain::AuditEntity::BankConnection,
            entity_id: connection.id().as_uuid(),
            action,
            changes: Vec::new(),
        };
    let connection = conta_verde_domain::BankConnection::new(
        conta_verde_domain::BankConnectionId::generate(),
        company,
        bank_account,
        "fake",
        "item-ended",
        "acc-ended",
        ana.clone(),
        NOW,
    )
    .unwrap();
    connections
        .save(&connection, &entry(&connection, AuditAction::Created))
        .await
        .unwrap();
    let ended = connection.with_status(ConnectionStatus::Disconnected, None);
    connections
        .update_status(&ended, &entry(&ended, AuditAction::Updated))
        .await
        .unwrap();
    // The pull that read it while it was live finds the bank asking for the person, and writes that.
    let late = connection.with_status(
        ConnectionStatus::NeedsAttention,
        Some("the bank login failed"),
    );
    assert_eq!(
        connections
            .update_status(&late, &entry(&late, AuditAction::Updated))
            .await
            .unwrap_err(),
        RepositoryError::NotFound
    );
    let stored = connections
        .find(company, connection.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status(), ConnectionStatus::Disconnected);
    // Ending it again is the same, and the account takes another connection.
    connections
        .update_status(&ended, &entry(&ended, AuditAction::Updated))
        .await
        .unwrap();
}

/// A bank connection links a bank account of the company to an account at the aggregator and pulls its statement as statement
/// lines, once however many times the same movement is pulled, in the name of whoever connected it; the bank needing the person
/// stops the pull and says so; an aggregator that is down is tried again by the queue; and a connection that is ended is not pulled.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn syncs_bank_connections<C, S, E, T, F, Q>(
    sync: &BankSyncService<C, FakeBankAggregator, ReconciliationService<S, E, T, F>, Q>,
    aggregator: &FakeBankAggregator,
    lines: &ReconciliationService<S, E, T, F>,
    company: CompanyId,
    other_company: CompanyId,
    main: BankAccountId,
    reserve: BankAccountId,
) where
    C: BankConnectionRepository,
    S: StatementLineRepository,
    E: FinancialEntryRepository,
    T: TransferRepository,
    F: EntryReferences,
    Q: JobQueue,
{
    let ana = actor();
    let today = CalendarDate::parse("2026-11-10").unwrap();
    let now = 1_794_000_000_000_i64;
    let stored = || async {
        lines
            .list(company, &StatementLineQuery::default(), None, Some(100))
            .await
            .unwrap()
            .items
    };
    aggregator.add_transaction("acc-1", "t1", "2026-11-05", "1250.50", "PIX RECEBIDO Maria");
    aggregator.add_transaction("acc-1", "t2", "2026-11-06", "-30", "TARIFA");
    aggregator.add_transaction("acc-1", "t3", "2026-11-07", "0", "ZERADO");
    aggregator.add_transaction("acc-1", "t4", "2026-11-08", "-5.5", "CAFE");
    // Before the first pull's window: never asked for.
    aggregator.add_transaction("acc-1", "old", "2026-01-01", "-1", "ANTIGO");

    // What is connected must exist: a bank account of the company, an account the connection holds, and an item that is the
    // aggregator's.
    assert_eq!(
        sync.connect(
            company,
            &ana,
            BankAccountId::generate(),
            "item-1",
            "acc-1",
            now
        )
        .await
        .unwrap_err(),
        ApplicationError::UnusableReference("account")
    );
    assert_eq!(
        sync.connect(company, &ana, main, "item-1", "acc-x", now)
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("aggregator account")
    );
    assert_eq!(
        sync.connect(company, &ana, main, "no-item", "acc-1", now)
            .await
            .unwrap_err(),
        ApplicationError::AggregatorUnavailable
    );
    assert_eq!(
        sync.connect(other_company, &ana, main, "item-1", "acc-1", now)
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("account")
    );
    assert!(sync.list(company).await.unwrap().is_empty());

    let connection = sync
        .connect(company, &ana, main, "item-1", "acc-1", now)
        .await
        .unwrap();
    assert_eq!(
        (
            connection.status(),
            connection.provider(),
            connection.last_synced_on()
        ),
        (ConnectionStatus::Connected, "fake", None)
    );
    let trail = sync.history(company, connection.id()).await.unwrap();
    assert_eq!(
        (trail.len(), trail[0].entry.actor.clone()),
        (1, ana.clone())
    );
    assert_eq!(
        sync.connect(company, &ana, main, "item-1", "acc-1", now)
            .await
            .unwrap_err(),
        ApplicationError::Repository(RepositoryError::DuplicateBankConnection)
    );
    assert!(matches!(
        sync.history(other_company, connection.id())
            .await
            .unwrap_err(),
        ApplicationError::BankConnectionNotFound
    ));
    assert_eq!(
        sync.sync(other_company, connection.id(), today)
            .await
            .unwrap_err(),
        ApplicationError::BankConnectionNotFound
    );

    // The first pull goes three months back, stores what can be a line and counts what cannot.
    let first = sync.sync(company, connection.id(), today).await.unwrap();
    assert_eq!(
        (
            first.created,
            first.skipped,
            first.refused,
            first.needs_attention
        ),
        (3, 0, 1, false)
    );
    let held = stored().await;
    assert_eq!(held.len(), 3);
    let mut refs: Vec<&str> = held.iter().filter_map(|line| line.bank_ref()).collect();
    refs.sort_unstable();
    assert_eq!(refs, ["t1", "t2", "t4"]);
    assert_eq!(
        aggregator.pulls().last().unwrap(),
        &(
            "acc-1".to_owned(),
            CalendarDate::parse("2026-08-12").unwrap(),
            today
        )
    );
    let after = sync.list(company).await.unwrap();
    assert_eq!(after[0].last_synced_on(), Some(today));
    // The lines are the person's in the trail of reconciliation, made in the name of whoever connected it.
    let line_trail = lines.history(company, held[0].id()).await.unwrap();
    assert_eq!(line_trail[0].entry.actor, ana);

    // Pulling again, a day later, overlaps the last pull: what was pulled adds nothing, what is new is added once.
    aggregator.add_transaction("acc-1", "t5", "2026-11-10", "-12", "ONIBUS");
    let tomorrow = CalendarDate::parse("2026-11-11").unwrap();
    let second = sync.sync(company, connection.id(), tomorrow).await.unwrap();
    assert_eq!((second.created, second.skipped), (1, 3));
    assert_eq!(
        aggregator.pulls().last().unwrap().1,
        CalendarDate::parse("2026-11-03").unwrap()
    );
    let third = sync.sync(company, connection.id(), tomorrow).await.unwrap();
    assert_eq!((third.created, third.skipped), (0, 4));
    assert_eq!(stored().await.len(), 4);

    // The bank needing the person stops the pull, says why, and the connection waits.
    aggregator.set_state(
        "item-1",
        ItemState::NeedsAttention("the bank login failed".into()),
    );
    let needs = sync.sync(company, connection.id(), tomorrow).await.unwrap();
    assert!(needs.needs_attention);
    let waiting = sync.list(company).await.unwrap().remove(0);
    assert_eq!(
        (waiting.status(), waiting.last_error()),
        (
            ConnectionStatus::NeedsAttention,
            Some("the bank login failed")
        )
    );
    assert_eq!(waiting.last_synced_on(), Some(tomorrow));
    let trail = sync.history(company, connection.id()).await.unwrap();
    assert!(
        trail
            .last()
            .unwrap()
            .entry
            .changes
            .iter()
            .any(|change| change.field == "status")
    );
    // Asking while the bank still waits changes nothing and writes nothing more.
    let still = sync.sync(company, connection.id(), tomorrow).await.unwrap();
    assert!(still.needs_attention);
    assert_eq!(
        sync.history(company, connection.id()).await.unwrap().len(),
        2
    );
    // Once the person has done what the bank asked, asking again brings the connection back and pulls.
    aggregator.set_state("item-1", ItemState::Ready);
    let back = sync.sync(company, connection.id(), tomorrow).await.unwrap();
    assert!(!back.needs_attention);
    let revived = sync.list(company).await.unwrap().remove(0);
    assert_eq!(
        (revived.status(), revived.last_error()),
        (ConnectionStatus::Connected, None)
    );
    assert_eq!(
        sync.history(company, connection.id()).await.unwrap().len(),
        3
    );
    assert_eq!(stored().await.len(), 4);

    // Ending it frees the bank account for another connection, tells the aggregator, and is the same however often it is asked.
    let ended = sync
        .disconnect(company, &ana, connection.id())
        .await
        .unwrap();
    assert_eq!(ended.status(), ConnectionStatus::Disconnected);
    assert_eq!(aggregator.disconnected(), ["item-1"]);
    sync.disconnect(company, &ana, connection.id())
        .await
        .unwrap();
    assert_eq!(
        sync.history(company, connection.id()).await.unwrap().len(),
        4
    );
    assert_eq!(
        sync.disconnect(other_company, &ana, connection.id())
            .await
            .unwrap_err(),
        ApplicationError::BankConnectionNotFound
    );

    // A pull is done by the queue: every connection that has not pulled through the day is queued once, an aggregator that is down
    // makes the job wait and says why on the connection, and when it is back the job pulls and nothing is queued again.
    let _lock = QUEUE.lock().await;
    aggregator.set_state("item-1", ItemState::Ready);
    let again = sync
        .connect(company, &ana, main, "item-1", "acc-1", now)
        .await
        .unwrap();
    // An account at the aggregator feeds one bank account: another bank account cannot take the same one while it is live.
    assert_eq!(
        sync.connect(company, &ana, reserve, "item-1", "acc-1", now)
            .await
            .unwrap_err(),
        ApplicationError::Repository(RepositoryError::DuplicateBankConnection)
    );
    aggregator.add_account("item-1", "acc-2", "Poupanca");
    let other = sync
        .connect(company, &ana, reserve, "item-1", "acc-2", now)
        .await
        .unwrap();
    // Both were queued when they were made; the pass that finds what is due queues what has not pulled through the day.
    assert!(sync.enqueue_due(today, now / 1000).await.unwrap() >= 2);
    aggregator.set_down(true);
    let report = sync.run_due(now / 1000, today, 50).await.unwrap();
    assert!(report.retried >= 2, "{report:?}");
    let noted = sync.list(company).await.unwrap();
    assert!(
        noted
            .iter()
            .any(|c| c.id() == again.id() && c.last_error().is_some())
    );
    assert!(
        noted
            .iter()
            .filter(|c| c.id() == again.id() || c.id() == other.id())
            .all(|c| c.status() == ConnectionStatus::Connected)
    );
    aggregator.set_down(false);
    let report = sync
        .run_due(now / 1000 + 3 * 3600, today, 50)
        .await
        .unwrap();
    assert!(report.done >= 2, "{report:?}");
    let done = sync.list(company).await.unwrap();
    for id in [again.id(), other.id()] {
        let connection = done.iter().find(|c| c.id() == id).unwrap();
        assert_eq!(
            (connection.last_synced_on(), connection.last_error()),
            (Some(today), None),
            "{id:?}"
        );
    }
    assert_eq!(sync.enqueue_due(today, now / 1000).await.unwrap(), 0);
    // An aggregator that no longer accepts the connection waits for the person like a bank that asks for them (no retry changes
    // it), and comes back when asked again once it accepts it.
    aggregator.set_refused(true);
    let refused = sync.sync(company, other.id(), today).await.unwrap();
    assert!(refused.needs_attention);
    let waiting = sync
        .list(company)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id() == other.id())
        .unwrap();
    assert_eq!(waiting.status(), ConnectionStatus::NeedsAttention);
    assert!(waiting.last_error().is_some());
    aggregator.set_refused(false);
    assert!(
        !sync
            .sync(company, other.id(), today)
            .await
            .unwrap()
            .needs_attention
    );
    // A connection that was ended while its job waited pulls nothing. The aggregator's item is ended only when no connection
    // still uses it: another account of the same item is connected.
    sync.disconnect(company, &ana, other.id()).await.unwrap();
    assert_eq!(aggregator.disconnected(), ["item-1"]);
    assert!(
        sync.enqueue_due(CalendarDate::parse("2026-11-12").unwrap(), now / 1000)
            .await
            .unwrap()
            >= 1
    );
    sync.run_due(
        now / 1000 + 6 * 3600,
        CalendarDate::parse("2026-11-12").unwrap(),
        50,
    )
    .await
    .unwrap();
    let ended_one = sync
        .list(company)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id() == other.id())
        .unwrap();
    assert_eq!(ended_one.last_synced_on(), Some(today));
    sync.disconnect(company, &ana, again.id()).await.unwrap();
    assert_eq!(aggregator.disconnected(), ["item-1", "item-1"]);
}

/// The connector service over the real key and subscription services, as the API holds them.
type ConnectorsOver<C, KR, S, WR, CI, Q> = ConnectorService<
    C,
    std::sync::Arc<ApiKeyService<KR, S>>,
    std::sync::Arc<WebhookService<WR, CI, Q, S>>,
>;

/// Connectors ([ADR 0090]): turned on they make a key with the kind's scopes and a subscription to its events, turned off they
/// revoke the key and switch the subscription off, and what a failure leaves is taken back. `connectors` is the service over the
/// real key and subscription services, and `keys` and `hooks` are those same services, to look at what was made.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn runs_connectors<C, KR, S, WR, CI, Q>(
    connectors: &ConnectorsOver<C, KR, S, WR, CI, Q>,
    repository: &C,
    keys: &ApiKeyService<KR, S>,
    hooks: &WebhookService<WR, CI, Q, S>,
    company: CompanyId,
    other_company: CompanyId,
) where
    C: ConnectorRepository,
    KR: ApiKeyRepository,
    S: SecretService,
    WR: WebhookRepository,
    CI: SecretCipher,
    Q: JobQueue,
{
    use conta_verde_domain::{Connector, ConnectorKind, MAX_CONNECTORS};

    let ana = actor();
    let kind = ConnectorKind::WooCommerce;
    let shop = connectors
        .create(company, &ana, kind, "Loja do centro", NOW)
        .await
        .unwrap();
    assert!(!shop.is_active());
    // A name is the company's once, ignoring case; another company may use it; and there is a limit.
    assert_eq!(
        connectors
            .create(company, &ana, kind, "LOJA DO CENTRO", NOW)
            .await
            .unwrap_err(),
        ApplicationError::Repository(RepositoryError::DuplicateName)
    );
    connectors
        .create(other_company, &ana, kind, "Loja do centro", NOW)
        .await
        .unwrap();
    for number in 1..MAX_CONNECTORS {
        connectors
            .create(company, &ana, kind, &format!("Loja {number}"), NOW)
            .await
            .unwrap();
    }
    assert_eq!(
        connectors
            .create(company, &ana, kind, "Uma a mais", NOW)
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::TooManyConnectors)
    );

    // Off, nothing was made for it.
    let keys_before = keys.list(company).await.unwrap().len();
    let subscriptions_before = hooks.list(company).await.unwrap().len();
    let views = connectors.list(company, NOW).await.unwrap();
    assert_eq!(views.len(), MAX_CONNECTORS);
    assert!(
        views
            .iter()
            .all(|view| !view.key_usable && !view.subscription_on)
    );

    // An address the subscription would refuse is refused before anything is made: no key, no subscription, nothing to take back.
    assert!(
        connectors
            .turn_on(company, &ana, shop.id(), "http://localhost/hook", NOW)
            .await
            .is_err()
    );
    assert_eq!(keys.list(company).await.unwrap().len(), keys_before);
    assert_eq!(
        hooks.list(company).await.unwrap().len(),
        subscriptions_before
    );
    assert!(
        !connectors
            .list(company, NOW)
            .await
            .unwrap()
            .iter()
            .any(|v| v.connector.is_active())
    );

    // Turned on it makes the key with the kind's scopes and the subscription to its events, and gives the two secrets once.
    let on = connectors
        .turn_on(company, &ana, shop.id(), WEBHOOK_URL, NOW)
        .await
        .unwrap();
    assert!(on.connector.is_active());
    assert!(on.api_key_secret.starts_with("cvk_") && on.webhook_secret.starts_with("whsec_"));
    let key = keys
        .list(company)
        .await
        .unwrap()
        .into_iter()
        .find(|key| Some(key.id()) == on.connector.api_key_id())
        .unwrap();
    assert_eq!(key.scope_keys(), ["products:read", "sales:write"]);
    assert_eq!(key.name(), "Conector WooCommerce: Loja do centro");
    let subscription = hooks
        .list(company)
        .await
        .unwrap()
        .into_iter()
        .find(|subscription| Some(subscription.id()) == on.connector.webhook_subscription_id())
        .unwrap();
    assert_eq!(subscription.events_text(), "sale.cancelled,sale.completed");
    assert!(subscription.is_active());
    let view = connectors
        .list(company, NOW)
        .await
        .unwrap()
        .into_iter()
        .find(|view| view.connector.id() == shop.id())
        .unwrap();
    assert!(view.key_usable && view.subscription_on);
    // The key speaks for the company, and it is the person's to revoke: the state is read, not remembered.
    keys.revoke(company, &ana, key.id()).await.unwrap();
    let revoked = connectors
        .list(company, NOW)
        .await
        .unwrap()
        .into_iter()
        .find(|view| view.connector.id() == shop.id())
        .unwrap();
    assert!(revoked.connector.is_active() && !revoked.key_usable && revoked.subscription_on);

    // Turned on twice is refused, and makes nothing; and another company's connector is nobody's.
    let keys_now = keys.list(company).await.unwrap().len();
    assert_eq!(
        connectors
            .turn_on(company, &ana, shop.id(), WEBHOOK_URL, NOW)
            .await
            .unwrap_err(),
        ApplicationError::Repository(RepositoryError::ConnectorChanged)
    );
    assert_eq!(keys.list(company).await.unwrap().len(), keys_now);
    for result in [
        connectors
            .turn_on(other_company, &ana, shop.id(), WEBHOOK_URL, NOW)
            .await
            .map(|_| ()),
        connectors
            .turn_off(other_company, &ana, shop.id())
            .await
            .map(|_| ()),
        connectors
            .history(other_company, shop.id())
            .await
            .map(|_| ()),
    ] {
        assert_eq!(result.unwrap_err(), ApplicationError::ConnectorNotFound);
    }

    // Two at once make one winner: a write that read it off finds it on.
    let loser: Connector = on.connector.turned_off();
    let entry = conta_verde_domain::AuditEntry {
        company_id: company,
        actor: ana.clone(),
        entity: conta_verde_domain::AuditEntity::Connector,
        entity_id: shop.id().as_uuid(),
        action: AuditAction::Updated,
        changes: Vec::new(),
    };
    assert_eq!(
        repository.switch(&loser, false, &entry).await.unwrap_err(),
        RepositoryError::ConnectorChanged
    );

    // Turned off it revokes the key and switches the subscription off, which it keeps; asking again changes and records nothing.
    let off = connectors.turn_off(company, &ana, shop.id()).await.unwrap();
    assert!(!off.is_active() && off.api_key_id().is_none());
    assert_eq!(
        off.webhook_subscription_id(),
        on.connector.webhook_subscription_id()
    );
    assert!(
        keys.list(company)
            .await
            .unwrap()
            .iter()
            .all(|key| key.is_revoked())
    );
    assert!(
        !hooks
            .list(company)
            .await
            .unwrap()
            .into_iter()
            .find(|subscription| Some(subscription.id()) == off.webhook_subscription_id())
            .unwrap()
            .is_active()
    );
    let trail = connectors.history(company, shop.id()).await.unwrap().len();
    connectors.turn_off(company, &ana, shop.id()).await.unwrap();
    assert_eq!(
        connectors.history(company, shop.id()).await.unwrap().len(),
        trail
    );

    // Two turning it on again at the same time: one wins and the other takes back its own key and **touches nothing else**, so the
    // winner's connector, key and subscription (which is shared) stand.
    let (first, second) = tokio::join!(
        connectors.turn_on(company, &ana, shop.id(), WEBHOOK_URL, NOW),
        connectors.turn_on(company, &ana, shop.id(), WEBHOOK_URL, NOW),
    );
    let (again, lost) = match (first, second) {
        (Ok(won), Err(lost)) | (Err(lost), Ok(won)) => (won, lost),
        other => panic!("exactly one wins: {other:?}"),
    };
    assert_eq!(
        lost,
        ApplicationError::Repository(RepositoryError::ConnectorChanged)
    );
    let live: Vec<_> = keys
        .list(company)
        .await
        .unwrap()
        .into_iter()
        .filter(|key| !key.is_revoked())
        .collect();
    assert_eq!(live.len(), 1);
    assert_eq!(Some(live[0].id()), again.connector.api_key_id());
    assert!(
        hooks
            .list(company)
            .await
            .unwrap()
            .into_iter()
            .find(
                |subscription| Some(subscription.id()) == again.connector.webhook_subscription_id()
            )
            .unwrap()
            .is_active()
    );
    let view = connectors
        .list(company, NOW)
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.connector.id() == shop.id())
        .unwrap();
    assert!(view.key_usable && view.subscription_on);
    // Turned on again it makes a new key, switches the same subscription on and gives a new secret.
    assert_ne!(again.connector.api_key_id(), on.connector.api_key_id());
    assert_eq!(
        again.connector.webhook_subscription_id(),
        on.connector.webhook_subscription_id()
    );
    assert_ne!(again.webhook_secret, on.webhook_secret);
    assert_eq!(
        hooks.list(company).await.unwrap().len(),
        subscriptions_before + 1
    );
    // The trail says created, on, off, on, with ids and never a secret.
    let trail = connectors.history(company, shop.id()).await.unwrap();
    assert_eq!(trail.len(), 4);
    let written = format!("{trail:?}");
    assert!(!written.contains(&on.api_key_secret) && !written.contains(&again.webhook_secret));
    assert!(written.contains("active"));
}
