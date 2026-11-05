//! Behavior every adapter must share, written once and run against each of them.

use conta_verde_application::{
    ApplicationError, AuthError, AuthService, CnpjRegistry, IdentityRepository, PartyRepository,
    PartyService, RepositoryError, SecretService,
};
use conta_verde_domain::{AccountDraft, CompanyId, DocumentCheck, Party, PartyDraft, PartyId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::InMemoryCnpjRegistry;

pub const ACTIVE_CNPJ: &str = "11.222.333/0001-81";
pub const CLOSED_ALPHANUMERIC_CNPJ: &str = "12.ABC.345/01DE-35";
/// Valid check digits, but unknown to the contract registry.
pub const UNKNOWN_CNPJ: &str = "00.000.000/0001-91";

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
            .register(company, party_draft(name, document))
            .await
            .unwrap();
    }

    let listed = service.list(company, None, None).await.unwrap().items;
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
        .register(company, party_draft("Padaria", Some(ACTIVE_CNPJ)))
        .await
        .unwrap();
    let listed = service.list(company, None, None).await.unwrap().items;
    assert_eq!(listed[0].document_check(), &DocumentCheck::Unverified);
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
    repository.save(&party).await.unwrap();

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
    repository.update_document_check(&verified).await.unwrap();
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
        repository.update_document_check(&intruder).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.find(company, party.id()).await.unwrap(),
        Some(verified.clone())
    );

    // Going back is allowed too: the latest answer wins, whatever it was.
    let not_found = party.with_document_check(DocumentCheck::NotFound);
    repository.update_document_check(&not_found).await.unwrap();
    assert_eq!(
        repository.find(company, not_found.id()).await.unwrap(),
        Some(not_found)
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
        service.register(company, draft).await.unwrap();
    }

    let mut pages = Vec::new();
    let mut cursor = None;
    loop {
        let page = service
            .list(company, cursor.as_ref(), Some(2))
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
