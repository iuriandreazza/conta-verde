//! Behavior every adapter must share, written once and run against each of them.

use conta_verde_application::{
    ApplicationError, AuthError, AuthService, CnpjRegistry, CostCenterQuery, CostCenterRepository,
    CostCenterService, IdentityRepository, NameSort, PartyKind, PartyQuery, PartyRepository,
    PartyService, ProductQuery, ProductRepository, ProductService, RepositoryError, SecretService,
};
use conta_verde_domain::{
    AccountDraft, Actor, AuditAction, AuditEntity, AuditEntry, CompanyId, CostCenter,
    CostCenterDraft, CostCenterId, DocumentCheck, DomainError, FieldChange, Party, PartyDraft,
    PartyId, Product, ProductDraft, ProductId, ProductKind, UserId, cost_center_changes,
    cost_center_creation_changes, creation_changes, document_check_changes, product_changes,
    product_creation_changes,
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

/// Every name the query lists, a page at a time, so that each case also proves that the cursor
/// resumes correctly under that filter and sort.
async fn listed_names<R: PartyRepository, C: CnpjRegistry>(
    service: &PartyService<R, C>,
    company: CompanyId,
    query: &PartyQuery,
    page_size: usize,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = None;
    loop {
        let page = service
            .list(company, query, cursor.as_ref(), Some(page_size))
            .await
            .unwrap();
        names.extend(page.items.iter().map(|party| party.name().to_owned()));
        match page.next {
            Some(next) => cursor = Some(next),
            None => return names,
        }
    }
}

/// Names that differ only in case tie on the sort key, so the id decides: one page at a time, in
/// both directions, nothing may be skipped or repeated and descending must mirror ascending.
pub async fn breaks_ties_between_equal_names<R: PartyRepository, C: CnpjRegistry>(
    service: &PartyService<R, C>,
    company: CompanyId,
) {
    for name in ["Ana", "ana", "ANA", "Bia"] {
        let draft = PartyDraft {
            name: name.into(),
            document: None,
            email: None,
            is_customer: true,
            is_supplier: false,
        };
        service.register(company, &actor(), draft).await.unwrap();
    }

    let ascending = listed_names(service, company, &PartyQuery::default(), 1).await;
    let descending = listed_names(
        service,
        company,
        &PartyQuery::default().sorted(NameSort::NameDesc),
        1,
    )
    .await;
    assert_eq!(ascending.len(), 4);
    assert_eq!(ascending.last().map(String::as_str), Some("Bia"));
    assert_eq!(
        descending,
        ascending.iter().rev().cloned().collect::<Vec<_>>()
    );
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
    let names = |query: PartyQuery| async move { listed_names(service, company, &query, 2).await };

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
        names(all.clone().sorted(NameSort::NameDesc)).await,
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
        .sorted(NameSort::NameDesc);
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

// ---- products and services

fn product_draft(name: &str, kind: ProductKind, sku: Option<&str>, active: bool) -> ProductDraft {
    let is_product = kind == ProductKind::Product;
    ProductDraft {
        kind,
        name: name.into(),
        sku: sku.map(Into::into),
        unit: "UN".into(),
        ncm: is_product.then(|| "1905.90.90".into()),
        sale_price: Some("10.50".into()),
        cost_price: Some("4".into()),
        minimum_stock: is_product.then(|| "2.5".into()),
        active,
    }
}

fn product_entry(product: &Product, action: AuditAction, changes: Vec<FieldChange>) -> AuditEntry {
    AuditEntry {
        company_id: product.company_id(),
        actor: actor(),
        entity: AuditEntity::Product,
        entity_id: product.id().as_uuid(),
        action,
        changes,
    }
}

fn new_product(company: CompanyId, name: &str, sku: Option<&str>) -> Product {
    Product::new(
        ProductId::generate(),
        company,
        product_draft(name, ProductKind::Product, sku, true),
    )
    .unwrap()
}

async fn trail<R: ProductRepository>(
    repository: &R,
    company: CompanyId,
    product: &Product,
) -> usize {
    repository
        .history(company, product.id())
        .await
        .unwrap()
        .len()
}

/// A write is stored with its audit entry, scoped by company; one that is refused or misses leaves
/// nothing in the trail, and an entry that describes something else is refused outright.
pub async fn stores_products_with_their_trail<R: ProductRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let bread = new_product(company, "Pão", Some("PAO-1"));
    let entry = product_entry(
        &bread,
        AuditAction::Created,
        product_creation_changes(&bread),
    );
    repository.save(&bread, &entry).await.unwrap();

    // What comes back is what went in, whichever way the adapter keeps amounts.
    let stored = repository.find(company, bread.id()).await.unwrap().unwrap();
    assert_eq!(stored, bread);
    assert_eq!(stored.sale_price().unwrap().to_string(), "10.5");
    assert_eq!(stored.minimum_stock().unwrap().to_string(), "2.5");
    // Another company's product looks missing.
    assert_eq!(
        repository.find(other_company, bread.id()).await.unwrap(),
        None
    );

    let history = repository.history(company, bread.id()).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].entry, entry);
    assert!(
        repository
            .history(other_company, bread.id())
            .await
            .unwrap()
            .is_empty()
    );

    // The same SKU in the same company is refused, with no row and no entry...
    let twin = new_product(company, "Pão 2", Some("PAO-1"));
    assert_eq!(
        repository
            .save(&twin, &product_entry(&twin, AuditAction::Created, vec![]))
            .await,
        Err(RepositoryError::DuplicateSku)
    );
    assert_eq!(repository.find(company, twin.id()).await.unwrap(), None);
    assert_eq!(trail(repository, company, &twin).await, 0);
    // ...but another company may use it, and a product without SKU never clashes.
    let elsewhere = new_product(other_company, "Pão", Some("PAO-1"));
    let entry = product_entry(&elsewhere, AuditAction::Created, vec![]);
    repository.save(&elsewhere, &entry).await.unwrap();
    for name in ["Sem SKU 1", "Sem SKU 2"] {
        let bare = new_product(company, name, None);
        let entry = product_entry(&bare, AuditAction::Created, vec![]);
        repository.save(&bare, &entry).await.unwrap();
    }

    // An entry about another record, or another company, is refused and nothing is stored.
    let stray = new_product(company, "Estranho", None);
    let wrong_record = product_entry(&bread, AuditAction::Created, vec![]);
    assert!(repository.save(&stray, &wrong_record).await.is_err());
    assert_eq!(repository.find(company, stray.id()).await.unwrap(), None);
    let mut wrong_company = product_entry(&stray, AuditAction::Created, vec![]);
    wrong_company.company_id = other_company;
    assert!(repository.save(&stray, &wrong_company).await.is_err());
    assert_eq!(trail(repository, other_company, &stray).await, 0);

    // An update replaces the fields and adds an entry...
    let renamed = bread
        .replaced_with(product_draft(
            "Pão francês",
            ProductKind::Product,
            Some("PAO-1"),
            false,
        ))
        .unwrap();
    let changes = product_changes(&bread, &renamed);
    let entry = product_entry(&renamed, AuditAction::Updated, changes);
    repository.update(&renamed, &entry).await.unwrap();
    assert_eq!(
        repository.find(company, bread.id()).await.unwrap(),
        Some(renamed.clone())
    );
    assert_eq!(trail(repository, company, &bread).await, 2);

    // ...unless it clashes with another product's SKU: then it changes nothing and adds nothing.
    let bare = repository
        .list_by_company(
            company,
            &ProductQuery::default().searching("Sem SKU 1"),
            None,
            5,
        )
        .await
        .unwrap()
        .remove(0);
    let clashing = bare
        .replaced_with(product_draft(
            "Sem SKU 1",
            ProductKind::Product,
            Some("PAO-1"),
            true,
        ))
        .unwrap();
    let entry = product_entry(
        &clashing,
        AuditAction::Updated,
        product_changes(&bare, &clashing),
    );
    assert_eq!(
        repository.update(&clashing, &entry).await,
        Err(RepositoryError::DuplicateSku)
    );
    assert_eq!(
        repository.find(company, bare.id()).await.unwrap(),
        Some(bare.clone())
    );
    // Only the entry of its creation: the refused update added none.
    assert_eq!(trail(repository, company, &bare).await, 1);

    // Keeping its own SKU is not a clash.
    let same_sku = renamed
        .replaced_with(product_draft(
            "Pão francês 2",
            ProductKind::Product,
            Some("PAO-1"),
            true,
        ))
        .unwrap();
    let entry = product_entry(
        &same_sku,
        AuditAction::Updated,
        product_changes(&renamed, &same_sku),
    );
    repository.update(&same_sku, &entry).await.unwrap();

    // The kind is fixed at registration: the port refuses to change it, and nothing is written.
    let kind_swap = Product::new(
        bread.id(),
        company,
        product_draft("Pão serviço", ProductKind::Service, None, true),
    )
    .unwrap();
    let entry = product_entry(&kind_swap, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&kind_swap, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository
            .find(company, bread.id())
            .await
            .unwrap()
            .unwrap()
            .kind(),
        ProductKind::Product
    );

    // A SKU given up by a rename is free for another product of the company.
    let freed = same_sku
        .replaced_with(product_draft(
            "Pão francês 2",
            ProductKind::Product,
            Some("PAO-2"),
            true,
        ))
        .unwrap();
    let entry = product_entry(
        &freed,
        AuditAction::Updated,
        product_changes(&same_sku, &freed),
    );
    repository.update(&freed, &entry).await.unwrap();
    let reuse = new_product(company, "Pão novo", Some("PAO-1"));
    let entry = product_entry(
        &reuse,
        AuditAction::Created,
        product_creation_changes(&reuse),
    );
    repository.save(&reuse, &entry).await.unwrap();

    // A product that is not in that company cannot be updated from it, and leaves no entry.
    let foreign = Product::new(
        bread.id(),
        other_company,
        product_draft("Roubo", ProductKind::Product, None, true),
    )
    .unwrap();
    let entry = product_entry(&foreign, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&foreign, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(trail(repository, other_company, &bread).await, 0);
    let unknown = new_product(company, "Fantasma", None);
    let entry = product_entry(&unknown, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&unknown, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(trail(repository, company, &unknown).await, 0);
}

/// An edit that changes nothing writes nothing, and the trail shows who changed what.
pub async fn edits_products_and_records_only_changes<R: ProductRepository>(
    service: &ProductService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let created = service
        .register(
            company,
            &actor(),
            product_draft("Bolo", ProductKind::Product, Some("bolo-1"), true),
        )
        .await
        .unwrap();
    assert_eq!(created.sku(), Some("BOLO-1"));

    // The same values, written differently, are no change.
    let same = ProductDraft {
        sale_price: Some("10.5000".into()),
        ..product_draft("Bolo", ProductKind::Product, Some("BOLO-1"), true)
    };
    service
        .update(company, &actor(), created.id(), same)
        .await
        .unwrap();
    assert_eq!(
        service.history(company, created.id()).await.unwrap().len(),
        1
    );

    let changed = ProductDraft {
        sale_price: Some("12".into()),
        ..product_draft("Bolo", ProductKind::Product, Some("BOLO-1"), false)
    };
    let updated = service
        .update(company, &actor(), created.id(), changed)
        .await
        .unwrap();
    assert_eq!(updated.sale_price().unwrap().to_string(), "12");
    assert!(!updated.is_active());

    let history = service.history(company, created.id()).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].entry.action, AuditAction::Created);
    assert_eq!(history[1].entry.action, AuditAction::Updated);
    let fields: Vec<(&str, Option<&str>, Option<&str>)> = history[1]
        .entry
        .changes
        .iter()
        .map(|change| {
            (
                change.field.as_str(),
                change.from.as_deref(),
                change.to.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            ("sale_price", Some("10.5"), Some("12")),
            ("active", Some("true"), Some("false"))
        ]
    );

    // A product is the same kind for good, and other companies cannot touch or read it.
    let as_service = product_draft("Bolo", ProductKind::Service, None, true);
    assert_eq!(
        service
            .update(company, &actor(), created.id(), as_service)
            .await,
        Err(ApplicationError::Domain(DomainError::KindCannotChange))
    );
    let draft = product_draft("Bolo", ProductKind::Product, None, true);
    assert_eq!(
        service
            .update(other_company, &actor(), created.id(), draft)
            .await,
        Err(ApplicationError::ProductNotFound)
    );
    assert_eq!(
        service.history(other_company, created.id()).await,
        Err(ApplicationError::ProductNotFound)
    );
}

/// Every name the query lists, a page at a time, so that each case also proves that the cursor
/// resumes correctly under that filter and sort.
async fn listed_product_names<R: ProductRepository>(
    service: &ProductService<R>,
    company: CompanyId,
    query: &ProductQuery,
    page_size: usize,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = None;
    loop {
        let page = service
            .list(company, query, cursor.as_ref(), Some(page_size))
            .await
            .unwrap();
        names.extend(page.items.iter().map(|product| product.name().to_owned()));
        match page.next {
            Some(next) => cursor = Some(next),
            None => return names,
        }
    }
}

pub async fn filters_and_sorts_products<R: ProductRepository>(
    service: &ProductService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let register =
        |company: CompanyId, name: &str, kind: ProductKind, sku: Option<&str>, active: bool| {
            let draft = product_draft(name, kind, sku, active);
            async move { service.register(company, &actor(), draft).await.unwrap() }
        };
    use ProductKind::{Product as Good, Service};
    register(company, "Alfa", Good, Some("AL-1"), true).await;
    register(company, "bravo", Good, None, false).await;
    register(company, "Charlie Service", Service, Some("SRV/01"), true).await;
    register(company, "Delta 100%", Good, Some("DL_2"), true).await;
    register(company, "echo", Service, None, false).await;
    // Another company's product never shows up, whatever the text, and may reuse a SKU.
    register(other_company, "Alfa", Good, Some("AL-1"), true).await;

    let all = ProductQuery::default();
    let names = |query: ProductQuery| async move {
        listed_product_names(service, company, &query, 2).await
    };

    let ascending = ["Alfa", "bravo", "Charlie Service", "Delta 100%", "echo"];
    assert_eq!(names(all.clone()).await, ascending);
    let descending: Vec<&str> = ascending.iter().rev().copied().collect();
    assert_eq!(
        names(all.clone().sorted(NameSort::NameDesc)).await,
        descending
    );

    // Inactive items are listed unless the query says otherwise.
    assert_eq!(
        names(all.clone().with_active(true)).await,
        ["Alfa", "Charlie Service", "Delta 100%"]
    );
    assert_eq!(
        names(all.clone().with_active(false)).await,
        ["bravo", "echo"]
    );
    assert_eq!(
        names(all.clone().of_kind(Service)).await,
        ["Charlie Service", "echo"]
    );
    assert_eq!(
        names(all.clone().of_kind(Good)).await,
        ["Alfa", "bravo", "Delta 100%"]
    );

    // Search: a name or a SKU anywhere in it, ignoring case.
    assert_eq!(names(all.clone().searching("ALF")).await, ["Alfa"]);
    assert_eq!(names(all.clone().searching("  al-1 ")).await, ["Alfa"]);
    assert_eq!(
        names(all.clone().searching("srv/")).await,
        ["Charlie Service"]
    );
    // `%` and `_` are ordinary characters, not wildcards.
    assert_eq!(names(all.clone().searching("%")).await, ["Delta 100%"]);
    assert_eq!(names(all.clone().searching("_")).await, ["Delta 100%"]);
    assert_eq!(
        names(all.clone().searching("a")).await,
        ["Alfa", "bravo", "Charlie Service", "Delta 100%"]
    );
    assert!(names(all.clone().searching("nobody")).await.is_empty());
    // Control characters (a NUL would be a storage error) are dropped rather than searched for.
    assert_eq!(names(all.clone().searching("al\0fa")).await, ["Alfa"]);
    assert_eq!(names(all.clone().searching("   ")).await, ascending);

    // Filters and sort combine, and the cursor keeps working across pages.
    let inactive_services = all.clone().of_kind(Service).with_active(false);
    assert_eq!(names(inactive_services).await, ["echo"]);
    let active_with_a = all
        .with_active(true)
        .searching("a")
        .sorted(NameSort::NameDesc);
    assert_eq!(
        names(active_with_a).await,
        ["Delta 100%", "Charlie Service", "Alfa"]
    );
}

/// Names that differ only in case tie on the sort key, so the id decides: one page at a time, in
/// both directions, nothing may be skipped or repeated and descending must mirror ascending.
pub async fn breaks_ties_between_equal_product_names<R: ProductRepository>(
    service: &ProductService<R>,
    company: CompanyId,
) {
    for name in ["Ana", "ana", "ANA", "Bia"] {
        let draft = product_draft(name, ProductKind::Product, None, true);
        service.register(company, &actor(), draft).await.unwrap();
    }
    let all = ProductQuery::default();
    let ascending = listed_product_names(service, company, &all, 1).await;
    let descending =
        listed_product_names(service, company, &all.sorted(NameSort::NameDesc), 1).await;
    assert_eq!(ascending.len(), 4);
    assert_eq!(ascending.last().map(String::as_str), Some("Bia"));
    assert_eq!(
        descending,
        ascending.iter().rev().cloned().collect::<Vec<_>>()
    );
}

// ---- cost centers

fn cost_center_entry(
    cost_center: &CostCenter,
    action: AuditAction,
    changes: Vec<FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: cost_center.company_id(),
        actor: actor(),
        entity: AuditEntity::CostCenter,
        entity_id: cost_center.id().as_uuid(),
        action,
        changes,
    }
}

fn cost_center_draft(name: &str, active: bool) -> CostCenterDraft {
    CostCenterDraft {
        name: name.into(),
        active,
    }
}

fn new_cost_center(company: CompanyId, name: &str) -> CostCenter {
    CostCenter::new(
        CostCenterId::generate(),
        company,
        cost_center_draft(name, true),
    )
    .unwrap()
}

async fn cost_center_trail<R: CostCenterRepository>(
    repository: &R,
    company: CompanyId,
    cost_center: &CostCenter,
) -> usize {
    repository
        .history(company, cost_center.id())
        .await
        .unwrap()
        .len()
}

/// A write is stored with its audit entry, scoped by company; one that is refused or misses leaves
/// nothing in the trail, and an entry that describes something else is refused outright.
pub async fn stores_cost_centers_with_their_trail<R: CostCenterRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let store = new_cost_center(company, "Loja Centro");
    let entry = cost_center_entry(
        &store,
        AuditAction::Created,
        cost_center_creation_changes(&store),
    );
    repository.save(&store, &entry).await.unwrap();
    assert_eq!(
        repository.find(company, store.id()).await.unwrap(),
        Some(store.clone())
    );
    assert_eq!(
        repository.find(other_company, store.id()).await.unwrap(),
        None
    );
    let history = repository.history(company, store.id()).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].entry, entry);
    assert!(
        repository
            .history(other_company, store.id())
            .await
            .unwrap()
            .is_empty()
    );

    // The same name in the same company, whatever its case, is refused: no row, no entry.
    let twin = new_cost_center(company, "LOJA centro");
    let entry = cost_center_entry(&twin, AuditAction::Created, vec![]);
    assert_eq!(
        repository.save(&twin, &entry).await,
        Err(RepositoryError::DuplicateName)
    );
    assert_eq!(repository.find(company, twin.id()).await.unwrap(), None);
    assert_eq!(cost_center_trail(repository, company, &twin).await, 0);
    // Another company may use it.
    let elsewhere = new_cost_center(other_company, "Loja Centro");
    let entry = cost_center_entry(&elsewhere, AuditAction::Created, vec![]);
    repository.save(&elsewhere, &entry).await.unwrap();

    // An entry about another record, or another company, is refused and nothing is stored.
    let stray = new_cost_center(company, "Estranho");
    let wrong_record = cost_center_entry(&store, AuditAction::Created, vec![]);
    assert!(repository.save(&stray, &wrong_record).await.is_err());
    assert_eq!(repository.find(company, stray.id()).await.unwrap(), None);
    let mut wrong_company = cost_center_entry(&stray, AuditAction::Created, vec![]);
    wrong_company.company_id = other_company;
    assert!(repository.save(&stray, &wrong_company).await.is_err());
    assert_eq!(
        cost_center_trail(repository, other_company, &stray).await,
        0
    );

    // An update replaces the fields and adds an entry; changing only the case of its own name is
    // not a clash.
    let recased = store
        .replaced_with(cost_center_draft("LOJA CENTRO", false))
        .unwrap();
    let entry = cost_center_entry(
        &recased,
        AuditAction::Updated,
        cost_center_changes(&store, &recased),
    );
    repository.update(&recased, &entry).await.unwrap();
    assert_eq!(
        repository.find(company, store.id()).await.unwrap(),
        Some(recased.clone())
    );
    assert_eq!(cost_center_trail(repository, company, &store).await, 2);

    // An update that clashes with another cost center's name changes nothing and adds nothing.
    let other = new_cost_center(company, "Projeto X");
    let entry = cost_center_entry(&other, AuditAction::Created, vec![]);
    repository.save(&other, &entry).await.unwrap();
    let clashing = other
        .replaced_with(cost_center_draft("loja centro", true))
        .unwrap();
    let entry = cost_center_entry(
        &clashing,
        AuditAction::Updated,
        cost_center_changes(&other, &clashing),
    );
    assert_eq!(
        repository.update(&clashing, &entry).await,
        Err(RepositoryError::DuplicateName)
    );
    assert_eq!(
        repository.find(company, other.id()).await.unwrap(),
        Some(other.clone())
    );
    // Only the entry of its creation.
    assert_eq!(cost_center_trail(repository, company, &other).await, 1);

    // A name given up by a rename is free again.
    let renamed = other
        .replaced_with(cost_center_draft("Projeto Y", true))
        .unwrap();
    let entry = cost_center_entry(
        &renamed,
        AuditAction::Updated,
        cost_center_changes(&other, &renamed),
    );
    repository.update(&renamed, &entry).await.unwrap();
    let reuse = new_cost_center(company, "projeto x");
    let entry = cost_center_entry(&reuse, AuditAction::Created, vec![]);
    repository.save(&reuse, &entry).await.unwrap();

    // Not in that company, or not at all: nothing is updated and no entry is left.
    let foreign =
        CostCenter::new(store.id(), other_company, cost_center_draft("Roubo", true)).unwrap();
    let entry = cost_center_entry(&foreign, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&foreign, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        cost_center_trail(repository, other_company, &store).await,
        0
    );
    let unknown = new_cost_center(company, "Fantasma");
    let entry = cost_center_entry(&unknown, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&unknown, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(cost_center_trail(repository, company, &unknown).await, 0);
}

/// An edit that changes nothing writes nothing, and the trail shows who changed what.
pub async fn edits_cost_centers_and_records_only_changes<R: CostCenterRepository>(
    service: &CostCenterService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let created = service
        .register(company, &actor(), cost_center_draft("  Loja Centro ", true))
        .await
        .unwrap();
    assert_eq!(created.name(), "Loja Centro");

    // The same values are no change.
    service
        .update(
            company,
            &actor(),
            created.id(),
            cost_center_draft("Loja Centro", true),
        )
        .await
        .unwrap();
    assert_eq!(
        service.history(company, created.id()).await.unwrap().len(),
        1
    );

    let updated = service
        .update(
            company,
            &actor(),
            created.id(),
            cost_center_draft("Loja Norte", false),
        )
        .await
        .unwrap();
    assert_eq!(updated.name(), "Loja Norte");
    assert!(!updated.is_active());
    let history = service.history(company, created.id()).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].entry.action, AuditAction::Updated);
    let fields: Vec<(&str, Option<&str>, Option<&str>)> = history[1]
        .entry
        .changes
        .iter()
        .map(|change| {
            (
                change.field.as_str(),
                change.from.as_deref(),
                change.to.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            ("name", Some("Loja Centro"), Some("Loja Norte")),
            ("active", Some("true"), Some("false"))
        ]
    );

    // Other companies cannot touch or read it.
    assert_eq!(
        service
            .update(
                other_company,
                &actor(),
                created.id(),
                cost_center_draft("X", true)
            )
            .await,
        Err(ApplicationError::CostCenterNotFound)
    );
    assert_eq!(
        service.history(other_company, created.id()).await,
        Err(ApplicationError::CostCenterNotFound)
    );
    assert_eq!(
        service
            .register(company, &actor(), cost_center_draft("loja norte", true))
            .await,
        Err(ApplicationError::Repository(RepositoryError::DuplicateName))
    );
}

async fn listed_cost_center_names<R: CostCenterRepository>(
    service: &CostCenterService<R>,
    company: CompanyId,
    query: &CostCenterQuery,
    page_size: usize,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = None;
    loop {
        let page = service
            .list(company, query, cursor.as_ref(), Some(page_size))
            .await
            .unwrap();
        names.extend(page.items.iter().map(|item| item.name().to_owned()));
        match page.next {
            Some(next) => cursor = Some(next),
            None => return names,
        }
    }
}

pub async fn filters_and_sorts_cost_centers<R: CostCenterRepository>(
    service: &CostCenterService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    for (name, active) in [
        ("Alfa", true),
        ("bravo", false),
        ("Charlie 100%", true),
        ("Delta_x", true),
        ("echo", false),
    ] {
        service
            .register(company, &actor(), cost_center_draft(name, active))
            .await
            .unwrap();
    }
    // Another company's cost center never shows up, and may reuse a name.
    service
        .register(other_company, &actor(), cost_center_draft("Alfa", true))
        .await
        .unwrap();

    let all = CostCenterQuery::default();
    let names = |query: CostCenterQuery| async move {
        listed_cost_center_names(service, company, &query, 2).await
    };
    let ascending = ["Alfa", "bravo", "Charlie 100%", "Delta_x", "echo"];
    assert_eq!(names(all.clone()).await, ascending);
    let descending: Vec<&str> = ascending.iter().rev().copied().collect();
    assert_eq!(
        names(all.clone().sorted(NameSort::NameDesc)).await,
        descending
    );

    // Inactive ones are listed unless the query says otherwise.
    assert_eq!(
        names(all.clone().with_active(true)).await,
        ["Alfa", "Charlie 100%", "Delta_x"]
    );
    assert_eq!(
        names(all.clone().with_active(false)).await,
        ["bravo", "echo"]
    );

    assert_eq!(names(all.clone().searching("ALF")).await, ["Alfa"]);
    assert_eq!(names(all.clone().searching("%")).await, ["Charlie 100%"]);
    assert_eq!(names(all.clone().searching("_")).await, ["Delta_x"]);
    assert!(names(all.clone().searching("nobody")).await.is_empty());
    assert_eq!(names(all.clone().searching("al\0fa")).await, ["Alfa"]);
    assert_eq!(names(all.clone().searching("  ")).await, ascending);

    let active_with_a = all
        .with_active(true)
        .searching("a")
        .sorted(NameSort::NameDesc);
    assert_eq!(
        names(active_with_a).await,
        ["Delta_x", "Charlie 100%", "Alfa"]
    );
}
