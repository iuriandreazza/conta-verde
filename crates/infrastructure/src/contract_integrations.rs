//! Behavior every adapter of the public API's ports must share ([ADR 0087]), written once and run against each of them.

use conta_verde_application::{
    ApiKeyRepository, ApiKeyService, ApplicationError, AuthError, DocumentReferences,
    DocumentStock, EntryReferences, FinancialEntryRepository, QuoteRepository, SaleClaimRepository,
    SaleRepository, SaleService, SecretService,
};
use conta_verde_domain::{
    Access, Actor, ApiKeyDraft, ApiKeyId, Area, AuditAction, CalendarDate, CompanyId, DomainError,
    MAX_API_KEYS, SaleId, SaleStatus,
};

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
    assert_eq!(service.list(company).await.unwrap().len(), 2);

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
