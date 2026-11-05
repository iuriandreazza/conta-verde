//! Behavior every adapter must share, written once and run against each of them.

use conta_verde_application::{
    ApplicationError, AuthError, AuthService, CnpjRegistry, IdentityRepository, PartyKind,
    PartyQuery, PartyRepository, PartyService, PartySort, RepositoryError, SecretService,
};
use conta_verde_domain::{
    AccountDraft, Actor, AuditAction, AuditEntity, AuditEntry, CompanyId, DocumentCheck, Party,
    PartyDraft, PartyId, UserId, creation_changes, document_check_changes,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::InMemoryCnpjRegistry;

pub const ACTIVE_CNPJ: &str = "11.222.333/0001-81";
pub const CLOSED_ALPHANUMERIC_CNPJ: &str = "12.ABC.345/01DE-35";
/// Valid check digits, but unknown to the contract registry.
pub const UNKNOWN_CNPJ: &str = "00.000.000/0001-91";

/// The user every contract test acts as. Postgres keeps no foreign key on the actor, so the id
/// need not belong to a stored user.
pub fn actor() -> Actor {
    Actor::User {
        id: UserId::generate(),
        name: "Ana Souza".into(),
    }
}

/// A registry that knows the two companies the contract tests register, and no others.
pub fn registry() -> InMemoryCnpjRegistry {
    InMemoryCnpjRegistry::default()
        .with_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA")
        .with_company("12ABC34501DE35", "OFICINA ALFANUMERICA LTDA", "BAIXADA")
}

pub async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    crate::connect_and_migrate(&url).await.unwrap()
}

/// Parties reference companies, so database tests need a real one.
pub async fn create_company(pool: &PgPool) -> CompanyId {
    let id = CompanyId::generate();
    sqlx::query("INSERT INTO companies (id, name) VALUES ($1, 'Test company')")
        .bind(id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    id
}

fn party_draft(name: &str, document: Option<&str>) -> PartyDraft {
    PartyDraft {
        name: name.into(),
        document: document.map(Into::into),
        email: None,
        is_customer: true,
        is_supplier: false,
    }
}

/// The registry's answer is stored with the party and comes back unchanged. Expects a service
/// built with `registry()`.
pub async fn keeps_what_the_registry_said<R: PartyRepository>(
    service: &PartyService<R, InMemoryCnpjRegistry>,
    company: CompanyId,
) {
    for (name, document) in [
        ("Padaria", Some(ACTIVE_CNPJ)),
        ("Oficina", Some(CLOSED_ALPHANUMERIC_CNPJ)),
        ("Fantasma", Some(UNKNOWN_CNPJ)),
        ("Ana", Some("529.982.247-25")),
        ("Sem documento", None),
    ] {
        service
            .register(company, &actor(), party_draft(name, document))
            .await
            .unwrap();
    }

    let listed = service
        .list(company, &PartyQuery::default(), None, None)
        .await
        .unwrap()
        .items;
    let checks: Vec<(&str, DocumentCheck)> = listed
        .iter()
        .map(|party| (party.name(), party.document_check().clone()))
        .collect();
    let verified = |name: &str, situation: &str| DocumentCheck::Verified {
        legal_name: name.into(),
        situation: situation.into(),
    };
    assert_eq!(
        checks,
        [
            ("Ana", DocumentCheck::NotApplicable),
            ("Fantasma", DocumentCheck::NotFound),
            ("Oficina", verified("OFICINA ALFANUMERICA LTDA", "BAIXADA")),
            ("Padaria", verified("PADARIA CENTRAL LTDA", "ATIVA")),
            ("Sem documento", DocumentCheck::NotApplicable),
        ]
    );
    assert_eq!(
        listed[2].document().map(|document| document.as_str()),
        Some("12ABC34501DE35")
    );
}

/// A CNPJ stored while the registry was down stays marked as unverified.
pub async fn keeps_the_unverified_mark<R: PartyRepository>(
    service: &PartyService<R, InMemoryCnpjRegistry>,
    company: CompanyId,
) {
    service
        .register(company, &actor(), party_draft("Padaria", Some(ACTIVE_CNPJ)))
        .await
        .unwrap();
    let listed = service
        .list(company, &PartyQuery::default(), None, None)
        .await
        .unwrap()
        .items;
    assert_eq!(listed[0].document_check(), &DocumentCheck::Unverified);
}

pub fn created_entry(party: &Party) -> AuditEntry {
    AuditEntry {
        company_id: party.company_id(),
        actor: actor(),
        entity: AuditEntity::Party,
        entity_id: party.id().as_uuid(),
        action: AuditAction::Created,
        changes: creation_changes(party),
    }
}

/// An update entry for a party whose document check went from `before` to `after`.
pub fn check_entry(before: &Party, after: &Party) -> AuditEntry {
    AuditEntry {
        company_id: after.company_id(),
        actor: actor(),
        entity: AuditEntity::Party,
        entity_id: after.id().as_uuid(),
        action: AuditAction::Updated,
        changes: document_check_changes(before.document_check(), after.document_check()),
    }
}

/// The audit trail is written together with the write it describes, in order, scoped by company,
/// and never for a write that did not happen.
pub async fn records_what_was_written<R: PartyRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let unverified = Party::new(
        PartyId::generate(),
        company,
        party_draft("Padaria", Some(ACTIVE_CNPJ)),
    )
    .unwrap()
    .with_document_check(DocumentCheck::Unverified);
    let created = created_entry(&unverified);
    repository.save(&unverified, &created).await.unwrap();

    let verified = unverified
        .clone()
        .with_document_check(DocumentCheck::Verified {
            legal_name: "PADARIA CENTRAL LTDA".into(),
            situation: "ATIVA".into(),
        });
    let rechecked = check_entry(&unverified, &verified);
    repository
        .update_document_check(&verified, &rechecked)
        .await
        .unwrap();

    // Oldest first, with a growing sequence and a real time, carrying exactly what was written.
    let history = repository.history(company, unverified.id()).await.unwrap();
    assert_eq!(
        history
            .iter()
            .map(|record| &record.entry)
            .collect::<Vec<_>>(),
        [&created, &rechecked]
    );
    assert!(history[0].sequence < history[1].sequence);
    assert!(
        history
            .iter()
            .all(|record| record.at_millis > 1_700_000_000_000)
    );
    assert!(history[0].at_millis <= history[1].at_millis);
    assert_eq!(history[0].entry.action, AuditAction::Created);
    assert_eq!(history[1].entry.action, AuditAction::Updated);
    // The re-check lists only the fields that changed.
    let changed: Vec<&str> = history[1]
        .entry
        .changes
        .iter()
        .map(|change| change.field.as_str())
        .collect();
    assert_eq!(
        changed,
        [
            "document_check",
            "registry_legal_name",
            "registry_situation"
        ]
    );

    // Scoped by company: the same party id read through another company shows nothing.
    assert!(
        repository
            .history(other_company, unverified.id())
            .await
            .unwrap()
            .is_empty()
    );
    // And another party has its own, separate trail.
    assert!(
        repository
            .history(company, PartyId::generate())
            .await
            .unwrap()
            .is_empty()
    );
}

/// A write that is refused or misses leaves nothing in the trail, and a trail entry is never
/// stored without its write. Postgres does it with a transaction, memory by recording last.
pub async fn leaves_no_trace_of_a_write_that_did_not_happen<R: PartyRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let first = Party::new(
        PartyId::generate(),
        company,
        party_draft("Padaria", Some(ACTIVE_CNPJ)),
    )
    .unwrap();
    repository
        .save(&first, &created_entry(&first))
        .await
        .unwrap();

    // The same document twice in one company is refused...
    let duplicate = Party::new(
        PartyId::generate(),
        company,
        party_draft("Padaria 2", Some(ACTIVE_CNPJ)),
    )
    .unwrap();
    assert_eq!(
        repository
            .save(&duplicate, &created_entry(&duplicate))
            .await,
        Err(RepositoryError::DuplicateDocument)
    );
    // ...and the refused party left no entry behind.
    assert!(
        repository
            .history(company, duplicate.id())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repository.history(company, first.id()).await.unwrap().len(),
        1
    );

    // An update through the wrong company misses, and leaves no entry either.
    let wrong_company = Party::new(
        first.id(),
        other_company,
        party_draft("Padaria", Some(ACTIVE_CNPJ)),
    )
    .unwrap()
    .with_document_check(DocumentCheck::NotFound);
    assert_eq!(
        repository
            .update_document_check(&wrong_company, &check_entry(&first, &wrong_company))
            .await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.history(company, first.id()).await.unwrap().len(),
        1
    );

    // An entry that is not about this party (another company's trail, another record) is a
    // caller's bug: refused, with neither the party nor an entry stored.
    let stranger = Party::new(
        PartyId::generate(),
        company,
        party_draft("Oficina", Some(CLOSED_ALPHANUMERIC_CNPJ)),
    )
    .unwrap();
    let for_another_company = AuditEntry {
        company_id: other_company,
        ..created_entry(&stranger)
    };
    let for_another_record = AuditEntry {
        entity_id: first.id().as_uuid(),
        ..created_entry(&stranger)
    };
    for mismatched in [&for_another_company, &for_another_record] {
        assert!(matches!(
            repository.save(&stranger, mismatched).await,
            Err(RepositoryError::Storage(_))
        ));
    }
    assert_eq!(repository.find(company, stranger.id()).await.unwrap(), None);
    assert!(
        repository
            .history(other_company, stranger.id())
            .await
            .unwrap()
            .is_empty()
    );
    // The same guard protects the update.
    let rechecked = first.clone().with_document_check(DocumentCheck::NotFound);
    assert!(matches!(
        repository
            .update_document_check(&rechecked, &for_another_company)
            .await,
        Err(RepositoryError::Storage(_))
    ));
    assert_eq!(
        repository.history(company, first.id()).await.unwrap().len(),
        1
    );
}

/// A party is found only inside its own company, and replacing its document check changes
/// nothing else about it.
pub async fn finds_parties_and_replaces_their_document_check<R: PartyRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let party = Party::new(
        PartyId::generate(),
        company,
        party_draft("Padaria", Some(ACTIVE_CNPJ)),
    )
    .unwrap()
    .with_document_check(DocumentCheck::Unverified);
    repository
        .save(&party, &created_entry(&party))
        .await
        .unwrap();

    assert_eq!(
        repository.find(company, party.id()).await.unwrap(),
        Some(party.clone())
    );
    assert_eq!(
        repository.find(other_company, party.id()).await.unwrap(),
        None
    );
    assert_eq!(
        repository.find(company, PartyId::generate()).await.unwrap(),
        None
    );

    let verified = party.clone().with_document_check(DocumentCheck::Verified {
        legal_name: "PADARIA CENTRAL LTDA".into(),
        situation: "ATIVA".into(),
    });
    repository
        .update_document_check(&verified, &check_entry(&party, &verified))
        .await
        .unwrap();
    assert_eq!(
        repository.find(company, party.id()).await.unwrap(),
        Some(verified.clone())
    );

    // The company id is the only tenancy guard on this write: a party object carrying the same id
    // but another company must not change the stored one, and the miss must be reported.
    let intruder = Party::new(
        party.id(),
        other_company,
        party_draft("Padaria", Some(ACTIVE_CNPJ)),
    )
    .unwrap()
    .with_document_check(DocumentCheck::NotFound);
    assert_eq!(
        repository
            .update_document_check(&intruder, &check_entry(&party, &intruder))
            .await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.find(company, party.id()).await.unwrap(),
        Some(verified.clone())
    );

    // Going back is allowed too: the latest answer wins, whatever it was.
    let not_found = party.with_document_check(DocumentCheck::NotFound);
    repository
        .update_document_check(&not_found, &check_entry(&verified, &not_found))
        .await
        .unwrap();
    assert_eq!(
        repository.find(company, not_found.id()).await.unwrap(),
        Some(not_found)
    );
}

/// Every name the query lists, two to a page, so that each case also proves that the cursor
/// resumes correctly under that filter and sort.
async fn listed_names<R: PartyRepository, C: CnpjRegistry>(
    service: &PartyService<R, C>,
    company: CompanyId,
    query: &PartyQuery,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = None;
    loop {
        let page = service
            .list(company, query, cursor.as_ref(), Some(2))
            .await
            .unwrap();
        names.extend(page.items.iter().map(|party| party.name().to_owned()));
        match page.next {
            Some(next) => cursor = Some(next),
            None => return names,
        }
    }
}

pub async fn filters_and_sorts_parties<R: PartyRepository, C: CnpjRegistry>(
    service: &PartyService<R, C>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let register = |company: CompanyId,
                    name: &str,
                    document: Option<&str>,
                    email: Option<&str>,
                    is_customer: bool,
                    is_supplier: bool| {
        let draft = PartyDraft {
            name: name.into(),
            document: document.map(Into::into),
            email: email.map(Into::into),
            is_customer,
            is_supplier,
        };
        async move { service.register(company, &actor(), draft).await.unwrap() }
    };
    register(
        company,
        "Ana Souza",
        Some("529.982.247-25"),
        Some("ana@loja.com"),
        true,
        false,
    )
    .await;
    register(
        company,
        "padaria Central",
        Some(ACTIVE_CNPJ),
        Some("contato@padaria.com.br"),
        true,
        true,
    )
    .await;
    register(company, "Bruno Atacado", None, None, false, true).await;
    register(company, "100% Natural", None, None, true, false).await;
    register(company, "Under_score Ltda", None, None, false, true).await;
    // Another company's party never shows up, whatever the text.
    register(other_company, "Ana Souza", None, None, true, false).await;

    let all = PartyQuery::default();
    let names = |query: PartyQuery| async move { listed_names(service, company, &query).await };

    let ascending = [
        "100% Natural",
        "Ana Souza",
        "Bruno Atacado",
        "padaria Central",
        "Under_score Ltda",
    ];
    assert_eq!(names(all.clone()).await, ascending);
    let descending: Vec<&str> = ascending.iter().rev().copied().collect();
    assert_eq!(
        names(all.clone().sorted(PartySort::NameDesc)).await,
        descending
    );

    // A party that is both a customer and a supplier is in both lists.
    assert_eq!(
        names(all.clone().of_kind(PartyKind::Customer)).await,
        ["100% Natural", "Ana Souza", "padaria Central"]
    );
    assert_eq!(
        names(all.clone().of_kind(PartyKind::Supplier)).await,
        ["Bruno Atacado", "padaria Central", "Under_score Ltda"]
    );

    // Search: name or e-mail anywhere in it, ignoring case; a document ignoring punctuation.
    assert_eq!(names(all.clone().searching("ANA")).await, ["Ana Souza"]);
    assert_eq!(
        names(all.clone().searching("  padaria ")).await,
        ["padaria Central"]
    );
    assert_eq!(
        names(all.clone().searching("loja.com")).await,
        ["Ana Souza"]
    );
    assert_eq!(names(all.clone().searching("529.982")).await, ["Ana Souza"]);
    assert_eq!(
        names(all.clone().searching("52998224725")).await,
        ["Ana Souza"]
    );
    assert_eq!(
        names(all.clone().searching("11.222.333/0001")).await,
        ["padaria Central"]
    );
    // `%` and `_` are ordinary characters, not wildcards.
    assert_eq!(names(all.clone().searching("%")).await, ["100% Natural"]);
    assert_eq!(
        names(all.clone().searching("_")).await,
        ["Under_score Ltda"]
    );
    assert!(names(all.clone().searching("nobody")).await.is_empty());
    // Control characters (a NUL would be a storage error) are dropped rather than searched for.
    assert_eq!(names(all.clone().searching("an\0a")).await, ["Ana Souza"]);
    // Blank text is no search.
    assert_eq!(names(all.clone().searching("   ")).await, ascending);

    // Filters and sort combine, and the cursor keeps working across pages.
    let customers_with_a = all
        .of_kind(PartyKind::Customer)
        .searching("a")
        .sorted(PartySort::NameDesc);
    assert_eq!(
        names(customers_with_a).await,
        ["padaria Central", "Ana Souza", "100% Natural"]
    );
}

pub async fn pages_through_parties<R: PartyRepository, C: CnpjRegistry>(
    service: &PartyService<R, C>,
    company: CompanyId,
) {
    for name in ["delta", "Alfa", "charlie", "Bravo", "echo"] {
        let draft = PartyDraft {
            name: name.into(),
            document: None,
            email: None,
            is_customer: true,
            is_supplier: false,
        };
        service.register(company, &actor(), draft).await.unwrap();
    }

    let mut pages = Vec::new();
    let mut cursor = None;
    loop {
        let page = service
            .list(company, &PartyQuery::default(), cursor.as_ref(), Some(2))
            .await
            .unwrap();
        pages.push(
            page.items
                .iter()
                .map(|party| party.name().to_owned())
                .collect::<Vec<_>>(),
        );
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    assert_eq!(
        pages,
        [
            vec!["Alfa", "Bravo"],
            vec!["charlie", "delta"],
            vec!["echo"]
        ]
    );
}

fn draft(email: &str) -> AccountDraft {
    AccountDraft {
        user_name: "Ana".into(),
        email: email.into(),
        company_name: "Padaria".into(),
    }
}

pub async fn runs_the_account_lifecycle<I, S>(auth: &AuthService<I, S>)
where
    I: IdentityRepository,
    S: SecretService,
{
    let email = format!("ana-{}@example.com", Uuid::new_v4());
    let password = "correct horse";

    let signed_up = auth.register(draft(&email), password).await.unwrap();
    let company = signed_up.profile.companies[0].id();

    let duplicate = auth
        .register(draft(&email.to_uppercase()), password)
        .await
        .unwrap_err();
    assert_eq!(
        duplicate,
        ApplicationError::Repository(RepositoryError::DuplicateEmail)
    );

    let wrong_password = auth.login(&email, "wrong password").await.unwrap_err();
    assert_eq!(
        wrong_password,
        ApplicationError::Auth(AuthError::InvalidCredentials)
    );
    let unknown_email = auth
        .login("nobody@example.com", password)
        .await
        .unwrap_err();
    assert_eq!(
        unknown_email,
        ApplicationError::Auth(AuthError::InvalidCredentials)
    );

    let logged_in = auth.login(&email.to_uppercase(), password).await.unwrap();
    assert_ne!(logged_in.token, signed_up.token);
    let profile = auth.profile(&logged_in.token).await.unwrap();
    assert_eq!(profile.user.email().as_str(), email);
    assert_eq!(profile.companies.len(), 1);
    assert_eq!(profile.companies[0].name(), "Padaria");

    auth.authorize(&logged_in.token, company).await.unwrap();

    let stranger = auth
        .register(
            draft(&format!("bia-{}@example.com", Uuid::new_v4())),
            password,
        )
        .await
        .unwrap();
    assert_eq!(
        auth.authorize(&stranger.token, company).await.unwrap_err(),
        ApplicationError::Auth(AuthError::Forbidden)
    );
    assert_eq!(
        auth.authorize(&logged_in.token, CompanyId::generate())
            .await
            .unwrap_err(),
        ApplicationError::Auth(AuthError::Forbidden)
    );

    auth.logout(&logged_in.token).await.unwrap();
    assert_eq!(
        auth.profile(&logged_in.token).await.unwrap_err(),
        ApplicationError::Auth(AuthError::Unauthenticated)
    );
    // Logging out one session leaves the others alone.
    auth.profile(&signed_up.token).await.unwrap();
}

/// Expects an `AuthService` whose sessions last zero seconds.
pub async fn expired_sessions_are_rejected<I, S>(auth: &AuthService<I, S>)
where
    I: IdentityRepository,
    S: SecretService,
{
    let email = format!("ana-{}@example.com", Uuid::new_v4());
    let signed_up = auth.register(draft(&email), "correct horse").await.unwrap();
    assert_eq!(
        auth.profile(&signed_up.token).await.unwrap_err(),
        ApplicationError::Auth(AuthError::Unauthenticated)
    );
}
