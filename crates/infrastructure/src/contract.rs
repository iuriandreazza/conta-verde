//! Behavior every adapter must share, written once and run against each of them.

use conta_verde_application::{
    ApplicationError, AuthError, AuthService, BankAccountQuery, BankAccountRepository,
    BankAccountService, CategoryRepository, CategoryService, CnpjRegistry, CostCenterQuery,
    CostCenterRepository, CostCenterService, IdentityRepository, NameSort, NewCategory, PartyKind,
    PartyQuery, PartyRepository, PartyService, PaymentMethodQuery, PaymentMethodRepository,
    PaymentMethodService, ProductQuery, ProductRepository, ProductService, RepositoryError,
    SecretService,
};
use conta_verde_application::{
    ImportRow, InvitationRepository, InvitationState, Invited, MemberService, NewAccount,
    NewInvitation, NewUser, PaymentTermQuery, PaymentTermRepository, PaymentTermService,
};
use conta_verde_domain::{
    Account, AccountDraft, Actor, AddressDraft, AuditAction, AuditEntity, AuditEntry, BankAccount,
    BankAccountDraft, BankAccountId, BankAccountKind, CashFlowGroup, Category, CategoryId,
    CompanyId, Contact, ContactDraft, CostCenter, CostCenterDraft, CostCenterId, DocumentCheck,
    DomainError, DreGroup, FieldChange, Invitation, InvitationId, Party, PartyDraft, PartyId,
    PaymentMethod, PaymentMethodDraft, PaymentMethodId, PaymentMethodKind, PaymentTerm,
    PaymentTermDraft, PaymentTermId, Product, ProductDraft, ProductId, ProductKind, Role, Tag,
    User, UserId, bank_account_changes, bank_account_creation_changes, category_changes,
    category_creation_changes, cost_center_changes, cost_center_creation_changes, creation_changes,
    document_check_changes, payment_method_changes, payment_method_creation_changes,
    product_changes, product_creation_changes,
};
use conta_verde_domain::{InstallmentDraft, payment_term_changes, payment_term_creation_changes};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{InMemoryCnpjRegistry, InMemoryEmailSender};

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
        ..PartyDraft::default()
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
    repository.update(&verified, &rechecked).await.unwrap();

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
            .update(&wrong_company, &check_entry(&first, &wrong_company))
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
        repository.update(&rechecked, &for_another_company).await,
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
        .update(&verified, &check_entry(&party, &verified))
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
            .update(&intruder, &check_entry(&party, &intruder))
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
        .update(&not_found, &check_entry(&verified, &not_found))
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
            ..PartyDraft::default()
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
            ..PartyDraft::default()
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

fn address_draft(postal_code: &str, street: &str) -> AddressDraft {
    AddressDraft {
        postal_code: postal_code.into(),
        street: street.into(),
        city: "São Paulo".into(),
        state: "SP".into(),
        ..AddressDraft::default()
    }
}

async fn party_trail<R: PartyRepository, C: CnpjRegistry>(
    service: &PartyService<R, C>,
    company: CompanyId,
    id: PartyId,
) -> Vec<AuditEntry> {
    let history = service.history(company, id).await.unwrap();
    history.into_iter().map(|record| record.entry).collect()
}

fn changed_fields(entry: &AuditEntry) -> Vec<&str> {
    entry
        .changes
        .iter()
        .map(|change| change.field.as_str())
        .collect()
}

/// A party is replaced as a whole and only what differs is recorded; the registry is asked again
/// only for a new CNPJ; a refused edit leaves the party and its trail as they were.
pub async fn edits_parties_and_records_only_changes<R: PartyRepository>(
    service: &PartyService<R, InMemoryCnpjRegistry>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let ana = service
        .register(
            company,
            &actor(),
            party_draft("Ana", Some("529.982.247-25")),
        )
        .await
        .unwrap();
    let padaria = service
        .register(company, &actor(), party_draft("Padaria", Some(ACTIVE_CNPJ)))
        .await
        .unwrap();
    let edit = |id: PartyId, draft: PartyDraft| async move {
        service.update(company, &actor(), id, draft).await
    };

    // Refused edits leave nothing behind.
    let duplicate = edit(ana.id(), party_draft("Ana", Some(ACTIVE_CNPJ))).await;
    assert_eq!(
        duplicate.unwrap_err(),
        ApplicationError::Repository(RepositoryError::DuplicateDocument)
    );
    let no_role = edit(
        ana.id(),
        PartyDraft {
            is_customer: false,
            ..party_draft("Ana", Some("529.982.247-25"))
        },
    )
    .await;
    assert_eq!(
        no_role.unwrap_err(),
        ApplicationError::Domain(DomainError::MissingRole)
    );
    let bad_address = edit(
        ana.id(),
        PartyDraft {
            address: Some(address_draft("123", "Rua A")),
            ..party_draft("Ana", Some("529.982.247-25"))
        },
    )
    .await;
    assert_eq!(
        bad_address.unwrap_err(),
        ApplicationError::Domain(DomainError::InvalidPostalCode)
    );
    assert_eq!(party_trail(service, company, ana.id()).await.len(), 1);
    let stored = service
        .list(company, &PartyQuery::default(), None, None)
        .await
        .unwrap()
        .items;
    assert_eq!(stored[0].document().unwrap().as_str(), "52998224725");

    // Another company's party, and one that does not exist, are not found.
    let foreign = service
        .update(other_company, &actor(), ana.id(), party_draft("Ana", None))
        .await;
    assert_eq!(foreign.unwrap_err(), ApplicationError::PartyNotFound);
    let unknown = edit(PartyId::generate(), party_draft("Ana", None)).await;
    assert_eq!(unknown.unwrap_err(), ApplicationError::PartyNotFound);

    // A change of name, role and address keeps the registry answer: the document is the same.
    let carrier_draft = PartyDraft {
        is_carrier: true,
        address: Some(address_draft("01310-100", "Avenida Paulista")),
        ..party_draft("Padaria do Centro", Some(ACTIVE_CNPJ))
    };
    let edited = edit(padaria.id(), carrier_draft.clone()).await.unwrap();
    assert_eq!(edited.name(), "Padaria do Centro");
    assert_eq!(edited.address().unwrap().postal_code(), "01310100");
    assert!(matches!(
        edited.document_check(),
        DocumentCheck::Verified { .. }
    ));
    let trail = party_trail(service, company, padaria.id()).await;
    assert_eq!(trail.len(), 2);
    assert_eq!(trail[1].action, AuditAction::Updated);
    assert_eq!(
        changed_fields(&trail[1]),
        [
            "name",
            "is_carrier",
            "postal_code",
            "street",
            "city",
            "state"
        ]
    );
    assert_eq!(trail[1].changes[0].from.as_deref(), Some("Padaria"));
    assert_eq!(trail[1].changes[0].to.as_deref(), Some("Padaria do Centro"));
    let carriers = service
        .list(
            company,
            &PartyQuery::default().of_kind(PartyKind::Carrier),
            None,
            None,
        )
        .await
        .unwrap()
        .items;
    assert_eq!(carriers.len(), 1);
    assert_eq!(carriers[0].address(), edited.address());

    // The same edit again changes nothing, so it records nothing.
    edit(padaria.id(), carrier_draft.clone()).await.unwrap();
    assert_eq!(party_trail(service, company, padaria.id()).await.len(), 2);

    // A new CNPJ is asked about; its answer replaces the old one, and the trail says so.
    let closed = edit(
        padaria.id(),
        PartyDraft {
            document: Some(CLOSED_ALPHANUMERIC_CNPJ.into()),
            ..carrier_draft.clone()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        closed.document_check(),
        &DocumentCheck::Verified {
            legal_name: "OFICINA ALFANUMERICA LTDA".into(),
            situation: "BAIXADA".into(),
        }
    );
    let trail = party_trail(service, company, padaria.id()).await;
    assert_eq!(
        changed_fields(&trail[2]),
        ["document", "registry_legal_name", "registry_situation"]
    );
    let unknown_cnpj = edit(
        padaria.id(),
        PartyDraft {
            document: Some(UNKNOWN_CNPJ.into()),
            ..carrier_draft.clone()
        },
    )
    .await
    .unwrap();
    assert_eq!(unknown_cnpj.document_check(), &DocumentCheck::NotFound);

    // Without a document there is nothing left to say about it.
    let without = edit(
        padaria.id(),
        PartyDraft {
            document: None,
            ..carrier_draft
        },
    )
    .await
    .unwrap();
    assert_eq!(without.document_check(), &DocumentCheck::NotApplicable);
    let trail = party_trail(service, company, padaria.id()).await;
    assert_eq!(trail.len(), 5);
    assert_eq!(changed_fields(&trail[4]), ["document", "document_check"]);
    assert!(trail[4].changes.iter().all(|change| change.to.is_none()));
}

fn contact_draft(name: &str, phone: Option<&str>) -> ContactDraft {
    ContactDraft {
        name: name.into(),
        job_title: Some("Compras".into()),
        email: Some(format!("{}@example.com", name.to_lowercase())),
        phone: phone.map(Into::into),
    }
}

fn party_with(
    name: &str,
    document: Option<&str>,
    tags: &[&str],
    contacts: Vec<ContactDraft>,
) -> PartyDraft {
    PartyDraft {
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        contacts,
        ..party_draft(name, document)
    }
}

fn tag_names(party: &Party) -> Vec<&str> {
    party.tags().iter().map(Tag::as_str).collect()
}

fn contact_names(party: &Party) -> Vec<&str> {
    party.contacts().iter().map(Contact::name).collect()
}

fn party_names(parties: &[Party]) -> Vec<&str> {
    parties.iter().map(Party::name).collect()
}

/// Contacts and tags are stored with the party, come back in the order they were given, are found
/// by tag within the company only, and are replaced as a whole; a refused write leaves the old
/// ones and the trail as they were.
pub async fn keeps_party_contacts_and_tags<R: PartyRepository>(
    service: &PartyService<R, InMemoryCnpjRegistry>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let register = |company: CompanyId, draft: PartyDraft| async move {
        service.register(company, &actor(), draft).await.unwrap()
    };
    let ana = register(
        company,
        party_with(
            "Ana",
            None,
            &["VIP", "atacado"],
            vec![
                contact_draft("Bruno", Some("(81) 99999-0000")),
                contact_draft("Carla", None),
            ],
        ),
    )
    .await;
    register(company, party_with("Dani", None, &["vip"], vec![])).await;
    let eva = register(company, party_with("Eva", None, &[], vec![])).await;
    register(other_company, party_with("Fred", None, &["vip"], vec![])).await;

    let listed = |query: PartyQuery| async move {
        service
            .list(company, &query, None, None)
            .await
            .unwrap()
            .items
    };
    let tagged = |tag: &str| PartyQuery::default().tagged(Tag::parse(tag).unwrap());
    let everyone = listed(PartyQuery::default()).await;
    assert_eq!(party_names(&everyone), ["Ana", "Dani", "Eva"]);
    // Tags read alphabetically, contacts in the order they were given, each with what it holds.
    assert_eq!(tag_names(&everyone[0]), ["atacado", "vip"]);
    assert_eq!(contact_names(&everyone[0]), ["Bruno", "Carla"]);
    let bruno = &everyone[0].contacts()[0];
    assert_eq!(bruno.job_title(), Some("Compras"));
    assert_eq!(bruno.email().unwrap().as_str(), "bruno@example.com");
    assert_eq!(bruno.phone(), Some("(81) 99999-0000"));
    assert_eq!(everyone[0].contacts()[1].phone(), None);
    assert!(everyone[2].contacts().is_empty() && everyone[2].tags().is_empty());

    // The filter finds a tag however it is written, in this company only, and pages like any list.
    assert_eq!(party_names(&listed(tagged("VIP")).await), ["Ana", "Dani"]);
    assert_eq!(party_names(&listed(tagged("atacado")).await), ["Ana"]);
    assert!(listed(tagged("novo")).await.is_empty());
    let first = service
        .list(company, &tagged("vip"), None, Some(1))
        .await
        .unwrap();
    assert_eq!(party_names(&first.items), ["Ana"]);
    let second = service
        .list(company, &tagged("vip"), first.next.as_ref(), Some(1))
        .await
        .unwrap();
    assert_eq!(party_names(&second.items), ["Dani"]);
    assert!(second.next.is_none());

    // A refused edit leaves the contacts, the tags and the trail as they were.
    let gil = register(company, party_draft("Gil", Some("529.982.247-25"))).await;
    let refused = service
        .update(
            company,
            &actor(),
            eva.id(),
            party_with(
                "Eva",
                Some("529.982.247-25"),
                &["novo"],
                vec![contact_draft("Hugo", None)],
            ),
        )
        .await;
    assert_eq!(
        refused.unwrap_err(),
        ApplicationError::Repository(RepositoryError::DuplicateDocument)
    );
    let eva_now = listed(PartyQuery::default().searching("Eva")).await;
    assert!(eva_now[0].contacts().is_empty() && eva_now[0].tags().is_empty());
    assert!(listed(tagged("novo")).await.is_empty());
    assert_eq!(party_trail(service, company, eva.id()).await.len(), 1);
    assert_eq!(party_trail(service, company, gil.id()).await.len(), 1);

    // An edit replaces both lists as a whole, and the trail says what changed.
    let reordered = vec![
        contact_draft("Carla", None),
        contact_draft("Bruno", Some("(81) 99999-0000")),
    ];
    let ana_id = ana.id();
    let edit = |tags: &[&str], contacts: Vec<ContactDraft>| {
        let draft = party_with("Ana", None, tags, contacts);
        async move {
            service
                .update(company, &actor(), ana_id, draft)
                .await
                .unwrap()
        }
    };
    let edited = edit(&["vip", "novo"], reordered.clone()).await;
    assert_eq!(tag_names(&edited), ["novo", "vip"]);
    let after = listed(tagged("vip")).await;
    assert_eq!(party_names(&after), ["Ana", "Dani"]);
    assert_eq!(tag_names(&after[0]), ["novo", "vip"]);
    assert_eq!(contact_names(&after[0]), ["Carla", "Bruno"]);
    assert!(listed(tagged("atacado")).await.is_empty());
    let trail = party_trail(service, company, ana.id()).await;
    assert_eq!(trail.len(), 2);
    assert_eq!(changed_fields(&trail[1]), ["tags", "contacts"]);
    assert_eq!(trail[1].changes[0].from.as_deref(), Some("atacado, vip"));
    assert_eq!(trail[1].changes[0].to.as_deref(), Some("novo, vip"));

    // The same lists written another way change nothing, so nothing is recorded.
    edit(&["NOVO", " vip"], reordered).await;
    assert_eq!(party_trail(service, company, ana.id()).await.len(), 2);

    // Leaving both out clears them.
    let cleared = edit(&[], vec![]).await;
    assert!(cleared.contacts().is_empty() && cleared.tags().is_empty());
    assert!(listed(tagged("novo")).await.is_empty());
    let trail = party_trail(service, company, ana.id()).await;
    assert_eq!(changed_fields(&trail[2]), ["tags", "contacts"]);
    assert!(trail[2].changes.iter().all(|change| change.to.is_none()));
    // Other companies' parties are untouched.
    let fred = service
        .list(other_company, &tagged("vip"), None, None)
        .await
        .unwrap()
        .items;
    assert_eq!(party_names(&fred), ["Fred"]);
}

fn import_rows(rows: Vec<PartyDraft>) -> Vec<ImportRow<PartyDraft>> {
    // The header is row 1, so the first party is row 2, as in a spreadsheet.
    rows.into_iter()
        .enumerate()
        .map(|(index, draft)| ImportRow {
            row: index + 2,
            draft,
        })
        .collect()
}

/// An import checks every row before writing any, refuses a file with a bad or repeated row
/// without leaving a trace, skips what is already registered, and records each creation.
pub async fn imports_parties_and_records_each_creation<R: PartyRepository>(
    service: &PartyService<R, InMemoryCnpjRegistry>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let import = |company: CompanyId, rows: Vec<PartyDraft>, dry_run: bool| async move {
        service
            .import(company, &actor(), import_rows(rows), dry_run)
            .await
            .unwrap()
    };
    let everyone = |company: CompanyId| async move {
        service
            .list(company, &PartyQuery::default(), None, None)
            .await
            .unwrap()
            .items
    };
    let note = |row: usize, message: &str| conta_verde_application::RowNote {
        row,
        message: message.to_owned(),
    };

    // A file with a bad row, a repeated document and a row without a role writes nothing, and the
    // report names every one of them, not just the first.
    let refused = import(
        company,
        vec![
            party_draft("Ana", Some("529.982.247-25")),
            party_draft("Bruno", Some("111.111.111-11")),
            party_draft("Ana de novo", Some("52998224725")),
            PartyDraft {
                is_customer: false,
                ..party_draft("Sem papel", None)
            },
        ],
        false,
    )
    .await;
    assert_eq!((refused.valid, refused.created), (1, 0));
    assert_eq!(
        refused.refused,
        [
            note(3, "document is not a valid CPF or CNPJ"),
            note(4, "the same document as row 2"),
            note(
                5,
                "party must be a customer, a supplier, a carrier or a seller"
            ),
        ]
    );
    assert!(everyone(company).await.is_empty());

    // A dry run says what would happen and writes nothing.
    let rows = || {
        vec![
            party_draft("Ana", Some("529.982.247-25")),
            party_draft("Padaria", Some(ACTIVE_CNPJ)),
            party_draft("Sem documento", None),
        ]
    };
    let dry = import(company, rows(), true).await;
    assert_eq!((dry.valid, dry.created), (3, 0));
    assert!(everyone(company).await.is_empty());

    // The real import writes every row, one audit entry each, and does not ask the registry: the
    // contract registry knows this CNPJ, yet it is stored unverified.
    let done = import(company, rows(), false).await;
    assert_eq!((done.valid, done.created), (3, 3));
    assert!(done.skipped.is_empty() && done.refused.is_empty());
    let stored = everyone(company).await;
    assert_eq!(
        stored.iter().map(Party::name).collect::<Vec<_>>(),
        ["Ana", "Padaria", "Sem documento"]
    );
    assert_eq!(stored[0].document_check(), &DocumentCheck::NotApplicable);
    assert_eq!(stored[1].document_check(), &DocumentCheck::Unverified);
    for party in &stored {
        let trail = party_trail(service, company, party.id()).await;
        assert_eq!(trail.len(), 1);
        assert_eq!(trail[0].action, AuditAction::Created);
    }
    // The trail says the CNPJ was left unchecked, not that the registry was asked.
    let cnpj_trail = party_trail(service, company, stored[1].id()).await;
    assert!(cnpj_trail[0].changes.contains(&FieldChange {
        field: "document_check".into(),
        from: None,
        to: Some("unverified".into()),
    }));

    // Sending the file again skips the rows that have a document and cannot recognize the one
    // that has none.
    let again = import(company, rows(), false).await;
    assert_eq!(again.created, 1);
    assert_eq!(
        again.skipped,
        [
            note(2, "a party with this document already exists"),
            note(3, "a party with this document already exists"),
        ]
    );
    assert_eq!(everyone(company).await.len(), 4);

    // Another company's documents are its own.
    let other = import(other_company, rows(), false).await;
    assert_eq!(other.created, 3);
}

fn product_import_rows(rows: Vec<ProductDraft>) -> Vec<ImportRow<ProductDraft>> {
    rows.into_iter()
        .enumerate()
        .map(|(index, draft)| ImportRow {
            row: index + 2,
            draft,
        })
        .collect()
}

/// An import of products behaves like one of parties, with the SKU as the key: every row is checked
/// before any is written, a repeated SKU is refused, a registered one is skipped and not
/// overwritten, and each creation is recorded.
pub async fn imports_products_and_records_each_creation<R: ProductRepository>(
    service: &ProductService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let import = |company: CompanyId, rows: Vec<ProductDraft>, dry_run: bool| async move {
        service
            .import(company, &actor(), product_import_rows(rows), dry_run)
            .await
            .unwrap()
    };
    let everyone = |company: CompanyId| async move {
        service
            .list(company, &ProductQuery::default(), None, None)
            .await
            .unwrap()
            .items
    };
    let note = |row: usize, message: &str| conta_verde_application::RowNote {
        row,
        message: message.to_owned(),
    };
    let draft = |name: &str, sku: Option<&str>| ProductDraft {
        sale_price: Some("12.50".into()),
        ..product_draft(name, ProductKind::Product, sku, true)
    };

    // A bad row and a repeated SKU (written another way) write nothing, and both are reported.
    let refused = import(
        company,
        vec![
            draft("Pão", Some("PAO-1")),
            ProductDraft {
                sale_price: Some("12,5,0".into()),
                ..draft("Bolo", Some("BOLO-1"))
            },
            draft("Pão de novo", Some("pao-1")),
        ],
        false,
    )
    .await;
    assert_eq!((refused.valid, refused.created), (1, 0));
    assert_eq!(
        refused.refused,
        [
            note(
                3,
                "sale price must be a decimal with up to 13 integer and 6 fractional digits, written like 10.50"
            ),
            note(4, "the same SKU as row 2"),
        ]
    );
    assert!(everyone(company).await.is_empty());

    let rows = || {
        vec![
            draft("Pão", Some("PAO-1")),
            draft("Bolo", Some("BOLO-1")),
            draft("Sem código", None),
        ]
    };
    let dry = import(company, rows(), true).await;
    assert_eq!((dry.valid, dry.created), (3, 0));
    assert!(everyone(company).await.is_empty());

    let done = import(company, rows(), false).await;
    assert_eq!((done.valid, done.created), (3, 3));
    let stored = everyone(company).await;
    assert_eq!(
        stored.iter().map(Product::name).collect::<Vec<_>>(),
        ["Bolo", "Pão", "Sem código"]
    );
    for product in &stored {
        let history = service.history(company, product.id()).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].entry.action, AuditAction::Created);
    }

    // Sending it again skips what has a SKU and cannot recognize what has none.
    let again = import(company, rows(), false).await;
    assert_eq!(again.created, 1);
    assert_eq!(
        again.skipped,
        [
            note(2, "a product with this SKU already exists"),
            note(3, "a product with this SKU already exists"),
        ]
    );

    // Another company's SKUs are its own.
    assert_eq!(import(other_company, rows(), false).await.created, 3);
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
            ..PartyDraft::default()
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
    let company = signed_up.profile.memberships[0].company.id();

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
    assert_eq!(profile.memberships.len(), 1);
    assert_eq!(profile.memberships[0].company.name(), "Padaria");
    assert_eq!(profile.memberships[0].role, Role::Owner);

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

/// A user can exist with no company (one who joined through an invitation): found by e-mail like any
/// other, in no company, and the address is still unique, against accounts made the other way too.
pub async fn registers_a_user_with_no_company<I: IdentityRepository>(identities: &I) {
    let unique = Uuid::new_v4();
    let email = format!("sem-empresa-{unique}@example.com");
    let new_user = |email: &str| NewUser {
        user: User::new(UserId::generate(), "Gil", email).unwrap(),
        password_hash: "not a real hash".into(),
    };
    let gil = new_user(&email);
    identities.register_user(&gil).await.unwrap();
    let found = identities
        .find_credentials(gil.user.email())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.user, gil.user);
    assert!(
        identities
            .memberships_of(gil.user.id())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        identities
            .register_user(&new_user(&email))
            .await
            .unwrap_err(),
        RepositoryError::DuplicateEmail
    );

    // The same address cannot start a company either, and an address that started one cannot join.
    let account = Account::new(
        UserId::generate(),
        CompanyId::generate(),
        AccountDraft {
            user_name: "Gil".into(),
            email: email.clone(),
            company_name: "Padaria".into(),
        },
    )
    .unwrap();
    assert_eq!(
        identities
            .register_account(&NewAccount {
                account,
                password_hash: "x".into(),
            })
            .await
            .unwrap_err(),
        RepositoryError::DuplicateEmail
    );
}

/// A membership says what its user is in the company: the creator is the owner, another user can be
/// added with a role of their own, a second membership never changes the first one's role, and what
/// a user is differs from company to company.
pub async fn memberships_carry_a_role<I: IdentityRepository>(identities: &I) {
    let register = |name: &str, company: &str| {
        let account = Account::new(
            UserId::generate(),
            CompanyId::generate(),
            AccountDraft {
                user_name: name.into(),
                email: format!("{name}-{}@example.com", Uuid::new_v4()),
                company_name: company.into(),
            },
        )
        .unwrap();
        async move {
            identities
                .register_account(&NewAccount {
                    account: account.clone(),
                    password_hash: "not a real hash".into(),
                })
                .await
                .unwrap();
            account
        }
    };
    let ana = register("Ana", "Padaria Ana").await;
    let bia = register("Bia", "Oficina Bia").await;
    let (ana_company, bia_company) = (ana.company.id(), bia.company.id());

    assert_eq!(
        identities
            .role_in(ana.user.id(), ana_company)
            .await
            .unwrap(),
        Some(Role::Owner)
    );
    // Not a member of the other company: no role, which is not the same as the lowest one.
    assert_eq!(
        identities
            .role_in(ana.user.id(), bia_company)
            .await
            .unwrap(),
        None
    );

    identities
        .add_membership(bia.user.id(), ana_company, Role::Sales)
        .await
        .unwrap();
    assert_eq!(
        identities
            .role_in(bia.user.id(), ana_company)
            .await
            .unwrap(),
        Some(Role::Sales)
    );
    // The second attempt, with another role, is refused and changes nothing.
    assert_eq!(
        identities
            .add_membership(bia.user.id(), ana_company, Role::Admin)
            .await
            .unwrap_err(),
        RepositoryError::DuplicateMembership
    );
    assert_eq!(
        identities
            .role_in(bia.user.id(), ana_company)
            .await
            .unwrap(),
        Some(Role::Sales)
    );

    // What a user is depends on the company, and the list says it, ordered by company name.
    let memberships = identities.memberships_of(bia.user.id()).await.unwrap();
    let found: Vec<(&str, Role)> = memberships
        .iter()
        .map(|m| (m.company.name(), m.role))
        .collect();
    assert_eq!(
        found,
        [("Oficina Bia", Role::Owner), ("Padaria Ana", Role::Sales)]
    );

    // A role changes in one company only, and a stranger has none to change.
    identities
        .set_role(bia.user.id(), ana_company, Role::Financial)
        .await
        .unwrap();
    assert_eq!(
        identities
            .role_in(bia.user.id(), ana_company)
            .await
            .unwrap(),
        Some(Role::Financial)
    );
    assert_eq!(
        identities
            .role_in(bia.user.id(), bia_company)
            .await
            .unwrap(),
        Some(Role::Owner)
    );
    assert_eq!(
        identities
            .set_role(ana.user.id(), bia_company, Role::Viewer)
            .await
            .unwrap_err(),
        RepositoryError::NotFound
    );
    // The owner's membership is never touched, by either write, whatever the caller checked.
    assert_eq!(
        identities
            .set_role(ana.user.id(), ana_company, Role::Viewer)
            .await
            .unwrap_err(),
        RepositoryError::NotFound
    );
    assert_eq!(
        identities
            .remove_membership(ana.user.id(), ana_company)
            .await
            .unwrap_err(),
        RepositoryError::NotFound
    );
    assert_eq!(
        identities
            .role_in(ana.user.id(), ana_company)
            .await
            .unwrap(),
        Some(Role::Owner)
    );

    // Removing ends that membership only, and the person can be added again.
    identities
        .remove_membership(bia.user.id(), ana_company)
        .await
        .unwrap();
    assert_eq!(
        identities
            .role_in(bia.user.id(), ana_company)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        identities
            .role_in(bia.user.id(), bia_company)
            .await
            .unwrap(),
        Some(Role::Owner)
    );
    assert_eq!(
        identities
            .remove_membership(bia.user.id(), ana_company)
            .await
            .unwrap_err(),
        RepositoryError::NotFound
    );
    identities
        .add_membership(bia.user.id(), ana_company, Role::Viewer)
        .await
        .unwrap();
}

/// Inviting, accepting and withdrawing: who may be invited, that a link works once, for the address it
/// was sent to, until it is withdrawn or expires, and that an expired one makes way for a new one.
/// `short_lived` is a service whose invitations last a few milliseconds.
pub async fn runs_the_invitation_lifecycle<I, M, S>(
    auth: &AuthService<M, S>,
    service: &MemberService<I, M, InMemoryEmailSender, S>,
    short_lived: &MemberService<I, M, InMemoryEmailSender, S>,
    emails: &InMemoryEmailSender,
) where
    I: InvitationRepository,
    M: IdentityRepository,
    S: SecretService,
{
    let register = |name: &str, email: &str| {
        let draft = AccountDraft {
            user_name: name.into(),
            email: email.into(),
            company_name: format!("Empresa de {name}"),
        };
        async move { auth.register(draft, "correct horse").await.unwrap().profile }
    };
    let unique = Uuid::new_v4();
    let ana = register("Ana", &format!("ana-{unique}@example.com")).await;
    let bia_email = format!("bia-{unique}@example.com");
    let bia = register("Bia", &bia_email).await;
    let cris = register("Cris", &format!("cris-{unique}@example.com")).await;
    let company = ana.memberships[0].company.id();
    let ana_actor = Actor::User {
        id: ana.user.id(),
        name: "Ana".into(),
    };
    let token_of = |link: &str| link.rsplit('/').next().unwrap().to_owned();

    let members = service.members(company).await.unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(
        (members[0].user.name(), members[0].role),
        ("Ana", Role::Owner)
    );

    // Nobody is invited as an owner, with a malformed address, or when already a member.
    let refused = |result: Result<Invited, ApplicationError>| result.unwrap_err();
    assert_eq!(
        refused(
            service
                .invite(company, &ana_actor, &bia_email, Role::Owner)
                .await
        ),
        ApplicationError::Domain(DomainError::OwnerCannotBeInvited)
    );
    assert_eq!(
        refused(
            service
                .invite(company, &ana_actor, "not-an-email", Role::Sales)
                .await
        ),
        ApplicationError::Domain(DomainError::InvalidEmail)
    );
    assert_eq!(
        refused(
            service
                .invite(company, &ana_actor, ana.user.email().as_str(), Role::Sales)
                .await
        ),
        ApplicationError::Repository(RepositoryError::DuplicateMembership)
    );
    assert!(emails.sent().is_empty());

    // A valid invitation is stored, e-mailed with its link, and listed without it.
    let invited = service
        .invite(company, &ana_actor, &bia_email.to_uppercase(), Role::Sales)
        .await
        .unwrap();
    assert!(invited.emailed);
    assert!(invited.link.contains("/invitations/"));
    let sent = emails.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to.as_str(), bia_email);
    assert!(sent[0].body.contains(&invited.link));
    assert!(sent[0].subject.contains("Ana") && sent[0].subject.contains("Empresa de Ana"));
    let open = service.open_invitations(company).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(
        (open[0].email().as_str(), open[0].role()),
        (bia_email.as_str(), Role::Sales)
    );
    assert_eq!(
        refused(
            service
                .invite(company, &ana_actor, &bia_email, Role::Viewer)
                .await
        ),
        ApplicationError::Repository(RepositoryError::DuplicateInvitation)
    );

    // The link says what it leads to, to anyone who holds it; an invented one leads nowhere.
    let token = token_of(&invited.link);
    let preview = service.preview(&token).await.unwrap();
    assert_eq!(
        (
            preview.company_name.as_str(),
            preview.email.as_str(),
            preview.role
        ),
        ("Empresa de Ana", bia_email.as_str(), Role::Sales)
    );
    assert_eq!(
        service.preview("not-a-token").await.unwrap_err(),
        ApplicationError::InvitationNotFound
    );

    // Only the address it was sent to accepts it, and a refusal leaves it open.
    assert_eq!(
        service.accept(&token, &cris.user).await.unwrap_err(),
        ApplicationError::InvitationForAnotherEmail
    );
    assert_eq!(service.open_invitations(company).await.unwrap().len(), 1);
    assert_eq!(service.accept(&token, &bia.user).await.unwrap(), company);
    let members = service.members(company).await.unwrap();
    let found: Vec<(&str, Role)> = members.iter().map(|m| (m.user.name(), m.role)).collect();
    assert_eq!(found, [("Ana", Role::Owner), ("Bia", Role::Sales)]);
    assert!(service.open_invitations(company).await.unwrap().is_empty());
    // Once only.
    assert_eq!(
        service.accept(&token, &bia.user).await.unwrap_err(),
        ApplicationError::InvitationNoLongerValid
    );
    assert_eq!(
        service.preview(&token).await.unwrap_err(),
        ApplicationError::InvitationNoLongerValid
    );
    assert_eq!(
        refused(
            service
                .invite(company, &ana_actor, &bia_email, Role::Viewer)
                .await
        ),
        ApplicationError::Repository(RepositoryError::DuplicateMembership)
    );

    // A withdrawn invitation's link stops working, and only its own company can withdraw it.
    let dani_email = format!("dani-{unique}@example.com");
    let invited = service
        .invite(company, &ana_actor, &dani_email, Role::Viewer)
        .await
        .unwrap();
    let id = service.open_invitations(company).await.unwrap()[0].id();
    assert_eq!(
        service
            .revoke(cris.memberships[0].company.id(), id)
            .await
            .unwrap_err(),
        ApplicationError::InvitationNotFound
    );
    service.revoke(company, id).await.unwrap();
    assert!(service.open_invitations(company).await.unwrap().is_empty());
    assert_eq!(
        service.preview(&token_of(&invited.link)).await.unwrap_err(),
        ApplicationError::InvitationNoLongerValid
    );
    assert_eq!(
        service.revoke(company, id).await.unwrap_err(),
        ApplicationError::InvitationNotFound
    );
    // ... and it can be sent again.
    service
        .invite(company, &ana_actor, &dani_email, Role::Viewer)
        .await
        .unwrap();

    // An expired invitation is no longer valid, is still listed to the person who sent it, and
    // makes way for a new one to the same address.
    let eva_email = format!("eva-{unique}@example.com");
    let short = short_lived
        .invite(company, &ana_actor, &eva_email, Role::Stock)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        short_lived
            .preview(&token_of(&short.link))
            .await
            .unwrap_err(),
        ApplicationError::InvitationNoLongerValid
    );
    assert!(
        service
            .open_invitations(company)
            .await
            .unwrap()
            .iter()
            .any(|invitation| invitation.email().as_str() == eva_email)
    );
    let again = service
        .invite(company, &ana_actor, &eva_email, Role::Stock)
        .await
        .unwrap();
    assert_eq!(
        service
            .preview(&token_of(&again.link))
            .await
            .unwrap()
            .email
            .as_str(),
        eva_email
    );
    let evas = service
        .open_invitations(company)
        .await
        .unwrap()
        .into_iter()
        .filter(|invitation| invitation.email().as_str() == eva_email)
        .count();
    assert_eq!(evas, 1);

    // Someone with no account signs up through the link: an account with no company of its own,
    // a member of the invited one in its role. Everything is checked before anything is stored.
    let gil_email = format!("gil-{unique}@example.com");
    let gil = service
        .invite(company, &ana_actor, &gil_email, Role::Stock)
        .await
        .unwrap();
    let gil_token = token_of(&gil.link);
    assert_eq!(
        service
            .join(
                &gil_token,
                "Gil",
                &format!("outro-{unique}@example.com"),
                "correct horse"
            )
            .await
            .unwrap_err(),
        ApplicationError::InvitationForAnotherEmail
    );
    assert_eq!(
        service
            .join(&gil_token, "Gil", &gil_email, "short")
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::WeakPassword)
    );
    assert_eq!(
        service
            .join("not-a-token", "Gil", &gil_email, "correct horse")
            .await
            .unwrap_err(),
        ApplicationError::InvitationNotFound
    );
    // Nothing above made an account.
    assert_eq!(
        auth.login(&gil_email, "correct horse").await.unwrap_err(),
        ApplicationError::Auth(AuthError::InvalidCredentials)
    );
    let (gil_user, joined) = service
        .join(
            &gil_token,
            "Gil",
            &gil_email.to_uppercase(),
            "correct horse",
        )
        .await
        .unwrap();
    assert_eq!(
        (joined, gil_user.email().as_str()),
        (company, gil_email.as_str())
    );
    let profile = auth
        .login(&gil_email, "correct horse")
        .await
        .unwrap()
        .profile;
    assert_eq!(profile.memberships.len(), 1, "no company of its own");
    assert_eq!(
        (
            profile.memberships[0].company.id(),
            profile.memberships[0].role
        ),
        (company, Role::Stock)
    );
    // Once only.
    assert_eq!(
        service
            .join(&gil_token, "Gil", &gil_email, "correct horse")
            .await
            .unwrap_err(),
        ApplicationError::InvitationNoLongerValid
    );
    // An address that already has an account is told so, and the invitation stays open.
    let cris_invitation = service
        .invite(
            company,
            &ana_actor,
            cris.user.email().as_str(),
            Role::Viewer,
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .join(
                &token_of(&cris_invitation.link),
                "Cris",
                cris.user.email().as_str(),
                "correct horse"
            )
            .await
            .unwrap_err(),
        ApplicationError::Repository(RepositoryError::DuplicateEmail)
    );
    assert!(
        service
            .preview(&token_of(&cris_invitation.link))
            .await
            .is_ok()
    );
}

/// Claiming an invitation is one step that takes it once, not after it expired or was withdrawn, and
/// that can be given back when what it was for could not be done.
pub async fn claims_an_invitation_once_and_only_while_it_lasts<I, M>(
    invitations: &I,
    identities: &M,
) where
    I: InvitationRepository,
    M: IdentityRepository,
{
    let unique = Uuid::new_v4();
    let ana = Account::new(
        UserId::generate(),
        CompanyId::generate(),
        AccountDraft {
            user_name: "Ana".into(),
            email: format!("ana-{unique}@example.com"),
            company_name: "Padaria".into(),
        },
    )
    .unwrap();
    identities
        .register_account(&NewAccount {
            account: ana.clone(),
            password_hash: "not a real hash".into(),
        })
        .await
        .unwrap();
    let now = 1_800_000_000_000_i64;
    let digest = |label: &str| format!("{label}-{unique}");
    let invite = |email: &str| {
        let invitation = Invitation::new(
            InvitationId::generate(),
            ana.company.id(),
            &format!("{email}-{unique}@example.com"),
            Role::Sales,
            ana.user.id(),
            now,
            60_000,
        )
        .unwrap();
        (invitation.clone(), digest(email))
    };
    let state_of = |digest: String| async move {
        invitations
            .find_by_token_digest(&digest)
            .await
            .unwrap()
            .unwrap()
            .state
    };

    // Taken once; a second claim, and a release of one that was never claimed, find nothing.
    let (first, first_digest) = invite("bia");
    invitations
        .create(
            &NewInvitation {
                invitation: first.clone(),
                token_digest: first_digest.clone(),
            },
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        invitations.release(first.id()).await.unwrap_err(),
        RepositoryError::NotFound
    );
    invitations.claim(first.id(), now + 1_000).await.unwrap();
    assert_eq!(
        state_of(first_digest.clone()).await,
        InvitationState::Accepted
    );
    assert_eq!(
        invitations
            .claim(first.id(), now + 1_000)
            .await
            .unwrap_err(),
        RepositoryError::NotFound
    );
    // Given back, it can be taken again.
    invitations.release(first.id()).await.unwrap();
    assert_eq!(state_of(first_digest.clone()).await, InvitationState::Open);
    invitations.claim(first.id(), now + 1_000).await.unwrap();

    // Not after it expired, and it stays open for whoever lists it.
    let (late, late_digest) = invite("cris");
    invitations
        .create(
            &NewInvitation {
                invitation: late.clone(),
                token_digest: late_digest.clone(),
            },
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        invitations
            .claim(late.id(), now + 60_000)
            .await
            .unwrap_err(),
        RepositoryError::NotFound
    );
    assert_eq!(state_of(late_digest).await, InvitationState::Open);

    // Not after it was withdrawn, and a withdrawn one cannot be given back by a release.
    let (gone, gone_digest) = invite("dani");
    invitations
        .create(
            &NewInvitation {
                invitation: gone.clone(),
                token_digest: gone_digest.clone(),
            },
            now,
        )
        .await
        .unwrap();
    invitations
        .revoke(ana.company.id(), gone.id())
        .await
        .unwrap();
    assert_eq!(
        invitations.claim(gone.id(), now + 1_000).await.unwrap_err(),
        RepositoryError::NotFound
    );
    assert_eq!(
        invitations.release(gone.id()).await.unwrap_err(),
        RepositoryError::NotFound
    );
    assert_eq!(state_of(gone_digest).await, InvitationState::Revoked);
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

// ---- categories

fn category_entry(
    category: &Category,
    action: AuditAction,
    changes: Vec<FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: category.company_id(),
        actor: actor(),
        entity: AuditEntity::Category,
        entity_id: category.id().as_uuid(),
        action,
        changes,
    }
}

fn created(category: &Category) -> (Category, AuditEntry) {
    let entry = category_entry(
        category,
        AuditAction::Created,
        category_creation_changes(category),
    );
    (category.clone(), entry)
}

fn root_category(company: CompanyId, name: &str) -> Category {
    Category::new_root(
        CategoryId::generate(),
        company,
        name,
        DreGroup::GrossRevenue,
        CashFlowGroup::Operating,
    )
    .unwrap()
}

fn child_category(parent: &Category, name: &str) -> Category {
    Category::new_child(CategoryId::generate(), parent, name).unwrap()
}

async fn category_trail<R: CategoryRepository>(
    repository: &R,
    company: CompanyId,
    category: &Category,
) -> usize {
    repository
        .history(company, category.id())
        .await
        .unwrap()
        .len()
}

async fn save_category<R: CategoryRepository>(repository: &R, category: &Category) {
    let (category, entry) = created(category);
    repository.save(&category, &entry).await.unwrap();
}

/// A write is stored with its audit entry, scoped by company; one that is refused or misses leaves
/// nothing in the trail, and an entry that describes something else is refused outright.
pub async fn stores_categories_with_their_trail<R: CategoryRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let revenue = root_category(company, "Receitas");
    let sales = child_category(&revenue, "Vendas");
    save_category(repository, &revenue).await;
    save_category(repository, &sales).await;
    for category in [&revenue, &sales] {
        assert_eq!(
            repository
                .find(company, category.id())
                .await
                .unwrap()
                .as_ref(),
            Some(category)
        );
        assert_eq!(
            repository.find(other_company, category.id()).await.unwrap(),
            None
        );
        assert_eq!(category_trail(repository, company, category).await, 1);
        assert_eq!(category_trail(repository, other_company, category).await, 0);
    }
    let history = repository.history(company, sales.id()).await.unwrap();
    assert_eq!(history[0].entry.action, AuditAction::Created);
    assert_eq!(history[0].entry.changes, category_creation_changes(&sales));

    // Siblings have distinct names whatever their case; the roots are siblings of each other.
    for twin in [
        child_category(&revenue, "VENDAS"),
        root_category(company, "receitas"),
    ] {
        let (twin, entry) = created(&twin);
        assert_eq!(
            repository.save(&twin, &entry).await,
            Err(RepositoryError::DuplicateName)
        );
        assert_eq!(repository.find(company, twin.id()).await.unwrap(), None);
        assert_eq!(category_trail(repository, company, &twin).await, 0);
    }
    // The same name under another parent, or in another company, is fine.
    let expenses = root_category(company, "Despesas");
    save_category(repository, &expenses).await;
    save_category(repository, &child_category(&expenses, "Vendas")).await;
    save_category(repository, &root_category(other_company, "Receitas")).await;

    // A parent has to be in the company.
    let orphan = Category::restore(
        CategoryId::generate(),
        other_company,
        Some(revenue.id()),
        "Filha de outra empresa",
        DreGroup::GrossRevenue,
        CashFlowGroup::Operating,
        2,
        true,
    )
    .unwrap();
    let (orphan, entry) = created(&orphan);
    assert_eq!(
        repository.save(&orphan, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(category_trail(repository, other_company, &orphan).await, 0);

    // A parent has to be one level up and have the groups the child carries.
    for (dre_group, cash_flow_group, level) in [
        (DreGroup::Costs, CashFlowGroup::Operating, 2),
        (DreGroup::GrossRevenue, CashFlowGroup::Financing, 2),
        (DreGroup::GrossRevenue, CashFlowGroup::Operating, 3),
    ] {
        let misfit = Category::restore(
            CategoryId::generate(),
            company,
            Some(revenue.id()),
            "Fora do lugar",
            dre_group,
            cash_flow_group,
            level,
            true,
        )
        .unwrap();
        let (misfit, entry) = created(&misfit);
        assert_eq!(
            repository.save(&misfit, &entry).await,
            Err(RepositoryError::NotFound)
        );
        assert_eq!(category_trail(repository, company, &misfit).await, 0);
    }

    // One category for transfers per company.
    let transfers = Category::new_root(
        CategoryId::generate(),
        company,
        "Transferências",
        DreGroup::Transfers,
        CashFlowGroup::Transfer,
    )
    .unwrap();
    save_category(repository, &transfers).await;
    let second = Category::new_root(
        CategoryId::generate(),
        company,
        "Entre contas",
        DreGroup::Transfers,
        CashFlowGroup::Transfer,
    )
    .unwrap();
    let (second, entry) = created(&second);
    assert_eq!(
        repository.save(&second, &entry).await,
        Err(RepositoryError::DuplicateTransferCategory)
    );
    assert_eq!(category_trail(repository, company, &second).await, 0);

    // An entry about another record, or another company, is refused and nothing is stored.
    let stray = root_category(company, "Estranha");
    let wrong_record = category_entry(&revenue, AuditAction::Created, vec![]);
    assert!(repository.save(&stray, &wrong_record).await.is_err());
    assert_eq!(repository.find(company, stray.id()).await.unwrap(), None);
    let mut wrong_company = category_entry(&stray, AuditAction::Created, vec![]);
    wrong_company.company_id = other_company;
    assert!(repository.save(&stray, &wrong_company).await.is_err());
    assert_eq!(category_trail(repository, other_company, &stray).await, 0);

    // An update changes the name and the flag, adds an entry, and leaves the rest where it was;
    // changing only the case of its own name is not a clash.
    let changed = sales.changed("Vendas e serviços", false).unwrap();
    let entry = category_entry(
        &changed,
        AuditAction::Updated,
        category_changes(&sales, &changed),
    );
    repository.update(&changed, &entry).await.unwrap();
    assert_eq!(
        repository.find(company, sales.id()).await.unwrap(),
        Some(changed.clone())
    );
    assert_eq!(category_trail(repository, company, &sales).await, 2);
    let recased = revenue.changed("RECEITAS", true).unwrap();
    let entry = category_entry(
        &recased,
        AuditAction::Updated,
        category_changes(&revenue, &recased),
    );
    repository.update(&recased, &entry).await.unwrap();

    // One that clashes with a sibling changes nothing and adds nothing.
    let clash = expenses.changed("receitas", true).unwrap();
    let entry = category_entry(
        &clash,
        AuditAction::Updated,
        category_changes(&expenses, &clash),
    );
    assert_eq!(
        repository.update(&clash, &entry).await,
        Err(RepositoryError::DuplicateName)
    );
    assert_eq!(
        repository.find(company, expenses.id()).await.unwrap(),
        Some(expenses.clone())
    );
    assert_eq!(category_trail(repository, company, &expenses).await, 1);

    // Not in that company, or not at all: nothing is updated and no entry is left.
    let foreign = Category::restore(
        revenue.id(),
        other_company,
        None,
        "Roubo",
        DreGroup::GrossRevenue,
        CashFlowGroup::Operating,
        1,
        true,
    )
    .unwrap();
    let entry = category_entry(&foreign, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&foreign, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(category_trail(repository, other_company, &revenue).await, 0);

    // The list is the whole company, in a fixed order, and can be narrowed to the active ones.
    let names = |categories: Vec<Category>| -> Vec<String> {
        categories
            .iter()
            .map(|category| category.name().to_owned())
            .collect()
    };
    let all = repository.list_by_company(company, None).await.unwrap();
    assert_eq!(
        names(all),
        [
            "Despesas",
            "RECEITAS",
            "Transferências",
            "Vendas",
            "Vendas e serviços"
        ]
    );
    let inactive = repository
        .list_by_company(company, Some(false))
        .await
        .unwrap();
    assert_eq!(names(inactive), ["Vendas e serviços"]);
    assert_eq!(
        repository
            .list_by_company(company, Some(true))
            .await
            .unwrap()
            .len(),
        4
    );
}

/// A batch is stored whole, with all its entries, or not at all.
pub async fn saves_a_batch_all_or_nothing<R: CategoryRepository>(
    repository: &R,
    company: CompanyId,
) {
    let revenue = root_category(company, "Receitas");
    let sales = child_category(&revenue, "Vendas");
    let twin = root_category(company, "RECEITAS");
    let batch = vec![created(&revenue), created(&sales), created(&twin)];
    assert_eq!(
        repository.save_all(&batch).await,
        Err(RepositoryError::DuplicateName)
    );
    assert!(
        repository
            .list_by_company(company, None)
            .await
            .unwrap()
            .is_empty()
    );
    for category in [&revenue, &sales, &twin] {
        assert_eq!(category_trail(repository, company, category).await, 0);
    }

    // An entry that describes something else refuses the whole batch too.
    let wrong = (
        sales.clone(),
        category_entry(&revenue, AuditAction::Created, vec![]),
    );
    assert!(
        repository
            .save_all(&[created(&revenue), wrong])
            .await
            .is_err()
    );
    assert!(
        repository
            .list_by_company(company, None)
            .await
            .unwrap()
            .is_empty()
    );

    // Without the clash it is stored, parents first, each with its entry.
    repository
        .save_all(&[created(&revenue), created(&sales)])
        .await
        .unwrap();
    assert_eq!(
        repository
            .list_by_company(company, None)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(category_trail(repository, company, &revenue).await, 1);
    assert_eq!(category_trail(repository, company, &sales).await, 1);
}

pub async fn applies_the_default_template_once<R: CategoryRepository>(
    service: &CategoryService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let applied = service
        .apply_default_template(company, &actor())
        .await
        .unwrap();
    let listed = service.list(company, None).await.unwrap();
    // What the template answers is the list as every other list says it, not the template's own order.
    assert_eq!(applied, listed);
    assert!(applied.len() > 20);
    // Found by its group, never by its name: one category for transfers, a root.
    let transfers: Vec<&Category> = listed
        .iter()
        .filter(|category| category.is_transfer())
        .collect();
    assert_eq!(transfers.len(), 1);
    assert_eq!(transfers[0].cash_flow_group(), CashFlowGroup::Transfer);
    // Every one has its entry, and every subcategory sits under a parent of the same groups.
    for category in &listed {
        let history = service.history(company, category.id()).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].entry.action, AuditAction::Created);
        if let Some(parent) = category
            .parent_id()
            .and_then(|id| listed.iter().find(|c| c.id() == id))
        {
            assert_eq!(category.dre_group(), parent.dre_group());
            assert_eq!(category.level(), parent.level() + 1);
        }
    }
    assert!(service.list(company, Some(false)).await.unwrap().is_empty());

    // Only to a company that has none; another company is not touched and may have its own.
    assert_eq!(
        service.apply_default_template(company, &actor()).await,
        Err(ApplicationError::CategoriesAlreadyExist)
    );
    assert_eq!(
        service.list(company, None).await.unwrap().len(),
        applied.len()
    );
    assert!(service.list(other_company, None).await.unwrap().is_empty());
    service
        .apply_default_template(other_company, &actor())
        .await
        .unwrap();
    assert_eq!(
        service.list(other_company, None).await.unwrap().len(),
        applied.len()
    );
}

fn new_category(
    parent: Option<&Category>,
    name: &str,
    groups: Option<(DreGroup, CashFlowGroup)>,
) -> NewCategory {
    NewCategory {
        parent_id: parent.map(Category::id),
        name: name.into(),
        dre_group: groups.map(|groups| groups.0),
        cash_flow_group: groups.map(|groups| groups.1),
    }
}

/// The rules that keep a tree consistent, as the use case enforces them.
pub async fn keeps_the_tree_consistent<R: CategoryRepository>(
    service: &CategoryService<R>,
    company: CompanyId,
) {
    let who = actor();
    let costs = (DreGroup::OperatingExpenses, CashFlowGroup::Operating);
    let domain = |error: DomainError| Err(ApplicationError::Domain(error));

    // A root says where it lands; a subcategory takes it from its parent and may not say.
    assert_eq!(
        service
            .create(company, &who, new_category(None, "Despesas", None))
            .await,
        domain(DomainError::MissingGroups)
    );
    let expenses = service
        .create(company, &who, new_category(None, "Despesas", Some(costs)))
        .await
        .unwrap();
    assert_eq!(
        service
            .create(
                company,
                &who,
                new_category(Some(&expenses), "Aluguel", Some(costs))
            )
            .await,
        domain(DomainError::GroupsAreInherited)
    );
    let rent = service
        .create(
            company,
            &who,
            new_category(Some(&expenses), "Aluguel", None),
        )
        .await
        .unwrap();
    assert_eq!(rent.dre_group(), DreGroup::OperatingExpenses);
    assert_eq!(rent.level(), 2);
    let unknown_parent = NewCategory {
        parent_id: Some(CategoryId::generate()),
        ..new_category(None, "Filha", None)
    };
    assert_eq!(
        service.create(company, &who, unknown_parent).await,
        Err(ApplicationError::ParentCategoryNotFound)
    );

    // Five levels, no more.
    let mut node = rent;
    for level in 3..=5 {
        node = service
            .create(
                company,
                &who,
                new_category(Some(&node), &format!("Nível {level}"), None),
            )
            .await
            .unwrap();
    }
    assert_eq!(
        service
            .create(company, &who, new_category(Some(&node), "Nível 6", None))
            .await,
        domain(DomainError::CategoryTooDeep)
    );

    // The category for transfers: one, no children, never off, but it can be renamed.
    let transfer_groups = (DreGroup::Transfers, CashFlowGroup::Transfer);
    let transfers = service
        .create(
            company,
            &who,
            new_category(None, "Transferências", Some(transfer_groups)),
        )
        .await
        .unwrap();
    assert!(transfers.is_transfer());
    assert_eq!(
        service
            .create(
                company,
                &who,
                new_category(None, "Entre contas", Some(transfer_groups))
            )
            .await,
        Err(ApplicationError::Repository(
            RepositoryError::DuplicateTransferCategory
        ))
    );
    assert_eq!(
        service
            .create(company, &who, new_category(Some(&transfers), "Filha", None))
            .await,
        domain(DomainError::TransferCategoryHasNoChildren)
    );
    assert_eq!(
        service
            .update(company, &who, transfers.id(), "Transferências", false)
            .await,
        domain(DomainError::TransferCategoryStaysActive)
    );
    let renamed = service
        .update(company, &who, transfers.id(), "Entre contas", true)
        .await
        .unwrap();
    assert!(renamed.is_transfer());

    // An active category is never under an inactive one, whichever way round it is asked for.
    let (parent, leaf) = (
        expenses.clone(),
        service
            .list(company, None)
            .await
            .unwrap()
            .into_iter()
            .find(|c| c.name() == "Aluguel")
            .unwrap(),
    );
    assert_eq!(
        service
            .update(company, &who, parent.id(), "Despesas", false)
            .await,
        Err(ApplicationError::CategoryHasActiveChildren)
    );
    // Its subtree has to be switched off from the bottom up.
    for name in ["Nível 5", "Nível 4", "Nível 3", "Aluguel", "Despesas"] {
        let category = service
            .list(company, None)
            .await
            .unwrap()
            .into_iter()
            .find(|category| category.name() == name)
            .unwrap();
        service
            .update(company, &who, category.id(), name, false)
            .await
            .unwrap();
    }
    assert_eq!(
        service
            .create(
                company,
                &who,
                new_category(Some(&parent), "Marketing", None)
            )
            .await,
        Err(ApplicationError::CategoryParentInactive)
    );
    assert_eq!(
        service
            .update(company, &who, leaf.id(), "Aluguel", true)
            .await,
        Err(ApplicationError::CategoryParentInactive)
    );
    service
        .update(company, &who, parent.id(), "Despesas", true)
        .await
        .unwrap();
    let back = service
        .update(company, &who, leaf.id(), "Aluguel", true)
        .await
        .unwrap();
    // The place in the tree and the groups did not move.
    assert_eq!(back.parent_id(), Some(parent.id()));
    assert_eq!(back.dre_group(), DreGroup::OperatingExpenses);
    assert_eq!(back.level(), 2);

    // A change that changes nothing writes nothing.
    let before = service.history(company, leaf.id()).await.unwrap().len();
    service
        .update(company, &who, leaf.id(), "  Aluguel ", true)
        .await
        .unwrap();
    assert_eq!(
        service.history(company, leaf.id()).await.unwrap().len(),
        before
    );
    let last = service
        .history(company, leaf.id())
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(last.entry.action, AuditAction::Updated);
    assert_eq!(last.entry.changes.len(), 1);
    assert_eq!(last.entry.changes[0].field, "active");

    // Unknown ones answer like another company's.
    let nobody = CategoryId::generate();
    assert_eq!(
        service.update(company, &who, nobody, "X", true).await,
        Err(ApplicationError::CategoryNotFound)
    );
    assert_eq!(
        service.history(company, nobody).await,
        Err(ApplicationError::CategoryNotFound)
    );
}

// ---- bank accounts

fn account_draft(
    kind: BankAccountKind,
    name: &str,
    identity: Option<(&str, &str, &str)>,
    active: bool,
) -> BankAccountDraft {
    BankAccountDraft {
        kind,
        name: name.into(),
        bank_code: identity.map(|identity| identity.0.into()),
        branch: identity.map(|identity| identity.1.into()),
        number: identity.map(|identity| identity.2.into()),
        active,
    }
}

fn new_account(
    company: CompanyId,
    name: &str,
    identity: Option<(&str, &str, &str)>,
) -> BankAccount {
    BankAccount::new(
        BankAccountId::generate(),
        company,
        account_draft(BankAccountKind::Checking, name, identity, true),
    )
    .unwrap()
}

fn account_entry(
    account: &BankAccount,
    action: AuditAction,
    changes: Vec<FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: account.company_id(),
        actor: actor(),
        entity: AuditEntity::BankAccount,
        entity_id: account.id().as_uuid(),
        action,
        changes,
    }
}

async fn save_account<R: BankAccountRepository>(
    repository: &R,
    account: &BankAccount,
) -> Result<(), RepositoryError> {
    let entry = account_entry(
        account,
        AuditAction::Created,
        bank_account_creation_changes(account),
    );
    repository.save(account, &entry).await
}

async fn account_trail<R: BankAccountRepository>(
    repository: &R,
    company: CompanyId,
    account: &BankAccount,
) -> usize {
    repository
        .history(company, account.id())
        .await
        .unwrap()
        .len()
}

/// A write is stored with its audit entry, scoped by company; one that is refused or misses leaves
/// nothing in the trail, and an entry that describes something else is refused outright.
pub async fn stores_bank_accounts_with_their_trail<R: BankAccountRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let main = new_account(company, "Itaú principal", Some(("341", "1234", "56789X")));
    save_account(repository, &main).await.unwrap();
    assert_eq!(
        repository.find(company, main.id()).await.unwrap(),
        Some(main.clone())
    );
    assert_eq!(
        repository.find(other_company, main.id()).await.unwrap(),
        None
    );
    assert_eq!(account_trail(repository, company, &main).await, 1);
    assert_eq!(account_trail(repository, other_company, &main).await, 0);

    // The same name, whatever its case, and the same bank, branch and number, are each refused:
    // no row and no entry.
    let same_name = new_account(company, "ITAÚ PRINCIPAL", None);
    let same_identity = new_account(company, "Outro nome", Some(("341", "1234", "56789X")));
    assert_eq!(
        save_account(repository, &same_name).await,
        Err(RepositoryError::DuplicateName)
    );
    assert_eq!(
        save_account(repository, &same_identity).await,
        Err(RepositoryError::DuplicateBankAccount)
    );
    for refused in [&same_name, &same_identity] {
        assert_eq!(repository.find(company, refused.id()).await.unwrap(), None);
        assert_eq!(account_trail(repository, company, refused).await, 0);
    }
    // The same bank, branch and number under another kind is another account (a savings account
    // may carry the checking account's), and so is a card of the same bank.
    let savings = BankAccount::new(
        BankAccountId::generate(),
        company,
        account_draft(
            BankAccountKind::Savings,
            "Poupança no mesmo banco",
            Some(("341", "1234", "56789X")),
            true,
        ),
    )
    .unwrap();
    save_account(repository, &savings).await.unwrap();
    // Another company may have both; an account whose identity is not complete never clashes.
    save_account(
        repository,
        &new_account(
            other_company,
            "Itaú principal",
            Some(("341", "1234", "56789X")),
        ),
    )
    .await
    .unwrap();
    for name in ["Sem número 1", "Sem número 2"] {
        let partial = BankAccount::new(
            BankAccountId::generate(),
            company,
            BankAccountDraft {
                number: None,
                ..account_draft(
                    BankAccountKind::Checking,
                    name,
                    Some(("341", "1234", "x")),
                    true,
                )
            },
        )
        .unwrap();
        save_account(repository, &partial).await.unwrap();
    }
    let cash = BankAccount::new(
        BankAccountId::generate(),
        company,
        account_draft(BankAccountKind::Cash, "Caixa", None, true),
    )
    .unwrap();
    save_account(repository, &cash).await.unwrap();

    // An entry about another record, or another company, is refused and nothing is stored.
    let stray = new_account(company, "Estranha", None);
    let wrong_record = account_entry(&main, AuditAction::Created, vec![]);
    assert!(repository.save(&stray, &wrong_record).await.is_err());
    assert_eq!(repository.find(company, stray.id()).await.unwrap(), None);
    let mut wrong_company = account_entry(&stray, AuditAction::Created, vec![]);
    wrong_company.company_id = other_company;
    assert!(repository.save(&stray, &wrong_company).await.is_err());
    assert_eq!(account_trail(repository, other_company, &stray).await, 0);

    // An update replaces the fields and adds an entry; changing only the case of its own name, or
    // keeping its own bank, branch and number, is not a clash.
    let changed = main
        .replaced_with(account_draft(
            BankAccountKind::Checking,
            "ITAÚ PRINCIPAL",
            Some(("341", "1234", "56789X")),
            false,
        ))
        .unwrap();
    let entry = account_entry(
        &changed,
        AuditAction::Updated,
        bank_account_changes(&main, &changed),
    );
    repository.update(&changed, &entry).await.unwrap();
    assert_eq!(
        repository.find(company, main.id()).await.unwrap(),
        Some(changed.clone())
    );
    assert_eq!(account_trail(repository, company, &main).await, 2);

    // One that clashes changes nothing and adds nothing: with another account's name, or with
    // another account's bank, branch and number.
    let other = new_account(company, "Poupança", Some(("104", "0001", "777")));
    save_account(repository, &other).await.unwrap();
    for draft in [
        account_draft(
            BankAccountKind::Checking,
            "itaú principal",
            Some(("104", "0001", "777")),
            true,
        ),
        account_draft(
            BankAccountKind::Checking,
            "Poupança",
            Some(("341", "1234", "56789X")),
            true,
        ),
    ] {
        let clashing = other.replaced_with(draft).unwrap();
        let entry = account_entry(
            &clashing,
            AuditAction::Updated,
            bank_account_changes(&other, &clashing),
        );
        assert!(matches!(
            repository.update(&clashing, &entry).await,
            Err(RepositoryError::DuplicateName | RepositoryError::DuplicateBankAccount)
        ));
        assert_eq!(
            repository.find(company, other.id()).await.unwrap(),
            Some(other.clone())
        );
        // Only the entry of its creation.
        assert_eq!(account_trail(repository, company, &other).await, 1);
    }

    // The kind is fixed at registration: the port refuses to change it and writes nothing.
    let kind_swap = BankAccount::new(
        other.id(),
        company,
        account_draft(
            BankAccountKind::Savings,
            "Poupança",
            Some(("104", "0001", "777")),
            true,
        ),
    )
    .unwrap();
    let entry = account_entry(&kind_swap, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&kind_swap, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository
            .find(company, other.id())
            .await
            .unwrap()
            .unwrap()
            .kind(),
        BankAccountKind::Checking
    );

    // A number given up by an edit is free for another account of the company.
    let moved = other
        .replaced_with(account_draft(
            BankAccountKind::Checking,
            "Poupança",
            Some(("104", "0001", "888")),
            true,
        ))
        .unwrap();
    let entry = account_entry(
        &moved,
        AuditAction::Updated,
        bank_account_changes(&other, &moved),
    );
    repository.update(&moved, &entry).await.unwrap();
    save_account(
        repository,
        &new_account(company, "Terceira", Some(("104", "0001", "777"))),
    )
    .await
    .unwrap();

    // Not in that company, or not at all: nothing is updated and no entry is left.
    let foreign = BankAccount::new(
        main.id(),
        other_company,
        account_draft(BankAccountKind::Checking, "Roubo", None, true),
    )
    .unwrap();
    let entry = account_entry(&foreign, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&foreign, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(account_trail(repository, other_company, &main).await, 0);
    let unknown = new_account(company, "Fantasma", None);
    let entry = account_entry(&unknown, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&unknown, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(account_trail(repository, company, &unknown).await, 0);
}

/// An edit that changes nothing writes nothing, and the trail shows who changed what.
pub async fn edits_bank_accounts_and_records_only_changes<R: BankAccountRepository>(
    service: &BankAccountService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let who = actor();
    // Written the way people write it, stored in one form.
    let created = service
        .register(
            company,
            &who,
            account_draft(
                BankAccountKind::Checking,
                "  Conta da loja ",
                Some(("341", " 12.34 ", "56789-x")),
                true,
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        (created.name(), created.branch(), created.number()),
        ("Conta da loja", Some("1234"), Some("56789X"))
    );

    // The same account written differently is no change.
    service
        .update(
            company,
            &who,
            created.id(),
            account_draft(
                BankAccountKind::Checking,
                "Conta da loja",
                Some(("341", "1234", "56789X")),
                true,
            ),
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
            &who,
            created.id(),
            account_draft(
                BankAccountKind::Checking,
                "Conta da loja",
                Some(("341", "1234", "99999")),
                false,
            ),
        )
        .await
        .unwrap();
    assert_eq!(updated.number(), Some("99999"));
    let history = service.history(company, created.id()).await.unwrap();
    assert_eq!(history.len(), 2);
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
            ("number", Some("56789X"), Some("99999")),
            ("active", Some("true"), Some("false"))
        ]
    );

    // A kind is for good, and other companies can neither touch nor read the account.
    assert_eq!(
        service
            .update(
                company,
                &who,
                created.id(),
                account_draft(BankAccountKind::Savings, "Conta da loja", None, true)
            )
            .await,
        Err(ApplicationError::Domain(DomainError::KindCannotChange))
    );
    assert_eq!(
        service
            .update(
                other_company,
                &who,
                created.id(),
                account_draft(BankAccountKind::Checking, "X", None, true)
            )
            .await,
        Err(ApplicationError::BankAccountNotFound)
    );
    assert_eq!(
        service.history(other_company, created.id()).await,
        Err(ApplicationError::BankAccountNotFound)
    );
    assert_eq!(
        service
            .register(
                company,
                &who,
                account_draft(
                    BankAccountKind::Checking,
                    "Outra",
                    Some(("341", "1234", "99999")),
                    true
                )
            )
            .await,
        Err(ApplicationError::Repository(
            RepositoryError::DuplicateBankAccount
        ))
    );
}

async fn listed_account_names<R: BankAccountRepository>(
    service: &BankAccountService<R>,
    company: CompanyId,
    query: &BankAccountQuery,
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

pub async fn filters_and_sorts_bank_accounts<R: BankAccountRepository>(
    service: &BankAccountService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let who = actor();
    use BankAccountKind::{Card, Cash, Checking, Savings};
    for (kind, name, identity, active) in [
        (Checking, "Alfa", Some(("341", "1", "1001")), true),
        (Savings, "bravo", Some(("001", "2", "2002")), false),
        (Cash, "Caixa 100%", None, true),
        (Card, "Delta_x", None, true),
        (Checking, "echo", None, false),
    ] {
        service
            .register(company, &who, account_draft(kind, name, identity, active))
            .await
            .unwrap();
    }
    // Another company's account never shows up, and may reuse a name and a number.
    service
        .register(
            other_company,
            &who,
            account_draft(Checking, "Alfa", Some(("341", "1", "1001")), true),
        )
        .await
        .unwrap();

    let all = BankAccountQuery::default();
    let names = |query: BankAccountQuery| async move {
        listed_account_names(service, company, &query, 2).await
    };
    let ascending = ["Alfa", "bravo", "Caixa 100%", "Delta_x", "echo"];
    assert_eq!(names(all.clone()).await, ascending);
    let descending: Vec<&str> = ascending.iter().rev().copied().collect();
    assert_eq!(
        names(all.clone().sorted(NameSort::NameDesc)).await,
        descending
    );

    assert_eq!(
        names(all.clone().with_active(true)).await,
        ["Alfa", "Caixa 100%", "Delta_x"]
    );
    assert_eq!(
        names(all.clone().with_active(false)).await,
        ["bravo", "echo"]
    );
    assert_eq!(names(all.clone().of_kind(Checking)).await, ["Alfa", "echo"]);
    assert_eq!(names(all.clone().of_kind(Cash)).await, ["Caixa 100%"]);

    // A name anywhere in it, ignoring case; an account number ignoring its punctuation.
    assert_eq!(names(all.clone().searching("ALF")).await, ["Alfa"]);
    assert_eq!(names(all.clone().searching("10-01")).await, ["Alfa"]);
    assert_eq!(names(all.clone().searching(" 2002 ")).await, ["bravo"]);
    // `%` and `_` are ordinary characters, not wildcards.
    assert_eq!(names(all.clone().searching("%")).await, ["Caixa 100%"]);
    assert_eq!(names(all.clone().searching("_")).await, ["Delta_x"]);
    assert!(names(all.clone().searching("nobody")).await.is_empty());
    assert_eq!(names(all.clone().searching("al\0fa")).await, ["Alfa"]);
    assert_eq!(names(all.clone().searching("  ")).await, ascending);

    let active_checking = all.clone().with_active(true).of_kind(Checking);
    assert_eq!(names(active_checking).await, ["Alfa"]);
    let active_with_a = all
        .with_active(true)
        .searching("a")
        .sorted(NameSort::NameDesc);
    assert_eq!(
        names(active_with_a).await,
        ["Delta_x", "Caixa 100%", "Alfa"]
    );
}

// ---- payment methods

fn method_draft(kind: PaymentMethodKind, name: &str, active: bool) -> PaymentMethodDraft {
    PaymentMethodDraft {
        kind,
        name: name.into(),
        active,
    }
}

fn new_method(company: CompanyId, name: &str) -> PaymentMethod {
    PaymentMethod::new(
        PaymentMethodId::generate(),
        company,
        method_draft(PaymentMethodKind::Pix, name, true),
    )
    .unwrap()
}

fn method_entry(
    method: &PaymentMethod,
    action: AuditAction,
    changes: Vec<FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: method.company_id(),
        actor: actor(),
        entity: AuditEntity::PaymentMethod,
        entity_id: method.id().as_uuid(),
        action,
        changes,
    }
}

async fn save_method<R: PaymentMethodRepository>(
    repository: &R,
    method: &PaymentMethod,
) -> Result<(), RepositoryError> {
    let entry = method_entry(
        method,
        AuditAction::Created,
        payment_method_creation_changes(method),
    );
    repository.save(method, &entry).await
}

async fn method_trail<R: PaymentMethodRepository>(
    repository: &R,
    company: CompanyId,
    method: &PaymentMethod,
) -> usize {
    repository
        .history(company, method.id())
        .await
        .unwrap()
        .len()
}

/// A write is stored with its audit entry, scoped by company; one that is refused or misses leaves
/// nothing in the trail, and an entry that describes something else is refused outright.
pub async fn stores_payment_methods_with_their_trail<R: PaymentMethodRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let pix = new_method(company, "Pix");
    save_method(repository, &pix).await.unwrap();
    assert_eq!(
        repository.find(company, pix.id()).await.unwrap(),
        Some(pix.clone())
    );
    assert_eq!(
        repository.find(other_company, pix.id()).await.unwrap(),
        None
    );
    assert_eq!(method_trail(repository, company, &pix).await, 1);
    assert_eq!(method_trail(repository, other_company, &pix).await, 0);

    // The same name in the same company, whatever its case, is refused: no row, no entry.
    let twin = new_method(company, "PIX");
    assert_eq!(
        save_method(repository, &twin).await,
        Err(RepositoryError::DuplicateName)
    );
    assert_eq!(repository.find(company, twin.id()).await.unwrap(), None);
    assert_eq!(method_trail(repository, company, &twin).await, 0);
    save_method(repository, &new_method(other_company, "Pix"))
        .await
        .unwrap();

    // An entry about another record, or another company, is refused and nothing is stored.
    let stray = new_method(company, "Estranho");
    let wrong_record = method_entry(&pix, AuditAction::Created, vec![]);
    assert!(repository.save(&stray, &wrong_record).await.is_err());
    assert_eq!(repository.find(company, stray.id()).await.unwrap(), None);
    let mut wrong_company = method_entry(&stray, AuditAction::Created, vec![]);
    wrong_company.company_id = other_company;
    assert!(repository.save(&stray, &wrong_company).await.is_err());
    assert_eq!(method_trail(repository, other_company, &stray).await, 0);

    // An update replaces the fields (the kind too) and adds an entry; the case of its own name is
    // not a clash.
    let changed = pix
        .replaced_with(method_draft(PaymentMethodKind::BankTransfer, "PIX", false))
        .unwrap();
    let entry = method_entry(
        &changed,
        AuditAction::Updated,
        payment_method_changes(&pix, &changed),
    );
    repository.update(&changed, &entry).await.unwrap();
    assert_eq!(
        repository.find(company, pix.id()).await.unwrap(),
        Some(changed.clone())
    );
    assert_eq!(method_trail(repository, company, &pix).await, 2);

    // One that clashes with another's name changes nothing and adds nothing.
    let other = new_method(company, "Boleto");
    save_method(repository, &other).await.unwrap();
    let clashing = other
        .replaced_with(method_draft(PaymentMethodKind::BankSlip, "pix", true))
        .unwrap();
    let entry = method_entry(
        &clashing,
        AuditAction::Updated,
        payment_method_changes(&other, &clashing),
    );
    assert_eq!(
        repository.update(&clashing, &entry).await,
        Err(RepositoryError::DuplicateName)
    );
    assert_eq!(
        repository.find(company, other.id()).await.unwrap(),
        Some(other.clone())
    );
    assert_eq!(method_trail(repository, company, &other).await, 1);

    // A name given up by a rename is free again.
    let renamed = other
        .replaced_with(method_draft(
            PaymentMethodKind::BankSlip,
            "Boleto bancário",
            true,
        ))
        .unwrap();
    let entry = method_entry(
        &renamed,
        AuditAction::Updated,
        payment_method_changes(&other, &renamed),
    );
    repository.update(&renamed, &entry).await.unwrap();
    save_method(repository, &new_method(company, "boleto"))
        .await
        .unwrap();

    // Not in that company, or not at all: nothing is updated and no entry is left.
    let foreign = PaymentMethod::new(
        pix.id(),
        other_company,
        method_draft(PaymentMethodKind::Cash, "Roubo", true),
    )
    .unwrap();
    let entry = method_entry(&foreign, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&foreign, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(method_trail(repository, other_company, &pix).await, 0);
    let unknown = new_method(company, "Fantasma");
    let entry = method_entry(&unknown, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&unknown, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(method_trail(repository, company, &unknown).await, 0);
}

/// An edit that changes nothing writes nothing, and the trail shows who changed what.
pub async fn edits_payment_methods_and_records_only_changes<R: PaymentMethodRepository>(
    service: &PaymentMethodService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let who = actor();
    let created = service
        .register(
            company,
            &who,
            method_draft(PaymentMethodKind::CreditCard, "  Visa ", true),
        )
        .await
        .unwrap();
    assert_eq!(created.name(), "Visa");

    service
        .update(
            company,
            &who,
            created.id(),
            method_draft(PaymentMethodKind::CreditCard, "Visa", true),
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
            &who,
            created.id(),
            method_draft(PaymentMethodKind::DebitCard, "Visa débito", false),
        )
        .await
        .unwrap();
    assert_eq!(updated.kind(), PaymentMethodKind::DebitCard);
    let history = service.history(company, created.id()).await.unwrap();
    assert_eq!(history.len(), 2);
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
            ("kind", Some("credit_card"), Some("debit_card")),
            ("name", Some("Visa"), Some("Visa débito")),
            ("active", Some("true"), Some("false"))
        ]
    );

    assert_eq!(
        service
            .update(
                other_company,
                &who,
                created.id(),
                method_draft(PaymentMethodKind::Cash, "X", true)
            )
            .await,
        Err(ApplicationError::PaymentMethodNotFound)
    );
    assert_eq!(
        service.history(other_company, created.id()).await,
        Err(ApplicationError::PaymentMethodNotFound)
    );
    assert_eq!(
        service
            .register(
                company,
                &who,
                method_draft(PaymentMethodKind::Cash, "visa DÉBITO", true)
            )
            .await,
        Err(ApplicationError::Repository(RepositoryError::DuplicateName))
    );
}

async fn listed_method_names<R: PaymentMethodRepository>(
    service: &PaymentMethodService<R>,
    company: CompanyId,
    query: &PaymentMethodQuery,
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

pub async fn filters_and_sorts_payment_methods<R: PaymentMethodRepository>(
    service: &PaymentMethodService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    use PaymentMethodKind::{Cash, Check, CreditCard, DebitCard, Pix};
    let who = actor();
    for (kind, name, active) in [
        (Pix, "Alfa", true),
        (Cash, "bravo", false),
        (CreditCard, "Charlie 100%", true),
        (DebitCard, "Delta_x", true),
        (Check, "echo", false),
    ] {
        service
            .register(company, &who, method_draft(kind, name, active))
            .await
            .unwrap();
    }
    service
        .register(other_company, &who, method_draft(Pix, "Alfa", true))
        .await
        .unwrap();

    let all = PaymentMethodQuery::default();
    let names = |query: PaymentMethodQuery| async move {
        listed_method_names(service, company, &query, 2).await
    };
    let ascending = ["Alfa", "bravo", "Charlie 100%", "Delta_x", "echo"];
    assert_eq!(names(all.clone()).await, ascending);
    let descending: Vec<&str> = ascending.iter().rev().copied().collect();
    assert_eq!(
        names(all.clone().sorted(NameSort::NameDesc)).await,
        descending
    );

    assert_eq!(
        names(all.clone().with_active(true)).await,
        ["Alfa", "Charlie 100%", "Delta_x"]
    );
    assert_eq!(
        names(all.clone().with_active(false)).await,
        ["bravo", "echo"]
    );
    assert_eq!(names(all.clone().of_kind(Check)).await, ["echo"]);
    assert_eq!(
        names(all.clone().of_kind(Pix).with_active(false)).await,
        Vec::<String>::new()
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

// ---- payment terms

fn term_draft(name: &str, parts: &[(u32, &str)], active: bool) -> PaymentTermDraft {
    PaymentTermDraft {
        name: name.into(),
        installments: parts
            .iter()
            .map(|(days, percent)| InstallmentDraft {
                days: *days,
                percent: (*percent).into(),
            })
            .collect(),
        active,
    }
}

fn new_term(company: CompanyId, name: &str, parts: &[(u32, &str)]) -> PaymentTerm {
    PaymentTerm::new(
        PaymentTermId::generate(),
        company,
        term_draft(name, parts, true),
    )
    .unwrap()
}

fn term_entry(term: &PaymentTerm, action: AuditAction, changes: Vec<FieldChange>) -> AuditEntry {
    AuditEntry {
        company_id: term.company_id(),
        actor: actor(),
        entity: AuditEntity::PaymentTerm,
        entity_id: term.id().as_uuid(),
        action,
        changes,
    }
}

async fn save_term<R: PaymentTermRepository>(
    repository: &R,
    term: &PaymentTerm,
) -> Result<(), RepositoryError> {
    let entry = term_entry(
        term,
        AuditAction::Created,
        payment_term_creation_changes(term),
    );
    repository.save(term, &entry).await
}

async fn term_trail<R: PaymentTermRepository>(
    repository: &R,
    company: CompanyId,
    term: &PaymentTerm,
) -> usize {
    repository.history(company, term.id()).await.unwrap().len()
}

async fn stored_installments<R: PaymentTermRepository>(
    repository: &R,
    company: CompanyId,
    term: &PaymentTerm,
) -> Option<String> {
    repository
        .find(company, term.id())
        .await
        .unwrap()
        .map(|stored| stored.installments_text())
}

/// A write is stored with its audit entry and its installments, scoped by company; one that is
/// refused or misses leaves nothing in the trail and the old installments in place.
pub async fn stores_payment_terms_with_their_trail<R: PaymentTermRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
) {
    let thirty = new_term(
        company,
        "30/60/90",
        &[(30, "33.33"), (60, "33.33"), (90, "33.34")],
    );
    save_term(repository, &thirty).await.unwrap();
    // What comes back is what went in: the installments, in order, with their percentages.
    assert_eq!(
        repository.find(company, thirty.id()).await.unwrap(),
        Some(thirty.clone())
    );
    assert_eq!(
        stored_installments(repository, company, &thirty)
            .await
            .as_deref(),
        Some("30:33.33,60:33.33,90:33.34")
    );
    assert_eq!(
        repository.find(other_company, thirty.id()).await.unwrap(),
        None
    );
    assert_eq!(term_trail(repository, company, &thirty).await, 1);
    assert_eq!(term_trail(repository, other_company, &thirty).await, 0);

    // A whole percentage reads back as one, not as `50.00`, whichever adapter keeps it.
    let half = new_term(company, "Entrada e 30", &[(0, "50"), (30, "50")]);
    save_term(repository, &half).await.unwrap();
    assert_eq!(
        repository.find(company, half.id()).await.unwrap(),
        Some(half.clone())
    );

    // The same name, whatever its case, is refused: no term, no installments, no entry.
    let twin = new_term(company, "30/60/90 ", &[(0, "100")]);
    let shouting = new_term(company, "ENTRADA E 30", &[(0, "100")]);
    for refused in [&twin, &shouting] {
        assert_eq!(
            save_term(repository, refused).await,
            Err(RepositoryError::DuplicateName)
        );
        assert_eq!(repository.find(company, refused.id()).await.unwrap(), None);
        assert_eq!(term_trail(repository, company, refused).await, 0);
    }
    save_term(
        repository,
        &new_term(other_company, "30/60/90", &[(0, "100")]),
    )
    .await
    .unwrap();

    // An entry about another record, or another company, is refused and nothing is stored.
    let stray = new_term(company, "Estranho", &[(0, "100")]);
    let wrong_record = term_entry(&thirty, AuditAction::Created, vec![]);
    assert!(repository.save(&stray, &wrong_record).await.is_err());
    assert_eq!(repository.find(company, stray.id()).await.unwrap(), None);
    let mut wrong_company = term_entry(&stray, AuditAction::Created, vec![]);
    wrong_company.company_id = other_company;
    assert!(repository.save(&stray, &wrong_company).await.is_err());
    assert_eq!(term_trail(repository, other_company, &stray).await, 0);

    // An update replaces the name, the flag and the installments as a whole: more, fewer or other
    // installments, with the entry; the case of its own name is not a clash.
    let changed = thirty
        .replaced_with(term_draft("30/60/90 dias", &[(0, "25"), (30, "75")], false))
        .unwrap();
    let entry = term_entry(
        &changed,
        AuditAction::Updated,
        payment_term_changes(&thirty, &changed),
    );
    repository.update(&changed, &entry).await.unwrap();
    assert_eq!(
        repository.find(company, thirty.id()).await.unwrap(),
        Some(changed.clone())
    );
    assert_eq!(
        stored_installments(repository, company, &thirty)
            .await
            .as_deref(),
        Some("0:25,30:75")
    );
    assert_eq!(term_trail(repository, company, &thirty).await, 2);
    let more = changed
        .replaced_with(term_draft(
            "30/60/90 dias",
            &[(10, "20"), (20, "20"), (30, "20"), (40, "20"), (50, "20")],
            false,
        ))
        .unwrap();
    let entry = term_entry(
        &more,
        AuditAction::Updated,
        payment_term_changes(&changed, &more),
    );
    repository.update(&more, &entry).await.unwrap();
    assert_eq!(
        repository.find(company, thirty.id()).await.unwrap(),
        Some(more.clone())
    );

    // An update that clashes with another's name changes nothing: not the name, not the flag, not
    // the installments it was about to replace, and no entry.
    let clashing = half
        .replaced_with(term_draft("30/60/90 DIAS", &[(5, "100")], false))
        .unwrap();
    let entry = term_entry(
        &clashing,
        AuditAction::Updated,
        payment_term_changes(&half, &clashing),
    );
    assert_eq!(
        repository.update(&clashing, &entry).await,
        Err(RepositoryError::DuplicateName)
    );
    assert_eq!(
        repository.find(company, half.id()).await.unwrap(),
        Some(half.clone())
    );
    assert_eq!(
        stored_installments(repository, company, &half)
            .await
            .as_deref(),
        Some("0:50,30:50")
    );
    assert_eq!(term_trail(repository, company, &half).await, 1);

    // A name given up by a rename is free again.
    let renamed = half
        .replaced_with(term_draft("Entrada + 30", &[(0, "50"), (30, "50")], true))
        .unwrap();
    let entry = term_entry(
        &renamed,
        AuditAction::Updated,
        payment_term_changes(&half, &renamed),
    );
    repository.update(&renamed, &entry).await.unwrap();
    save_term(
        repository,
        &new_term(company, "entrada e 30", &[(0, "100")]),
    )
    .await
    .unwrap();

    // Not in that company, or not at all: nothing is updated, no installment replaced, no entry.
    let foreign = PaymentTerm::new(
        thirty.id(),
        other_company,
        term_draft("Roubo", &[(0, "100")], true),
    )
    .unwrap();
    let entry = term_entry(&foreign, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&foreign, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(term_trail(repository, other_company, &thirty).await, 0);
    assert_eq!(
        stored_installments(repository, company, &thirty)
            .await
            .as_deref(),
        Some("10:20,20:20,30:20,40:20,50:20")
    );
    let unknown = new_term(company, "Fantasma", &[(0, "100")]);
    let entry = term_entry(&unknown, AuditAction::Updated, vec![]);
    assert_eq!(
        repository.update(&unknown, &entry).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(term_trail(repository, company, &unknown).await, 0);
}

/// An edit that changes nothing writes nothing, and the trail shows the installments that changed.
pub async fn edits_payment_terms_and_records_only_changes<R: PaymentTermRepository>(
    service: &PaymentTermService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let who = actor();
    let created = service
        .register(
            company,
            &who,
            term_draft("  À vista e 30 ", &[(0, " 50.00 "), (30, "50")], true),
        )
        .await
        .unwrap();
    assert_eq!(created.name(), "À vista e 30");
    assert_eq!(created.installments_text(), "0:50,30:50");

    // The same installments written differently are no change.
    service
        .update(
            company,
            &who,
            created.id(),
            term_draft("À vista e 30", &[(0, "50"), (30, "50.0")], true),
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
            &who,
            created.id(),
            term_draft("À vista e 30", &[(0, "40"), (30, "60")], false),
        )
        .await
        .unwrap();
    assert_eq!(updated.installments_text(), "0:40,30:60");
    let history = service.history(company, created.id()).await.unwrap();
    assert_eq!(history.len(), 2);
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
            ("installments", Some("0:50,30:50"), Some("0:40,30:60")),
            ("active", Some("true"), Some("false"))
        ]
    );

    // A term that does not add up, or whose days do not increase, is refused and changes nothing.
    let domain = |error: DomainError| Err(ApplicationError::Domain(error));
    assert_eq!(
        service
            .update(
                company,
                &who,
                created.id(),
                term_draft("X", &[(0, "40"), (30, "59.99")], true)
            )
            .await,
        domain(DomainError::PercentsDoNotSumTo100)
    );
    assert_eq!(
        service
            .update(
                company,
                &who,
                created.id(),
                term_draft("X", &[(30, "50"), (30, "50")], true)
            )
            .await,
        domain(DomainError::InstallmentDaysNotIncreasing)
    );
    assert_eq!(
        service.history(company, created.id()).await.unwrap().len(),
        2
    );

    assert_eq!(
        service
            .update(
                other_company,
                &who,
                created.id(),
                term_draft("X", &[(0, "100")], true)
            )
            .await,
        Err(ApplicationError::PaymentTermNotFound)
    );
    assert_eq!(
        service.history(other_company, created.id()).await,
        Err(ApplicationError::PaymentTermNotFound)
    );
    assert_eq!(
        service
            .register(
                company,
                &who,
                term_draft("à VISTA E 30", &[(0, "100")], true)
            )
            .await,
        Err(ApplicationError::Repository(RepositoryError::DuplicateName))
    );
}

async fn listed_term_names<R: PaymentTermRepository>(
    service: &PaymentTermService<R>,
    company: CompanyId,
    query: &PaymentTermQuery,
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

pub async fn filters_and_sorts_payment_terms<R: PaymentTermRepository>(
    service: &PaymentTermService<R>,
    company: CompanyId,
    other_company: CompanyId,
) {
    let who = actor();
    for (name, parts, active) in [
        ("Alfa", vec![(0, "100")], true),
        ("bravo", vec![(30, "50"), (60, "50")], false),
        (
            "Charlie 100%",
            vec![(0, "20"), (10, "30"), (20, "50")],
            true,
        ),
        ("Delta_x", vec![(7, "100")], true),
        ("echo", vec![(0, "100")], false),
    ] {
        service
            .register(company, &who, term_draft(name, &parts, active))
            .await
            .unwrap();
    }
    service
        .register(other_company, &who, term_draft("Alfa", &[(0, "100")], true))
        .await
        .unwrap();

    let all = PaymentTermQuery::default();
    let names = |query: PaymentTermQuery| async move {
        listed_term_names(service, company, &query, 2).await
    };
    let ascending = ["Alfa", "bravo", "Charlie 100%", "Delta_x", "echo"];
    assert_eq!(names(all.clone()).await, ascending);
    let descending: Vec<&str> = ascending.iter().rev().copied().collect();
    assert_eq!(
        names(all.clone().sorted(NameSort::NameDesc)).await,
        descending
    );

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

    // Each listed term has its own installments, in order, not another term's.
    let page = service.list(company, &all, None, None).await.unwrap();
    let texts: Vec<String> = page
        .items
        .iter()
        .map(PaymentTerm::installments_text)
        .collect();
    assert_eq!(
        texts,
        ["0:100", "30:50,60:50", "0:20,10:30,20:50", "7:100", "0:100"]
    );

    let active_with_a = all
        .with_active(true)
        .searching("a")
        .sorted(NameSort::NameDesc);
    assert_eq!(
        names(active_with_a).await,
        ["Delta_x", "Charlie 100%", "Alfa"]
    );
}

// ---------- financial entries ----------

use conta_verde_application::{
    EntryCursor, EntryReferences, EntrySort, FinancialEntryQuery, FinancialEntryRepository,
    FinancialEntryService,
};
use conta_verde_domain::{
    EntryId, EntryInstallmentDraft, EntryKind, FinancialEntry, FinancialEntryDraft, Settlement,
    SettlementDraft, SettlementId, entry_changes, entry_creation_changes,
};

/// What an entry may point at, and what it may not, in one company: made by `seed_entry_records`
/// through the registers' own ports, so any adapter can run the contract.
pub struct EntrySeed {
    pub category: CategoryId,
    pub other_category: CategoryId,
    pub inactive_category: CategoryId,
    pub transfer_category: CategoryId,
    pub cost_center: CostCenterId,
    pub inactive_cost_center: CostCenterId,
    pub party: PartyId,
    pub bank_account: BankAccountId,
    /// A second active account, for what moves between two of them.
    pub other_bank_account: BankAccountId,
    pub inactive_bank_account: BankAccountId,
    pub payment_method: PaymentMethodId,
    pub inactive_payment_method: PaymentMethodId,
}

pub async fn seed_entry_records<C, K, P, B, M>(
    categories: &C,
    cost_centers: &K,
    parties: &P,
    bank_accounts: &B,
    payment_methods: &M,
    company: CompanyId,
) -> EntrySeed
where
    C: CategoryRepository,
    K: CostCenterRepository,
    P: PartyRepository,
    B: BankAccountRepository,
    M: PaymentMethodRepository,
{
    let usable = root_category(company, "Vendas");
    let other = root_category(company, "Serviços");
    let retired = root_category(company, "Antiga");
    for category in [&usable, &other, &retired] {
        save_category(categories, category).await;
    }
    let deactivated = retired.changed("Antiga", false).unwrap();
    let entry = category_entry(
        &deactivated,
        AuditAction::Updated,
        category_changes(&retired, &deactivated),
    );
    categories.update(&deactivated, &entry).await.unwrap();
    let transfers = Category::new_root(
        CategoryId::generate(),
        company,
        "Transferências",
        DreGroup::Transfers,
        CashFlowGroup::Transfer,
    )
    .unwrap();
    save_category(categories, &transfers).await;

    let cost_center = new_cost_center(company, "Matriz");
    let inactive_cost_center = CostCenter::new(
        CostCenterId::generate(),
        company,
        cost_center_draft("Fechado", false),
    )
    .unwrap();
    for center in [&cost_center, &inactive_cost_center] {
        let entry = cost_center_entry(
            center,
            AuditAction::Created,
            cost_center_creation_changes(center),
        );
        cost_centers.save(center, &entry).await.unwrap();
    }

    let party = Party::new(
        PartyId::generate(),
        company,
        party_draft("Mercado Boa Compra", None),
    )
    .unwrap();
    parties.save(&party, &created_entry(&party)).await.unwrap();

    let account = new_account(company, "Conta principal", None);
    let closed_account = BankAccount::new(
        BankAccountId::generate(),
        company,
        account_draft(BankAccountKind::Cash, "Caixa antigo", None, false),
    )
    .unwrap();
    let second_account = new_account(company, "Reserva", None);
    for item in [&account, &second_account, &closed_account] {
        save_account(bank_accounts, item).await.unwrap();
    }
    let method = new_method(company, "Pix");
    let closed_method = PaymentMethod::new(
        PaymentMethodId::generate(),
        company,
        method_draft(PaymentMethodKind::Cash, "Cheque antigo", false),
    )
    .unwrap();
    for item in [&method, &closed_method] {
        save_method(payment_methods, item).await.unwrap();
    }

    EntrySeed {
        category: usable.id(),
        other_category: other.id(),
        inactive_category: retired.id(),
        transfer_category: transfers.id(),
        cost_center: cost_center.id(),
        inactive_cost_center: inactive_cost_center.id(),
        party: party.id(),
        bank_account: account.id(),
        other_bank_account: second_account.id(),
        inactive_bank_account: closed_account.id(),
        payment_method: method.id(),
        inactive_payment_method: closed_method.id(),
    }
}

fn entry_draft(
    seed: &EntrySeed,
    description: &str,
    competence: &str,
    parts: &[(&str, &str)],
) -> FinancialEntryDraft {
    FinancialEntryDraft {
        kind: EntryKind::Receivable,
        description: description.into(),
        party_id: None,
        category_id: seed.category,
        cost_center_id: None,
        competence_date: competence.into(),
        installments: parts
            .iter()
            .map(|(due, amount)| EntryInstallmentDraft {
                due_date: (*due).into(),
                amount: (*amount).into(),
            })
            .collect(),
    }
}

fn new_entry(company: CompanyId, draft: FinancialEntryDraft) -> FinancialEntry {
    FinancialEntry::new(EntryId::generate(), company, draft).unwrap()
}

fn entry_record(
    entry: &FinancialEntry,
    action: AuditAction,
    changes: Vec<FieldChange>,
) -> AuditEntry {
    AuditEntry {
        company_id: entry.company_id(),
        actor: actor(),
        entity: AuditEntity::FinancialEntry,
        entity_id: entry.id().as_uuid(),
        action,
        changes,
    }
}

async fn save_entry<R: FinancialEntryRepository>(
    repository: &R,
    entry: &FinancialEntry,
) -> Result<(), RepositoryError> {
    let record = entry_record(entry, AuditAction::Created, entry_creation_changes(entry));
    repository.save(entry, &record).await
}

async fn entry_trail<R: FinancialEntryRepository>(
    repository: &R,
    company: CompanyId,
    entry: &FinancialEntry,
) -> usize {
    repository.history(company, entry.id()).await.unwrap().len()
}

/// The registers say what an entry may point at: a category that exists in the company, is active
/// and is not the one for transfers; a cost center that exists and is active; a party that exists.
/// Another company's records answer like ones that do not exist.
pub async fn entry_references_say_what_is_usable<R: EntryReferences>(
    references: &R,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) {
    let category = |company, id| references.category_is_usable(company, id);
    assert!(category(company, seed.category).await.unwrap());
    assert!(category(company, seed.other_category).await.unwrap());
    assert!(!category(company, seed.inactive_category).await.unwrap());
    assert!(!category(company, seed.transfer_category).await.unwrap());
    assert!(!category(other_company, seed.category).await.unwrap());
    assert!(
        !category(company, CategoryId::generate()).await.unwrap(),
        "an unknown id"
    );

    let center = |company, id| references.cost_center_is_usable(company, id);
    assert!(center(company, seed.cost_center).await.unwrap());
    assert!(!center(company, seed.inactive_cost_center).await.unwrap());
    assert!(!center(other_company, seed.cost_center).await.unwrap());
    assert!(!center(company, CostCenterId::generate()).await.unwrap());

    let account = |company, id| references.bank_account_is_usable(company, id);
    assert!(account(company, seed.bank_account).await.unwrap());
    assert!(!account(company, seed.inactive_bank_account).await.unwrap());
    assert!(!account(other_company, seed.bank_account).await.unwrap());
    assert!(!account(company, BankAccountId::generate()).await.unwrap());
    let method = |company, id| references.payment_method_is_usable(company, id);
    assert!(method(company, seed.payment_method).await.unwrap());
    assert!(!method(company, seed.inactive_payment_method).await.unwrap());
    assert!(!method(other_company, seed.payment_method).await.unwrap());
    assert!(!method(company, PaymentMethodId::generate()).await.unwrap());

    assert!(references.party_exists(company, seed.party).await.unwrap());
    assert!(
        !references
            .party_exists(other_company, seed.party)
            .await
            .unwrap()
    );
    assert!(
        !references
            .party_exists(company, PartyId::generate())
            .await
            .unwrap()
    );
}

/// A write is stored with its audit entry and its installments, scoped by company; one that is
/// refused or misses leaves nothing in the trail and the old installments in place.
pub async fn stores_entries_with_their_trail<R: FinancialEntryRepository>(
    repository: &R,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) {
    let mut draft = entry_draft(
        seed,
        "Venda parcelada",
        "2026-11-05",
        &[
            ("2026-12-10", "100.50"),
            ("2027-01-10", "100.5"),
            ("2027-01-10", "0.01"),
        ],
    );
    draft.party_id = Some(seed.party);
    draft.cost_center_id = Some(seed.cost_center);
    let sale = new_entry(company, draft);
    save_entry(repository, &sale).await.unwrap();
    // What comes back is what went in: the installments in order, a whole amount without zeros.
    let stored = repository.find(company, sale.id()).await.unwrap();
    assert_eq!(stored, Some(sale.clone()));
    assert_eq!(
        stored.unwrap().installments_text(),
        "1:2026-12-10:100.5,2:2027-01-10:100.5,3:2027-01-10:0.01"
    );
    assert_eq!(
        repository.find(other_company, sale.id()).await.unwrap(),
        None
    );
    assert_eq!(entry_trail(repository, company, &sale).await, 1);
    assert_eq!(entry_trail(repository, other_company, &sale).await, 0);

    // An audit entry about another record, or another company, is refused and nothing is stored.
    let stray = new_entry(
        company,
        entry_draft(seed, "Estranho", "2026-11-05", &[("2026-12-10", "1")]),
    );
    let wrong_record = entry_record(&sale, AuditAction::Created, vec![]);
    assert!(repository.save(&stray, &wrong_record).await.is_err());
    assert_eq!(repository.find(company, stray.id()).await.unwrap(), None);
    let mut wrong_company = entry_record(&stray, AuditAction::Created, vec![]);
    wrong_company.company_id = other_company;
    assert!(repository.save(&stray, &wrong_company).await.is_err());
    assert_eq!(entry_trail(repository, other_company, &stray).await, 0);

    // An update replaces the fields and the installments as a whole: fewer, more or other ones.
    let mut changed_draft = entry_draft(
        seed,
        "Venda à vista",
        "2026-11-06",
        &[("2026-11-06", "201.01")],
    );
    changed_draft.kind = EntryKind::Payable;
    let changed = sale.replaced_with(changed_draft).unwrap();
    let record = entry_record(
        &changed,
        AuditAction::Updated,
        entry_changes(&sale, &changed),
    );
    repository.update(&changed, &record).await.unwrap();
    assert_eq!(
        repository.find(company, sale.id()).await.unwrap(),
        Some(changed.clone())
    );
    assert_eq!(entry_trail(repository, company, &sale).await, 2);
    let more = changed
        .replaced_with(entry_draft(
            seed,
            "Venda à vista",
            "2026-11-06",
            &[
                ("2026-11-06", "50"),
                ("2026-12-06", "50"),
                ("2027-01-06", "50"),
                ("2027-02-06", "51.01"),
            ],
        ))
        .unwrap();
    let record = entry_record(&more, AuditAction::Updated, entry_changes(&changed, &more));
    repository.update(&more, &record).await.unwrap();
    assert_eq!(
        repository.find(company, sale.id()).await.unwrap(),
        Some(more.clone())
    );

    // An update that misses (another company's entry) changes nothing: not the fields, not the
    // installments it was about to replace, and no audit entry.
    let foreign = FinancialEntry::new(
        sale.id(),
        other_company,
        entry_draft(seed, "Roubo", "2026-01-01", &[("2026-01-01", "9")]),
    )
    .unwrap();
    let record = entry_record(
        &foreign,
        AuditAction::Updated,
        entry_changes(&more, &foreign),
    );
    assert_eq!(
        repository.update(&foreign, &record).await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repository.find(company, sale.id()).await.unwrap(),
        Some(more.clone())
    );
    assert_eq!(entry_trail(repository, company, &sale).await, 3);
    assert_eq!(entry_trail(repository, other_company, &foreign).await, 0);
}

/// Registering and editing through the use case: references are checked before anything is written,
/// an edit that changes nothing writes and records nothing, an edit records only what changed, and
/// another company's entry is not found.
pub async fn edits_entries_and_records_only_changes<R, F>(
    service: &FinancialEntryService<R, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    R: FinancialEntryRepository,
    F: EntryReferences,
{
    let ana = actor();
    let draft = entry_draft(
        seed,
        "Aluguel de novembro",
        "2026-11-01",
        &[("2026-11-10", "2500")],
    );
    let rent = service
        .register(company, &ana, draft.clone())
        .await
        .unwrap();
    assert_eq!(service.history(company, rent.id()).await.unwrap().len(), 1);

    // Pointing at what cannot be used is refused, naming the field, and leaves nothing behind.
    let refused = [
        (
            "category",
            FinancialEntryDraft {
                category_id: seed.transfer_category,
                ..draft.clone()
            },
        ),
        (
            "category",
            FinancialEntryDraft {
                category_id: seed.inactive_category,
                ..draft.clone()
            },
        ),
        (
            "cost center",
            FinancialEntryDraft {
                cost_center_id: Some(seed.inactive_cost_center),
                ..draft.clone()
            },
        ),
        (
            "party",
            FinancialEntryDraft {
                party_id: Some(PartyId::generate()),
                ..draft.clone()
            },
        ),
    ];
    for (field, bad) in refused {
        assert_eq!(
            service
                .register(company, &ana, bad.clone())
                .await
                .unwrap_err(),
            ApplicationError::UnusableReference(field)
        );
        assert_eq!(
            service
                .update(company, &ana, rent.id(), bad)
                .await
                .unwrap_err(),
            ApplicationError::UnusableReference(field)
        );
    }
    let listed = service
        .list(company, &FinancialEntryQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1, "a refused entry is not stored");
    assert_eq!(service.history(company, rent.id()).await.unwrap().len(), 1);
    // The company's records are not usable in another company.
    assert_eq!(
        service
            .register(other_company, &ana, draft.clone())
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("category")
    );

    // The same draft changes nothing: no write, no audit entry.
    service
        .update(company, &ana, rent.id(), draft.clone())
        .await
        .unwrap();
    assert_eq!(service.history(company, rent.id()).await.unwrap().len(), 1);

    // A change records only the fields that changed.
    let mut moved = draft.clone();
    moved.installments = vec![
        EntryInstallmentDraft {
            due_date: "2026-11-10".into(),
            amount: "1250".into(),
        },
        EntryInstallmentDraft {
            due_date: "2026-12-10".into(),
            amount: "1250".into(),
        },
    ];
    moved.cost_center_id = Some(seed.cost_center);
    let updated = service
        .update(company, &ana, rent.id(), moved)
        .await
        .unwrap();
    assert_eq!(updated.total().to_string(), "2500");
    let trail = service.history(company, rent.id()).await.unwrap();
    assert_eq!(trail.len(), 2);
    let mut fields: Vec<&str> = trail[1]
        .entry
        .changes
        .iter()
        .map(|change| change.field.as_str())
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["cost_center_id", "installments"]);

    // A draft the domain refuses changes nothing.
    let mut invalid = draft.clone();
    invalid.installments[0].amount = "1.005".into();
    assert_eq!(
        service
            .update(company, &ana, rent.id(), invalid)
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::InvalidInstallmentAmount)
    );
    assert_eq!(
        service
            .get(company, rent.id())
            .await
            .unwrap()
            .installments_text(),
        "1:2026-11-10:1250,2:2026-12-10:1250"
    );

    // Another company's entry is not found, and neither is its trail.
    assert_eq!(
        service.get(other_company, rent.id()).await.unwrap_err(),
        ApplicationError::EntryNotFound
    );
    assert_eq!(
        service
            .update(other_company, &ana, rent.id(), draft)
            .await
            .unwrap_err(),
        ApplicationError::EntryNotFound
    );
    assert_eq!(
        service.history(other_company, rent.id()).await.unwrap_err(),
        ApplicationError::EntryNotFound
    );
}

/// Listing is by competence date, newest first unless asked otherwise, then by id; a page resumes
/// right after its cursor; a filter narrows by kind, category or a piece of the description; and a
/// company sees only its own entries.
pub async fn filters_and_sorts_entries<R, F>(
    service: &FinancialEntryService<R, F>,
    company: CompanyId,
    other_company: CompanyId,
    other_seed: &EntrySeed,
    seed: &EntrySeed,
) where
    R: FinancialEntryRepository,
    F: EntryReferences,
{
    let ana = actor();
    // Two entries share a competence day, so the id has to break the tie the same way everywhere.
    for (description, kind, category, competence) in [
        ("Aluguel", EntryKind::Payable, seed.category, "2026-09-01"),
        (
            "Venda de bolos 100%",
            EntryKind::Receivable,
            seed.category,
            "2026-10-01",
        ),
        (
            "Serviço_x",
            EntryKind::Receivable,
            seed.other_category,
            "2026-10-01",
        ),
        (
            "Fornecedor de farinha",
            EntryKind::Payable,
            seed.other_category,
            "2026-11-01",
        ),
        (
            "Venda de pães",
            EntryKind::Receivable,
            seed.category,
            "2026-08-01",
        ),
    ] {
        let mut draft = entry_draft(seed, description, competence, &[("2026-12-01", "10")]);
        draft.kind = kind;
        draft.category_id = category;
        service.register(company, &ana, draft).await.unwrap();
    }
    service
        .register(
            other_company,
            &ana,
            entry_draft(other_seed, "Doutro", "2026-10-15", &[("2026-12-01", "10")]),
        )
        .await
        .unwrap();

    let descriptions = |query: FinancialEntryQuery| async move {
        service
            .list(company, &query, None, None)
            .await
            .unwrap()
            .items
            .iter()
            .map(|entry| entry.description().to_owned())
            .collect::<Vec<_>>()
    };
    let all = FinancialEntryQuery::default();
    let newest_first = descriptions(all.clone()).await;
    assert_eq!(newest_first.len(), 5, "only this company's entries");
    assert_eq!(newest_first[0], "Fornecedor de farinha");
    assert_eq!(newest_first[3], "Aluguel");
    assert_eq!(newest_first[4], "Venda de pães");
    let mut oldest_first = descriptions(all.clone().sorted(EntrySort::CompetenceAsc)).await;
    oldest_first.reverse();
    assert_eq!(
        oldest_first, newest_first,
        "the other direction is the exact reverse"
    );

    // Pages follow the cursor, in both directions, and never repeat or skip.
    for sort in [EntrySort::CompetenceDesc, EntrySort::CompetenceAsc] {
        let query = all.clone().sorted(sort);
        let expected: Vec<EntryId> = service
            .list(company, &query, None, None)
            .await
            .unwrap()
            .items
            .iter()
            .map(FinancialEntry::id)
            .collect();
        let mut seen = Vec::new();
        let mut after: Option<EntryCursor> = None;
        loop {
            let page = service
                .list(company, &query, after.as_ref(), Some(2))
                .await
                .unwrap();
            assert!(page.items.len() <= 2);
            seen.extend(page.items.iter().map(FinancialEntry::id));
            match page.next {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        assert_eq!(seen, expected, "{sort:?}");
        assert_eq!(seen.len(), 5);
    }

    assert_eq!(
        descriptions(all.clone().with_kind(EntryKind::Payable)).await,
        ["Fornecedor de farinha", "Aluguel"]
    );
    assert_eq!(
        descriptions(
            all.clone()
                .with_category(seed.other_category)
                .sorted(EntrySort::CompetenceAsc)
        )
        .await,
        ["Serviço_x", "Fornecedor de farinha"]
    );
    assert_eq!(descriptions(all.clone().searching("VENDA")).await.len(), 2);
    assert_eq!(
        descriptions(all.clone().searching("%")).await,
        ["Venda de bolos 100%"]
    );
    assert_eq!(
        descriptions(all.clone().searching("_")).await,
        ["Serviço_x"]
    );
    assert!(
        descriptions(all.clone().searching("nobody"))
            .await
            .is_empty()
    );
    assert_eq!(descriptions(all.clone().searching("  ")).await.len(), 5);
    assert_eq!(
        descriptions(all.clone().searching("ven\0da")).await.len(),
        2
    );
    assert_eq!(
        descriptions(all.with_kind(EntryKind::Receivable).searching("venda"))
            .await
            .len(),
        2
    );
}

/// An edit that leaves a reference alone does not need it to be usable any more: a category that was
/// deactivated since does not stop the description from being fixed. Choosing it again, from another
/// category, is refused.
pub async fn an_edit_may_keep_what_was_deactivated_since<R, F, C>(
    service: &FinancialEntryService<R, F>,
    categories: &C,
    company: CompanyId,
    seed: &EntrySeed,
) where
    R: FinancialEntryRepository,
    F: EntryReferences,
    C: CategoryRepository,
{
    let ana = actor();
    let draft = entry_draft(seed, "Aluguel", "2026-11-01", &[("2026-11-10", "100")]);
    let rent = service
        .register(company, &ana, draft.clone())
        .await
        .unwrap();

    let before = categories
        .find(company, seed.category)
        .await
        .unwrap()
        .unwrap();
    let after = before.changed(before.name(), false).unwrap();
    let entry = category_entry(
        &after,
        AuditAction::Updated,
        category_changes(&before, &after),
    );
    categories.update(&after, &entry).await.unwrap();

    let fixed = FinancialEntryDraft {
        description: "Aluguel de novembro".into(),
        ..draft.clone()
    };
    service
        .update(company, &ana, rent.id(), fixed.clone())
        .await
        .unwrap();
    let elsewhere = FinancialEntryDraft {
        category_id: seed.other_category,
        ..fixed.clone()
    };
    service
        .update(company, &ana, rent.id(), elsewhere)
        .await
        .unwrap();
    assert_eq!(
        service
            .update(company, &ana, rent.id(), fixed)
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("category")
    );
}

fn settlement_draft(seed: &EntrySeed, installment: u16, applied: &str) -> SettlementDraft {
    SettlementDraft {
        installment,
        settled_on: "2026-12-10".into(),
        applied: applied.into(),
        interest: None,
        fine: None,
        discount: None,
        bank_account_id: seed.bank_account,
        payment_method_id: None,
    }
}

fn applied_of(settlements: &[Settlement], number: u16) -> String {
    conta_verde_domain::settled_of(settlements, number).to_string()
}

/// Paying installments: in part and in full, never for more than is left, only from a usable account
/// and method, only an installment the entry has, undone by reversing, and nothing of another
/// company's. What an entry may change while it has settlements, and after they are reversed.
pub async fn settles_installments_and_reverses_them<R, F>(
    service: &FinancialEntryService<R, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    R: FinancialEntryRepository,
    F: EntryReferences,
{
    let ana = actor();
    let entry = service
        .register(
            company,
            &ana,
            entry_draft(
                seed,
                "Venda parcelada",
                "2026-11-05",
                &[("2026-12-10", "100"), ("2027-01-10", "50.5")],
            ),
        )
        .await
        .unwrap();
    let settle = |draft: SettlementDraft| service.settle(company, &ana, entry.id(), draft);

    // In part, then the rest; what is applied, with interest, fine and discount, is the cash.
    let first = settle(SettlementDraft {
        interest: Some("1.5".into()),
        fine: Some("0.5".into()),
        discount: Some("2".into()),
        payment_method_id: Some(seed.payment_method),
        ..settlement_draft(seed, 1, "40")
    })
    .await
    .unwrap();
    assert_eq!(first.cash().to_string(), "40");
    let second = settle(settlement_draft(seed, 1, "60")).await.unwrap();
    let stored = service.settlements(company, entry.id()).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(applied_of(&stored, 1), "100");
    assert_eq!(applied_of(&stored, 2), "0");
    // What comes back is what went in, adjustments and method included.
    let found = stored.iter().find(|item| item.id() == first.id()).unwrap();
    assert_eq!(found, &first);
    assert_eq!(found.payment_method_id(), Some(seed.payment_method));

    // Never for more than is left, by a cent; a refused settlement leaves nothing behind.
    assert_eq!(
        settle(settlement_draft(seed, 1, "0.01")).await.unwrap_err(),
        ApplicationError::SettlementExceedsBalance
    );
    assert_eq!(
        settle(settlement_draft(seed, 2, "50.51"))
            .await
            .unwrap_err(),
        ApplicationError::SettlementExceedsBalance
    );
    assert_eq!(
        service
            .settlements(company, entry.id())
            .await
            .unwrap()
            .len(),
        2
    );

    // What it points at has to exist and be usable.
    assert_eq!(
        settle(settlement_draft(seed, 3, "1")).await.unwrap_err(),
        ApplicationError::UnusableReference("installment")
    );
    for (field, draft) in [
        (
            "bank account",
            SettlementDraft {
                bank_account_id: seed.inactive_bank_account,
                ..settlement_draft(seed, 2, "10")
            },
        ),
        (
            "bank account",
            SettlementDraft {
                bank_account_id: BankAccountId::generate(),
                ..settlement_draft(seed, 2, "10")
            },
        ),
        (
            "payment method",
            SettlementDraft {
                payment_method_id: Some(seed.inactive_payment_method),
                ..settlement_draft(seed, 2, "10")
            },
        ),
    ] {
        assert_eq!(
            settle(draft).await.unwrap_err(),
            ApplicationError::UnusableReference(field)
        );
    }
    // The domain's rules say what to change.
    assert_eq!(
        settle(settlement_draft(seed, 2, "1.005"))
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::InvalidSettlementAmount("applied"))
    );
    assert_eq!(
        settle(SettlementDraft {
            discount: Some("10".into()),
            ..settlement_draft(seed, 2, "10")
        })
        .await
        .unwrap_err(),
        ApplicationError::Domain(DomainError::SettlementMovesNothing)
    );

    // Another company's entry is not found, and a record of another company is not usable.
    assert_eq!(
        service
            .settle(
                other_company,
                &ana,
                entry.id(),
                settlement_draft(seed, 2, "10")
            )
            .await
            .unwrap_err(),
        ApplicationError::EntryNotFound
    );
    assert_eq!(
        service
            .settlements(other_company, entry.id())
            .await
            .unwrap_err(),
        ApplicationError::EntryNotFound
    );

    // While it has settlements, the installments and the kind are fixed; what only classifies is not.
    let same = entry_draft(
        seed,
        "Venda parcelada",
        "2026-11-05",
        &[("2026-12-10", "100"), ("2027-01-10", "50.5")],
    );
    let renamed = FinancialEntryDraft {
        description: "Venda de bolos".into(),
        category_id: seed.other_category,
        competence_date: "2026-11-06".into(),
        ..same.clone()
    };
    service
        .update(company, &ana, entry.id(), renamed.clone())
        .await
        .unwrap();
    for changed in [
        FinancialEntryDraft {
            installments: vec![EntryInstallmentDraft {
                due_date: "2026-12-10".into(),
                amount: "150.5".into(),
            }],
            ..renamed.clone()
        },
        FinancialEntryDraft {
            kind: EntryKind::Payable,
            ..renamed.clone()
        },
        FinancialEntryDraft {
            installments: vec![
                EntryInstallmentDraft {
                    due_date: "2026-12-11".into(),
                    amount: "100".into(),
                },
                EntryInstallmentDraft {
                    due_date: "2027-01-10".into(),
                    amount: "50.5".into(),
                },
            ],
            ..renamed.clone()
        },
    ] {
        assert_eq!(
            service
                .update(company, &ana, entry.id(), changed)
                .await
                .unwrap_err(),
            ApplicationError::EntryIsSettled
        );
    }
    let kept = service.get(company, entry.id()).await.unwrap();
    assert_eq!(
        kept.installments_text(),
        "1:2026-12-10:100,2:2027-01-10:50.5"
    );
    assert_eq!(kept.kind(), EntryKind::Receivable);
    // The same installments written another way are the same installments.
    service
        .update(
            company,
            &ana,
            entry.id(),
            FinancialEntryDraft {
                installments: vec![
                    EntryInstallmentDraft {
                        due_date: "2026-12-10".into(),
                        amount: "100.00".into(),
                    },
                    EntryInstallmentDraft {
                        due_date: "2027-01-10".into(),
                        amount: "50.50".into(),
                    },
                ],
                description: "Venda de bolos II".into(),
                ..renamed.clone()
            },
        )
        .await
        .unwrap();

    // Reversing: not found twice, nor for another company; it stops counting, and the installment
    // can be settled again.
    assert_eq!(
        service
            .reverse(other_company, &ana, entry.id(), second.id())
            .await
            .unwrap_err(),
        ApplicationError::SettlementNotFound
    );
    let undone = service
        .reverse(company, &ana, entry.id(), second.id())
        .await
        .unwrap();
    assert!(undone.is_reversed());
    assert_eq!(
        service
            .reverse(company, &ana, entry.id(), second.id())
            .await
            .unwrap_err(),
        ApplicationError::SettlementNotFound
    );
    assert_eq!(
        service
            .reverse(company, &ana, entry.id(), SettlementId::generate())
            .await
            .unwrap_err(),
        ApplicationError::SettlementNotFound
    );
    let after = service.settlements(company, entry.id()).await.unwrap();
    assert_eq!(after.len(), 2, "a reversed settlement stays");
    assert_eq!(applied_of(&after, 1), "40");
    settle(settlement_draft(seed, 1, "60")).await.unwrap();
    assert_eq!(
        settle(settlement_draft(seed, 1, "0.01")).await.unwrap_err(),
        ApplicationError::SettlementExceedsBalance
    );

    // Reversing them all frees the installments again.
    let all = service.settlements(company, entry.id()).await.unwrap();
    for settlement in all.iter().filter(|item| !item.is_reversed()) {
        service
            .reverse(company, &ana, entry.id(), settlement.id())
            .await
            .unwrap();
    }
    let single = FinancialEntryDraft {
        installments: vec![EntryInstallmentDraft {
            due_date: "2026-12-10".into(),
            amount: "150.5".into(),
        }],
        kind: EntryKind::Payable,
        ..renamed
    };
    let freed = service
        .update(company, &ana, entry.id(), single)
        .await
        .unwrap();
    assert_eq!(freed.total().to_string(), "150.5");
}

// ---------- transfers and balances ----------

use conta_verde_application::{
    AccountBalance, BalanceService, Balances, OpeningRepository, TransferQuery, TransferRepository,
    TransferService,
};
use conta_verde_domain::{CalendarDate, Transfer, TransferDraft};

fn transfer_draft(
    from: BankAccountId,
    to: BankAccountId,
    amount: &str,
    day: &str,
) -> TransferDraft {
    TransferDraft {
        from,
        to,
        amount: amount.into(),
        occurred_on: day.into(),
        note: None,
    }
}

async fn listed_transfers<T: TransferRepository, F: EntryReferences>(
    service: &TransferService<T, F>,
    company: CompanyId,
    query: &TransferQuery,
) -> Vec<Transfer> {
    service
        .list(company, query, None, Some(100))
        .await
        .unwrap()
        .items
}

/// A transfer moves money between two accounts of the company that are usable, is never edited, is
/// undone by reversing it, and leaves one audit entry per write and none for a refused one.
pub async fn moves_money_between_accounts<T, F>(
    service: &TransferService<T, F>,
    store: &T,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    T: TransferRepository,
    F: EntryReferences,
{
    let ana = actor();
    let (from, to) = (seed.bank_account, seed.other_bank_account);
    let everything = TransferQuery::default();

    let first = service
        .register(
            company,
            &ana,
            TransferDraft {
                note: Some("Reforço de caixa".into()),
                ..transfer_draft(from, to, "250.50", "2026-12-10")
            },
        )
        .await
        .unwrap();
    let second = service
        .register(company, &ana, transfer_draft(to, from, "10", "2026-12-12"))
        .await
        .unwrap();
    assert_eq!(first.note(), Some("Reforço de caixa"));
    assert_eq!(
        store.find(company, first.id()).await.unwrap(),
        Some(first.clone())
    );
    assert_eq!(store.history(company, first.id()).await.unwrap().len(), 1);

    // Newest first, narrowed by an account on either side, and nothing of another company's.
    let listed = listed_transfers(service, company, &everything).await;
    assert_eq!(
        listed.iter().map(Transfer::id).collect::<Vec<_>>(),
        vec![second.id(), first.id()]
    );
    for account in [from, to] {
        let query = TransferQuery::default().with_account(account);
        assert_eq!(listed_transfers(service, company, &query).await.len(), 2);
    }
    let unrelated = TransferQuery::default().with_account(seed.inactive_bank_account);
    assert!(
        listed_transfers(service, company, &unrelated)
            .await
            .is_empty()
    );
    assert!(
        listed_transfers(service, other_company, &everything)
            .await
            .is_empty()
    );
    assert_eq!(store.find(other_company, first.id()).await.unwrap(), None);
    assert!(
        store
            .history(other_company, first.id())
            .await
            .unwrap()
            .is_empty()
    );

    // Paging: one at a time, newest first, then from the cursor of the last one.
    let page = service
        .list(company, &everything, None, Some(1))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    let cursor = page.next.expect("there is another one");
    let rest = service
        .list(company, &everything, Some(&cursor), Some(1))
        .await
        .unwrap();
    assert_eq!(
        rest.items.iter().map(Transfer::id).collect::<Vec<_>>(),
        vec![first.id()]
    );
    assert!(rest.next.is_none());

    // What it refuses leaves nothing behind: the same account on both sides, an amount that is not
    // more than 0 or has more than two decimals, a date that is not one, an account that is closed
    // or is another company's, a note that is not one line.
    let refused = [
        (
            transfer_draft(from, from, "1", "2026-12-10"),
            ApplicationError::Domain(DomainError::TransferToTheSameAccount),
        ),
        (
            transfer_draft(from, to, "0", "2026-12-10"),
            ApplicationError::Domain(DomainError::InvalidTransferAmount),
        ),
        (
            transfer_draft(from, to, "1.005", "2026-12-10"),
            ApplicationError::Domain(DomainError::InvalidTransferAmount),
        ),
        (
            transfer_draft(from, to, "1", "2026-02-30"),
            ApplicationError::Domain(DomainError::InvalidDate("occurred_on")),
        ),
        (
            TransferDraft {
                note: Some("a\nb".into()),
                ..transfer_draft(from, to, "1", "2026-12-10")
            },
            ApplicationError::Domain(DomainError::InvalidNote),
        ),
        (
            transfer_draft(from, seed.inactive_bank_account, "1", "2026-12-10"),
            ApplicationError::UnusableReference("to"),
        ),
        (
            transfer_draft(seed.inactive_bank_account, to, "1", "2026-12-10"),
            ApplicationError::UnusableReference("from"),
        ),
    ];
    for (draft, error) in refused {
        assert_eq!(
            service.register(company, &ana, draft).await.unwrap_err(),
            error
        );
    }
    assert_eq!(
        service
            .register(
                other_company,
                &ana,
                transfer_draft(from, to, "1", "2026-12-10")
            )
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("from")
    );
    assert_eq!(
        listed_transfers(service, company, &everything).await.len(),
        2
    );

    // Reversing marks it: it stays listed, with its trail, and cannot be reversed twice. Another
    // company cannot reverse it.
    assert_eq!(
        service
            .reverse(other_company, &ana, first.id())
            .await
            .unwrap_err(),
        ApplicationError::TransferNotFound
    );
    let reversed = service.reverse(company, &ana, first.id()).await.unwrap();
    assert!(reversed.is_reversed());
    assert!(
        store
            .find(company, first.id())
            .await
            .unwrap()
            .unwrap()
            .is_reversed()
    );
    assert_eq!(
        listed_transfers(service, company, &everything).await.len(),
        2
    );
    assert_eq!(store.history(company, first.id()).await.unwrap().len(), 2);
    assert_eq!(
        service
            .reverse(company, &ana, first.id())
            .await
            .unwrap_err(),
        ApplicationError::TransferNotFound
    );
    assert_eq!(store.history(company, first.id()).await.unwrap().len(), 2);
    assert_eq!(store.history(company, second.id()).await.unwrap().len(), 1);
}

fn balance_of(balances: &Balances, account: BankAccountId) -> &AccountBalance {
    balances
        .accounts
        .iter()
        .find(|item| item.account.id() == account)
        .expect("the account is listed")
}

/// Received, paid, transfers in, transfers out and balance, as text.
fn figures(item: &AccountBalance) -> [String; 5] {
    [
        item.received.to_string(),
        item.paid.to_string(),
        item.transfers_in.to_string(),
        item.transfers_out.to_string(),
        item.balance.to_string(),
    ]
}

/// The balance of an account on a day is its opening balance plus what settlements and transfers
/// moved from the opening day through that day, to the cent; what is reversed does not count; before
/// the opening day it is 0; every account is listed, closed ones too; the total adds them up; and
/// another company's accounts and movements never mix in.
#[allow(clippy::too_many_arguments)]
pub async fn adds_balances_up<A, E, T, O, F>(
    balances: &BalanceService<A, E, T, O>,
    entries: &FinancialEntryService<E, F>,
    transfers: &TransferService<T, F>,
    openings: &O,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    A: BankAccountRepository,
    E: FinancialEntryRepository,
    T: TransferRepository,
    O: OpeningRepository,
    F: EntryReferences,
{
    let ana = actor();
    let (main, reserve, closed) = (
        seed.bank_account,
        seed.other_bank_account,
        seed.inactive_bank_account,
    );
    let day = |text: &str| CalendarDate::parse(text).unwrap();

    // With no opening balance and no movement every account is 0, closed ones included.
    let empty = balances.balances(company, day("2026-12-31")).await.unwrap();
    assert_eq!(empty.accounts.len(), 3);
    assert_eq!(empty.total.to_string(), "0");

    // The opening balance: replaced as a whole, nothing recorded when it does not change, refused
    // when it is not cents, for an account that is not the company's, or a day that is not one.
    let opening = balances
        .set_opening(company, &ana, main, "1000", "2026-12-01")
        .await
        .unwrap();
    assert_eq!(opening.balance().to_string(), "1000");
    assert_eq!(openings.history(company, main).await.unwrap().len(), 1);
    balances
        .set_opening(company, &ana, main, "1000.00", "2026-12-01")
        .await
        .unwrap();
    assert_eq!(openings.history(company, main).await.unwrap().len(), 1);
    assert_eq!(
        balances
            .set_opening(company, &ana, main, "1.001", "2026-12-01")
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::InvalidOpeningBalance)
    );
    assert_eq!(
        balances
            .set_opening(company, &ana, main, "1", "2026-13-01")
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::InvalidDate("on"))
    );
    assert_eq!(
        balances
            .set_opening(other_company, &ana, main, "1", "2026-12-01")
            .await
            .unwrap_err(),
        ApplicationError::BankAccountNotFound
    );
    assert_eq!(openings.history(company, main).await.unwrap().len(), 1);
    assert_eq!(openings.find(other_company, main).await.unwrap(), None);
    assert!(
        openings
            .list_by_company(other_company)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        openings.list_by_company(company).await.unwrap(),
        vec![opening.clone()]
    );

    // A receivable settled with interest and discount, a payable, and a receivable settled before
    // the opening day, all in the main account.
    let sale = entries
        .register(
            company,
            &ana,
            entry_draft(seed, "Venda", "2026-12-01", &[("2026-12-10", "100")]),
        )
        .await
        .unwrap();
    entries
        .settle(
            company,
            &ana,
            sale.id(),
            SettlementDraft {
                interest: Some("5".into()),
                discount: Some("2".into()),
                ..settlement_draft(seed, 1, "100")
            },
        )
        .await
        .unwrap();
    let bill = entries
        .register(
            company,
            &ana,
            FinancialEntryDraft {
                kind: EntryKind::Payable,
                ..entry_draft(seed, "Aluguel", "2026-12-01", &[("2026-12-12", "30")])
            },
        )
        .await
        .unwrap();
    let paid = entries
        .settle(
            company,
            &ana,
            bill.id(),
            SettlementDraft {
                settled_on: "2026-12-12".into(),
                ..settlement_draft(seed, 1, "30")
            },
        )
        .await
        .unwrap();
    let early = entries
        .register(
            company,
            &ana,
            entry_draft(seed, "Venda antiga", "2026-11-20", &[("2026-11-30", "10")]),
        )
        .await
        .unwrap();
    entries
        .settle(
            company,
            &ana,
            early.id(),
            SettlementDraft {
                settled_on: "2026-11-30".into(),
                ..settlement_draft(seed, 1, "10")
            },
        )
        .await
        .unwrap();

    // Transfers both ways, and one that is reversed.
    transfers
        .register(
            company,
            &ana,
            transfer_draft(main, reserve, "200", "2026-12-15"),
        )
        .await
        .unwrap();
    transfers
        .register(
            company,
            &ana,
            transfer_draft(reserve, main, "50.25", "2026-12-16"),
        )
        .await
        .unwrap();
    let undone = transfers
        .register(
            company,
            &ana,
            transfer_draft(main, reserve, "25", "2026-12-14"),
        )
        .await
        .unwrap();
    transfers.reverse(company, &ana, undone.id()).await.unwrap();

    // Main: 1000 + (100 + 5 - 2) - 30 - 200 + 50.25; the settlement before its opening day and the
    // reversed transfer do not count. Reserve has no opening balance, so it counts from the start.
    let at_year_end = balances.balances(company, day("2026-12-31")).await.unwrap();
    assert_eq!(
        figures(balance_of(&at_year_end, main)),
        ["103", "30", "50.25", "200", "923.25"]
    );
    assert_eq!(
        balance_of(&at_year_end, main).opening.as_ref(),
        Some(&opening)
    );
    assert_eq!(
        figures(balance_of(&at_year_end, reserve)),
        ["0", "0", "200", "50.25", "149.75"]
    );
    assert_eq!(
        figures(balance_of(&at_year_end, closed)),
        ["0", "0", "0", "0", "0"]
    );
    assert_eq!(at_year_end.total.to_string(), "1073");

    // Earlier days: the day itself counts, and the opening day counts from its start.
    let on_the_twelfth = balances.balances(company, day("2026-12-12")).await.unwrap();
    assert_eq!(
        figures(balance_of(&on_the_twelfth, main)),
        ["103", "30", "0", "0", "1073"]
    );
    let on_the_opening_day = balances.balances(company, day("2026-12-01")).await.unwrap();
    assert_eq!(
        balance_of(&on_the_opening_day, main).balance.to_string(),
        "1000"
    );
    let before_the_opening = balances.balances(company, day("2026-11-30")).await.unwrap();
    assert_eq!(
        figures(balance_of(&before_the_opening, main)),
        ["0", "0", "0", "0", "0"]
    );

    // Another company sees none of it.
    let alien = balances
        .balances(other_company, day("2026-12-31"))
        .await
        .unwrap();
    assert!(alien.accounts.is_empty());
    assert_eq!(alien.total.to_string(), "0");

    // Reversing a settlement takes it out of the balance; a new opening replaces the old one.
    entries
        .reverse(company, &ana, bill.id(), paid.id())
        .await
        .unwrap();
    let after_reversal = balances.balances(company, day("2026-12-31")).await.unwrap();
    assert_eq!(
        balance_of(&after_reversal, main).balance.to_string(),
        "953.25"
    );
    balances
        .set_opening(company, &ana, main, "500", "2026-12-01")
        .await
        .unwrap();
    assert_eq!(openings.history(company, main).await.unwrap().len(), 2);
    let replaced = balances.balances(company, day("2026-12-31")).await.unwrap();
    assert_eq!(balance_of(&replaced, main).balance.to_string(), "453.25");
    assert_eq!(openings.list_by_company(company).await.unwrap().len(), 1);

    // A negative opening balance is allowed: an overdraft.
    balances
        .set_opening(company, &ana, reserve, "-20.5", "2026-01-01")
        .await
        .unwrap();
    let overdrawn = balances.balances(company, day("2026-12-31")).await.unwrap();
    assert_eq!(
        balance_of(&overdrawn, reserve).balance.to_string(),
        "129.25"
    );
    let earlier = balances.balances(company, day("2026-06-30")).await.unwrap();
    assert_eq!(balance_of(&earlier, reserve).balance.to_string(), "-20.5");
}
