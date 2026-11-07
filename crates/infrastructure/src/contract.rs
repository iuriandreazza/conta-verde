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
    ChargeRepository, ChargeService, CollectionService, CollectionSettingsRepository, PageState,
    PaymentEventRepository, PaymentLinkRepository, PaymentPageService, PaymentWebhookService,
    ProviderError, Recorded, WebhookError,
};
use conta_verde_application::{
    CommissionRuleQuery, CommissionRuleRepository, CommissionService, ContractRepository,
    ContractService, Conversion, Refund, SaleQuery, SaleRepository, SaleService,
};
use conta_verde_application::{
    ContractQuery, DocumentReferences, DocumentStock, EmailSender, NumberSort, QuotePage,
    QuoteQuery, QuoteRepository, QuoteService,
};
use conta_verde_application::{
    DunningPlanRepository, DunningService, EmailMessage, InstallmentLinks, MessageSender,
    NotificationRepository, TextMessage,
};
use conta_verde_application::{EntryImportDraft, EntryLookups, SettlementImport};
use conta_verde_application::{
    ImportRow, InvitationRepository, InvitationState, Invited, MemberService, NewAccount,
    NewInvitation, NewUser, PaymentTermQuery, PaymentTermRepository, PaymentTermService,
};
use conta_verde_application::{PriceListQuery, PriceListRepository, PriceListService};
use conta_verde_application::{
    PurchaseOrderPage, PurchaseOrderQuery, PurchaseOrderRepository, PurchaseService,
};
use conta_verde_application::{
    StockMovementQuery, StockMovementRepository, StockService, StockSettingsRepository,
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
use conta_verde_domain::{BillingCycle, CommissionRuleDraft, ContractDraft, ContractStatus};
use conta_verde_domain::{ChargeMethod, ChargeStatus};
use conta_verde_domain::{Decimal, PriceListDraft, PriceListItemDraft, StockSituation};
use conta_verde_domain::{
    EntryOrigin, ImportId, ReturnItemDraft, SaleDraft, SaleReturn, SaleReturnDraft, SaleReturnId,
    SaleStatus, sale_return_creation_changes,
};
use conta_verde_domain::{InstallmentDraft, payment_term_changes, payment_term_creation_changes};
use conta_verde_domain::{LineDraft, QuoteDraft, QuoteStatus};
use conta_verde_domain::{PaymentEvent, PaymentEventId, PaymentEventKind, PaymentEventOutcome};
use conta_verde_domain::{
    PurchaseOrderDraft, PurchaseReceiptDraft, PurchaseReceiptId, PurchaseStatus, ReceiptItemDraft,
};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::{
    FakePaymentProvider, InMemoryCnpjRegistry, InMemoryEmailSender, InMemoryMessageSender,
};

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

/// A user row, for a test that needs somebody to have done something.
pub async fn create_user(pool: &PgPool) -> UserId {
    let id = UserId::generate();
    sqlx::query("INSERT INTO users (id, name, email, password_hash) VALUES ($1, 'Test', $2, 'x')")
        .bind(id.as_uuid())
        .bind(format!("test-{}@example.com", id.as_uuid()))
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

/// A password reset ([ADR 0060]): a link works once, several may be out at once and spending one ends them all, none is made a moment after the last, an
/// expired one does nothing, and spending one ends every session and puts the new password in place of the old.
pub async fn resets_a_password_once<I, S>(auth: &AuthService<I, S>, identities: &I, secrets: &S)
where
    I: IdentityRepository,
    S: SecretService,
{
    let email = format!("ana-{}@example.com", Uuid::new_v4());
    let signed_up = auth.register(draft(&email), "correct horse").await.unwrap();
    let user = signed_up.profile.user.id();
    let hour = Duration::from_secs(3600);
    let new_hash = secrets.hash_password("battery staple").await.unwrap();

    // One link at a time, and none right after another.
    assert!(
        identities
            .create_password_reset(user, "digest-1", hour, Duration::ZERO)
            .await
            .unwrap()
    );
    assert!(
        !identities
            .create_password_reset(user, "digest-2", hour, hour)
            .await
            .unwrap(),
        "too soon after the last one"
    );
    assert_eq!(
        identities.reset_password("digest-2", &new_hash).await,
        Err(RepositoryError::NotFound),
        "what was refused stored nothing"
    );
    // Another request does not kill the link already sent, and spending either ends them all.
    assert!(
        identities
            .create_password_reset(user, "digest-3", hour, Duration::ZERO)
            .await
            .unwrap()
    );

    // Spending the link changes the password and ends the sessions; the link works once.
    let session = auth.login(&email, "correct horse").await.unwrap().token;
    assert_eq!(
        identities
            .reset_password("digest-1", &new_hash)
            .await
            .unwrap(),
        user,
        "it says whose password it was"
    );
    assert_eq!(
        identities
            .find_user(user)
            .await
            .unwrap()
            .unwrap()
            .email()
            .as_str(),
        email
    );
    assert!(
        identities
            .find_user(UserId::generate())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        auth.profile(&session).await.unwrap_err(),
        ApplicationError::Auth(AuthError::Unauthenticated)
    );
    assert_eq!(
        auth.profile(&signed_up.token).await.unwrap_err(),
        ApplicationError::Auth(AuthError::Unauthenticated)
    );
    assert_eq!(
        auth.login(&email, "correct horse").await.unwrap_err(),
        ApplicationError::Auth(AuthError::InvalidCredentials)
    );
    auth.login(&email, "battery staple").await.unwrap();
    for spent in ["digest-1", "digest-3"] {
        assert_eq!(
            identities.reset_password(spent, &new_hash).await,
            Err(RepositoryError::NotFound),
            "{spent}"
        );
    }

    // A link that has expired does nothing.
    assert!(
        identities
            .create_password_reset(user, "digest-4", Duration::ZERO, Duration::ZERO)
            .await
            .unwrap()
    );
    assert_eq!(
        identities.reset_password("digest-4", &new_hash).await,
        Err(RepositoryError::NotFound)
    );
}

/// An address confirmation ([ADR 0063]): unconfirmed at first, several links may be out and none right after another, a
/// link confirms once and its siblings die with it, an expired one does nothing, and an address can be marked without one.
pub async fn confirms_an_email_once<I, S>(auth: &AuthService<I, S>, identities: &I)
where
    I: IdentityRepository,
    S: SecretService,
{
    let hour = Duration::from_secs(3600);
    let email = format!("ana-{}@example.com", Uuid::new_v4());
    let user = auth
        .register(draft(&email), "correct horse")
        .await
        .unwrap()
        .profile
        .user
        .id();
    assert!(!identities.email_verified(user).await.unwrap());

    assert!(
        identities
            .create_email_confirmation(user, "digest-1", hour, Duration::ZERO)
            .await
            .unwrap()
    );
    assert!(
        !identities
            .create_email_confirmation(user, "digest-2", hour, hour)
            .await
            .unwrap(),
        "too soon after the last one"
    );
    assert!(
        identities
            .create_email_confirmation(user, "digest-3", hour, Duration::ZERO)
            .await
            .unwrap()
    );
    assert_eq!(
        identities.confirm_email("digest-2").await,
        Err(RepositoryError::NotFound)
    );
    assert_eq!(identities.confirm_email("digest-1").await.unwrap(), user);
    assert!(identities.email_verified(user).await.unwrap());
    for spent in ["digest-1", "digest-3"] {
        assert_eq!(
            identities.confirm_email(spent).await,
            Err(RepositoryError::NotFound),
            "{spent}"
        );
    }
    assert!(
        auth.login(&email, "correct horse")
            .await
            .unwrap()
            .profile
            .email_verified
    );

    // An expired link does nothing, and an address can be marked without one.
    let other = auth
        .register(
            draft(&format!("bia-{}@example.com", Uuid::new_v4())),
            "correct horse",
        )
        .await
        .unwrap()
        .profile
        .user
        .id();
    assert!(
        identities
            .create_email_confirmation(other, "digest-4", Duration::ZERO, Duration::ZERO)
            .await
            .unwrap()
    );
    assert_eq!(
        identities.confirm_email("digest-4").await,
        Err(RepositoryError::NotFound)
    );
    assert!(!identities.email_verified(other).await.unwrap());
    identities.mark_email_verified(other).await.unwrap();
    assert!(identities.email_verified(other).await.unwrap());
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

pub fn entry_draft(
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

// ---------- cash flow ----------

use conta_verde_application::{CashFlow, CashFlowQuery, Granularity};

fn cash_flow_query(
    from: &str,
    to: &str,
    granularity: Granularity,
    account: Option<BankAccountId>,
) -> CashFlowQuery {
    CashFlowQuery::new(
        CalendarDate::parse(from).unwrap(),
        CalendarDate::parse(to).unwrap(),
        granularity,
        account,
    )
    .unwrap()
}

/// One period as text: start, end, received, paid, transfers in, transfers out, net, new opening
/// balances, closing balance.
fn realized(flow: &CashFlow, index: usize) -> [String; 9] {
    let period = &flow.periods[index];
    [
        period.start.to_string(),
        period.end.to_string(),
        period.received.to_string(),
        period.paid.to_string(),
        period.transfers_in.to_string(),
        period.transfers_out.to_string(),
        period.net.to_string(),
        period.new_opening_balances.to_string(),
        period.closing_balance.to_string(),
    ]
}

/// What is still to come in a period, as text: to receive, to pay, projected balance.
fn forecast(flow: &CashFlow, index: usize) -> [Option<String>; 3] {
    let period = &flow.periods[index];
    [
        period.to_receive.map(|amount| amount.to_string()),
        period.to_pay.map(|amount| amount.to_string()),
        period.projected_balance.map(|amount| amount.to_string()),
    ]
}

fn forecast_of(parts: [&str; 3]) -> [Option<String>; 3] {
    parts.map(|amount| Some(amount.to_owned()))
}

/// The cash flow cuts a span into days or calendar months, adds up what settlements and transfers
/// moved by the day they happened (reversed ones left out, and what is dated before an account's
/// opening day too), runs the balance on from the one before the span, and forecasts what is still
/// open by due date, for every account together but not for one. A span that is not one, or is too
/// long, and an account that is not the company's are refused.
pub async fn adds_the_cash_flow_up<A, E, T, O, F>(
    balances: &BalanceService<A, E, T, O>,
    entries: &FinancialEntryService<E, F>,
    transfers: &TransferService<T, F>,
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

    balances
        .set_opening(company, &ana, main, "1000", "2026-12-01")
        .await
        .unwrap();
    let payable = |description: &str, due: &str, amount: &str| FinancialEntryDraft {
        kind: EntryKind::Payable,
        ..entry_draft(seed, description, "2026-11-01", &[(due, amount)])
    };
    let settle = |entry: EntryId, day: &str, applied: &str| {
        let draft = SettlementDraft {
            settled_on: day.into(),
            ..settlement_draft(seed, 1, applied)
        };
        entries.settle(company, &ana, entry, draft)
    };
    let receivable = |description: &str, due: &str, amount: &str| {
        entry_draft(seed, description, "2026-11-01", &[(due, amount)])
    };

    // Settled: a sale with interest and discount, a bill, a bill paid in part, and a sale settled
    // before the opening day of the account it went into.
    let sale = entries
        .register(company, &ana, receivable("Venda", "2026-12-10", "100"))
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
    let rent = entries
        .register(company, &ana, payable("Aluguel", "2026-12-12", "30"))
        .await
        .unwrap();
    let paid_rent = settle(rent.id(), "2026-12-12", "30").await.unwrap();
    let supplier = entries
        .register(company, &ana, payable("Fornecedor", "2027-01-05", "80"))
        .await
        .unwrap();
    settle(supplier.id(), "2026-12-18", "30").await.unwrap();
    let old = entries
        .register(
            company,
            &ana,
            receivable("Venda antiga", "2026-11-30", "10"),
        )
        .await
        .unwrap();
    settle(old.id(), "2026-11-30", "10").await.unwrap();
    // Open: a receivable due in December, and one that fell due in November and is still open.
    entries
        .register(company, &ana, receivable("A receber", "2026-12-20", "200"))
        .await
        .unwrap();
    entries
        .register(company, &ana, receivable("Atrasada", "2026-11-15", "40"))
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

    // Every account, by month: the transfers cancel out, the opening balance comes in on its day,
    // the forecast is what is open by due date, and the projected balance adds it to the realized one.
    let december_to_january = cash_flow_query("2026-12-01", "2027-01-31", Granularity::Month, None);
    let all = balances
        .cash_flow(company, &december_to_january)
        .await
        .unwrap();
    assert_eq!(all.opening_balance.to_string(), "0");
    assert_eq!(all.periods.len(), 2);
    assert_eq!(
        realized(&all, 0),
        [
            "2026-12-01",
            "2026-12-31",
            "103",
            "60",
            "250.25",
            "250.25",
            "43",
            "1000",
            "1043"
        ]
    );
    assert_eq!(
        realized(&all, 1),
        [
            "2027-01-01",
            "2027-01-31",
            "0",
            "0",
            "0",
            "0",
            "0",
            "0",
            "1043"
        ]
    );
    assert_eq!(forecast(&all, 0), forecast_of(["200", "0", "1283"]));
    assert_eq!(forecast(&all, 1), forecast_of(["0", "50", "1233"]));
    // What fell due before the span and is still open is told apart, and the projection counts it.
    let late = all.past_due.expect("every account asked for");
    assert_eq!(
        (late.receivable.to_string(), late.payable.to_string()),
        ("40".into(), "0".into())
    );

    // What fell due before the span is not in it, and a span before the opening day of the account
    // that has one leaves out what was settled then, but still forecasts.
    let november = balances
        .cash_flow(
            company,
            &cash_flow_query("2026-11-01", "2026-11-30", Granularity::Month, None),
        )
        .await
        .unwrap();
    assert_eq!(realized(&november, 0)[2..], ["0"; 7]);
    assert_eq!(forecast(&november, 0), forecast_of(["40", "0", "40"]));
    let none_late = november.past_due.expect("every account asked for");
    assert_eq!(
        (
            none_late.receivable.to_string(),
            none_late.payable.to_string()
        ),
        ("0".into(), "0".into())
    );

    // Days: the day itself counts, every day of the span is a period, and the balance runs on from
    // the day before. One account has no forecast, because an installment has no account.
    let days = balances
        .cash_flow(
            company,
            &cash_flow_query("2026-12-10", "2026-12-12", Granularity::Day, Some(main)),
        )
        .await
        .unwrap();
    assert_eq!(days.opening_balance.to_string(), "1000");
    assert_eq!(days.periods.len(), 3);
    assert_eq!(
        realized(&days, 0)[2..],
        ["103", "0", "0", "0", "103", "0", "1103"]
    );
    assert_eq!(
        realized(&days, 1)[2..],
        ["0", "0", "0", "0", "0", "0", "1103"]
    );
    assert_eq!(
        realized(&days, 2)[2..],
        ["0", "30", "0", "0", "-30", "0", "1073"]
    );
    assert_eq!(forecast(&days, 0), [None, None, None]);
    assert_eq!(days.past_due, None);

    // The other account has no opening balance: it counts from the start of the span, and what came
    // into it and went out of it is told apart. The opening account counts from its opening day.
    let december = |account| {
        cash_flow_query(
            "2026-12-01",
            "2026-12-31",
            Granularity::Month,
            Some(account),
        )
    };
    let reserve_flow = balances
        .cash_flow(company, &december(reserve))
        .await
        .unwrap();
    assert_eq!(reserve_flow.opening_balance.to_string(), "0");
    assert_eq!(
        realized(&reserve_flow, 0)[2..],
        ["0", "0", "200", "50.25", "149.75", "0", "149.75"]
    );
    let main_flow = balances.cash_flow(company, &december(main)).await.unwrap();
    assert_eq!(
        realized(&main_flow, 0)[2..],
        ["103", "60", "50.25", "200", "-106.75", "1000", "893.25"]
    );
    let closed_flow = balances
        .cash_flow(company, &december(closed))
        .await
        .unwrap();
    assert_eq!(realized(&closed_flow, 0)[2..], ["0"; 7]);

    // A span that starts after the opening day starts from the balance of the day before it, and
    // that balance is the one the balances give.
    let late = balances
        .cash_flow(
            company,
            &cash_flow_query("2026-12-17", "2026-12-17", Granularity::Day, None),
        )
        .await
        .unwrap();
    let before = balances
        .balances(company, CalendarDate::parse("2026-12-16").unwrap())
        .await
        .unwrap();
    assert_eq!(late.opening_balance, before.total);
    assert_eq!(late.opening_balance.to_string(), "1073");

    // Reversing a settlement takes it out of what was realized and gives it back to the forecast.
    entries
        .reverse(company, &ana, rent.id(), paid_rent.id())
        .await
        .unwrap();
    let after = balances
        .cash_flow(company, &december_to_january)
        .await
        .unwrap();
    assert_eq!(
        realized(&after, 0)[2..],
        ["103", "30", "250.25", "250.25", "73", "1000", "1073"]
    );
    assert_eq!(forecast(&after, 0), forecast_of(["200", "30", "1283"]));
    assert_eq!(forecast(&after, 1), forecast_of(["0", "50", "1233"]));

    // Nothing of another company's, and what is not a span or an account is refused.
    let alien = balances
        .cash_flow(
            other_company,
            &cash_flow_query("2026-12-01", "2026-12-31", Granularity::Month, None),
        )
        .await
        .unwrap();
    assert_eq!(realized(&alien, 0)[2..], ["0"; 7]);
    assert_eq!(forecast(&alien, 0), forecast_of(["0", "0", "0"]));
    assert_eq!(
        balances
            .cash_flow(other_company, &december(main))
            .await
            .unwrap_err(),
        ApplicationError::BankAccountNotFound
    );
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    let span = |from: &str, to: &str, granularity| {
        CashFlowQuery::new(day(from), day(to), granularity, None)
    };
    assert!(matches!(
        span("2026-12-02", "2026-12-01", Granularity::Day),
        Err(ApplicationError::InvalidSpan(_))
    ));
    assert!(span("2026-01-01", "2027-02-05", Granularity::Day).is_err());
    assert!(span("2026-01-01", "2027-02-04", Granularity::Day).is_ok());
    assert!(span("2026-01-15", "2031-01-14", Granularity::Month).is_err());
    assert!(span("2026-01-15", "2030-12-31", Granularity::Month).is_ok());
}

// ---------- recurring entries ----------

use conta_verde_application::{RecurrenceQuery, RecurrenceRepository, RecurrenceService};
use conta_verde_domain::{Frequency, RecurrenceDraft, RecurrenceOrigin};

fn recurrence_draft(
    seed: &EntrySeed,
    description: &str,
    frequency: Frequency,
    starts_on: &str,
    ends_on: Option<&str>,
    amount: &str,
) -> RecurrenceDraft {
    RecurrenceDraft {
        kind: EntryKind::Payable,
        description: description.into(),
        party_id: None,
        category_id: seed.category,
        cost_center_id: None,
        amount: amount.into(),
        frequency,
        starts_on: starts_on.into(),
        ends_on: ends_on.map(Into::into),
        active: true,
    }
}

/// The entries of the company that came from the recurrence, by day, as `(day, amount)`.
async fn generated_by<E: FinancialEntryRepository>(
    entries: &E,
    company: CompanyId,
    recurrence: conta_verde_domain::RecurrenceId,
) -> Vec<(String, String)> {
    let mut found: Vec<(CalendarDate, String)> = entries
        .list_by_company(company, &FinancialEntryQuery::default(), None, 500)
        .await
        .unwrap()
        .iter()
        .filter_map(|entry| {
            let Some(EntryOrigin::Recurrence(origin)) = entry.origin() else {
                return None;
            };
            if origin.recurrence_id != recurrence {
                return None;
            }
            // An occurrence is an entry with one installment that falls due on its day.
            assert_eq!(entry.installments().len(), 1);
            assert_eq!(entry.installments()[0].due_date(), origin.occurrence_on);
            assert_eq!(entry.competence_date(), origin.occurrence_on);
            Some((origin.occurrence_on, entry.total().to_string()))
        })
        .collect();
    found.sort();
    found
        .into_iter()
        .map(|(day, amount)| (day.to_string(), amount))
        .collect()
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(day, amount)| ((*day).to_owned(), (*amount).to_owned()))
        .collect()
}

/// A recurrence is a register of its own and generates ordinary entries two months ahead, once per
/// occurrence day however often the job runs, from whenever it starts to wherever it ends, never while
/// paused, and in the name of whoever saved it. Changing it changes only what is generated from then
/// on. What it points at has to be usable, and one that stops being usable is skipped by the job without
/// stopping the others. Nothing of another company's shows, and what is refused leaves nothing behind.
#[allow(clippy::too_many_arguments)]
pub async fn generates_recurring_entries_ahead<R, E, F, C>(
    service: &RecurrenceService<R, E, F>,
    store: &R,
    entries: &E,
    categories: &C,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    R: RecurrenceRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    C: CategoryRepository,
{
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    let today = day("2026-11-06");

    // Registering generates what falls due up to 60 days ahead: this one starts a day before today.
    let rent = service
        .register(
            company,
            &ana,
            recurrence_draft(
                seed,
                "Aluguel",
                Frequency::Monthly,
                "2026-11-05",
                None,
                "2500",
            ),
            today,
        )
        .await
        .unwrap();
    assert_eq!(rent.generated_through(), Some(day("2027-01-05")));
    assert_eq!(
        generated_by(entries, company, rent.id()).await,
        pairs(&[
            ("2026-11-05", "2500"),
            ("2026-12-05", "2500"),
            ("2027-01-05", "2500")
        ])
    );
    let first = entries
        .list_by_company(company, &FinancialEntryQuery::default(), None, 500)
        .await
        .unwrap()
        .into_iter()
        .find(|entry| entry.origin().is_some())
        .unwrap();
    assert_eq!(first.description(), "Aluguel");
    assert_eq!(first.kind(), EntryKind::Payable);
    assert_eq!(first.category_id(), seed.category);
    // Made in the name of whoever saved the recurrence, and says so in its trail.
    assert_eq!(entry_trail(entries, company, &first).await, 1);

    // The job again, the same day, changes nothing; a later day adds only what became due.
    service.generate_all_due(today).await.unwrap();
    assert_eq!(generated_by(entries, company, rent.id()).await.len(), 3);
    service.generate_all_due(day("2026-12-10")).await.unwrap();
    assert_eq!(
        generated_by(entries, company, rent.id())
            .await
            .last()
            .unwrap(),
        &("2027-02-05".to_owned(), "2500".to_owned())
    );

    // Changing the amount touches nothing already generated, and applies from the next occurrence.
    let raised = service
        .update(
            company,
            &ana,
            rent.id(),
            recurrence_draft(
                seed,
                "Aluguel",
                Frequency::Monthly,
                "2026-11-05",
                None,
                "3000",
            ),
            day("2026-12-10"),
        )
        .await
        .unwrap();
    assert_eq!(raised.amount().to_string(), "3000");
    service.generate_all_due(day("2027-01-10")).await.unwrap();
    assert_eq!(
        generated_by(entries, company, rent.id()).await,
        pairs(&[
            ("2026-11-05", "2500"),
            ("2026-12-05", "2500"),
            ("2027-01-05", "2500"),
            ("2027-02-05", "2500"),
            ("2027-03-05", "3000"),
        ])
    );
    // The same body again changes nothing and records nothing.
    service
        .update(
            company,
            &ana,
            rent.id(),
            recurrence_draft(
                seed,
                "Aluguel",
                Frequency::Monthly,
                "2026-11-05",
                None,
                "3000",
            ),
            day("2027-01-10"),
        )
        .await
        .unwrap();
    assert_eq!(store.history(company, rent.id()).await.unwrap().len(), 2);

    // It ends where it says: four Mondays, and then the job leaves it alone.
    let weekly = service
        .register(
            company,
            &ana,
            RecurrenceDraft {
                kind: EntryKind::Receivable,
                ..recurrence_draft(
                    seed,
                    "Mensalidade",
                    Frequency::Weekly,
                    "2026-11-02",
                    Some("2026-11-23"),
                    "150.5",
                )
            },
            today,
        )
        .await
        .unwrap();
    assert_eq!(
        generated_by(entries, company, weekly.id()).await,
        pairs(&[
            ("2026-11-02", "150.5"),
            ("2026-11-09", "150.5"),
            ("2026-11-16", "150.5"),
            ("2026-11-23", "150.5")
        ])
    );
    assert_eq!(weekly.generated_through(), Some(day("2026-11-23")));
    service.generate_all_due(day("2027-06-01")).await.unwrap();
    assert_eq!(generated_by(entries, company, weekly.id()).await.len(), 4);

    // One that started in the past catches up with what fell due since.
    let backfilled = service
        .register(
            company,
            &ana,
            recurrence_draft(
                seed,
                "Internet",
                Frequency::Monthly,
                "2026-09-30",
                None,
                "99.9",
            ),
            today,
        )
        .await
        .unwrap();
    assert_eq!(
        generated_by(entries, company, backfilled.id())
            .await
            .iter()
            .map(|(day, _)| day.as_str())
            .collect::<Vec<_>>(),
        ["2026-09-30", "2026-10-30", "2026-11-30", "2026-12-30"]
    );

    // Paused, it generates nothing; brought back, it catches up.
    let paused = service
        .register(
            company,
            &ana,
            RecurrenceDraft {
                active: false,
                ..recurrence_draft(
                    seed,
                    "Seguro",
                    Frequency::Yearly,
                    "2026-11-20",
                    None,
                    "1200",
                )
            },
            today,
        )
        .await
        .unwrap();
    assert!(generated_by(entries, company, paused.id()).await.is_empty());
    service.generate_all_due(day("2026-11-07")).await.unwrap();
    service
        .update(
            company,
            &ana,
            paused.id(),
            recurrence_draft(
                seed,
                "Seguro",
                Frequency::Yearly,
                "2026-11-20",
                None,
                "1200",
            ),
            today,
        )
        .await
        .unwrap();
    assert_eq!(
        generated_by(entries, company, paused.id()).await,
        pairs(&[("2026-11-20", "1200")])
    );

    // The store itself refuses a second entry for the same occurrence, and leaves no trace of it.
    let origin = RecurrenceOrigin {
        recurrence_id: paused.id(),
        occurrence_on: day("2026-11-20"),
    };
    let twin = new_entry(
        company,
        entry_draft(seed, "Seguro", "2026-11-20", &[("2026-11-20", "1200")]),
    )
    .with_origin(EntryOrigin::Recurrence(origin));
    let record = entry_record(&twin, AuditAction::Created, entry_creation_changes(&twin));
    assert_eq!(
        entries.save(&twin, &record).await.unwrap_err(),
        RepositoryError::DuplicateOrigin
    );
    assert_eq!(entry_trail(entries, company, &twin).await, 0);
    let elsewhere = RecurrenceOrigin {
        occurrence_on: day("2026-11-21"),
        ..origin
    };
    let fresh = new_entry(
        company,
        entry_draft(seed, "Seguro", "2026-11-21", &[("2026-11-21", "1200")]),
    )
    .with_origin(EntryOrigin::Recurrence(elsewhere));
    let record = entry_record(&fresh, AuditAction::Created, entry_creation_changes(&fresh));
    entries.save(&fresh, &record).await.unwrap();
    assert_eq!(
        entries
            .find(company, fresh.id())
            .await
            .unwrap()
            .unwrap()
            .origin(),
        Some(EntryOrigin::Recurrence(elsewhere))
    );

    // What it points at has to be usable, and what is refused leaves nothing behind.
    let before = service
        .list(company, &RecurrenceQuery::default(), None, Some(100))
        .await
        .unwrap()
        .items
        .len();
    for (draft, error) in [
        (
            RecurrenceDraft {
                category_id: seed.inactive_category,
                ..recurrence_draft(seed, "X", Frequency::Monthly, "2026-12-01", None, "1")
            },
            ApplicationError::UnusableReference("category"),
        ),
        (
            RecurrenceDraft {
                category_id: seed.transfer_category,
                ..recurrence_draft(seed, "X", Frequency::Monthly, "2026-12-01", None, "1")
            },
            ApplicationError::UnusableReference("category"),
        ),
        (
            RecurrenceDraft {
                cost_center_id: Some(seed.inactive_cost_center),
                ..recurrence_draft(seed, "X", Frequency::Monthly, "2026-12-01", None, "1")
            },
            ApplicationError::UnusableReference("cost center"),
        ),
        (
            RecurrenceDraft {
                party_id: Some(PartyId::generate()),
                ..recurrence_draft(seed, "X", Frequency::Monthly, "2026-12-01", None, "1")
            },
            ApplicationError::UnusableReference("party"),
        ),
        (
            recurrence_draft(seed, "X", Frequency::Monthly, "2026-12-01", None, "0"),
            ApplicationError::Domain(DomainError::InvalidRecurrenceAmount),
        ),
        (
            recurrence_draft(
                seed,
                "X",
                Frequency::Monthly,
                "2026-12-01",
                Some("2026-11-30"),
                "1",
            ),
            ApplicationError::Domain(DomainError::RecurrenceEndsBeforeItStarts),
        ),
    ] {
        assert_eq!(
            service
                .register(company, &ana, draft, today)
                .await
                .unwrap_err(),
            error
        );
    }
    assert_eq!(
        service
            .list(company, &RecurrenceQuery::default(), None, Some(100))
            .await
            .unwrap()
            .items
            .len(),
        before
    );

    // Nothing of another company's: it is not found, listed or read, and its entries are not made.
    assert_eq!(
        service.get(other_company, rent.id()).await.unwrap_err(),
        ApplicationError::RecurrenceNotFound
    );
    assert_eq!(
        service.history(other_company, rent.id()).await.unwrap_err(),
        ApplicationError::RecurrenceNotFound
    );
    assert_eq!(
        service
            .update(
                other_company,
                &ana,
                rent.id(),
                recurrence_draft(seed, "Aluguel", Frequency::Monthly, "2026-11-05", None, "1"),
                today
            )
            .await
            .unwrap_err(),
        ApplicationError::RecurrenceNotFound
    );
    assert!(
        service
            .list(other_company, &RecurrenceQuery::default(), None, Some(100))
            .await
            .unwrap()
            .items
            .is_empty()
    );

    // The list is narrowed by kind, state and text, in order of description.
    let names = |page: conta_verde_application::RecurrencePage| {
        page.items
            .iter()
            .map(|item| item.description().to_owned())
            .collect::<Vec<_>>()
    };
    let all = service
        .list(company, &RecurrenceQuery::default(), None, Some(100))
        .await
        .unwrap();
    assert_eq!(names(all), ["Aluguel", "Internet", "Mensalidade", "Seguro"]);
    let receivable = RecurrenceQuery::default().of_kind(EntryKind::Receivable);
    assert_eq!(
        names(
            service
                .list(company, &receivable, None, Some(100))
                .await
                .unwrap()
        ),
        ["Mensalidade"]
    );
    let searched = RecurrenceQuery::default().searching("NET");
    assert_eq!(
        names(
            service
                .list(company, &searched, None, Some(100))
                .await
                .unwrap()
        ),
        ["Internet"]
    );
    let page = service
        .list(company, &RecurrenceQuery::default(), None, Some(2))
        .await
        .unwrap();
    assert_eq!(names(page.clone()), ["Aluguel", "Internet"]);
    let rest = service
        .list(
            company,
            &RecurrenceQuery::default(),
            page.next.as_ref(),
            Some(2),
        )
        .await
        .unwrap();
    assert_eq!(names(rest), ["Mensalidade", "Seguro"]);

    // A category that stops being usable stops that recurrence only; the job says so and goes on.
    let stalled = service
        .register(
            company,
            &ana,
            RecurrenceDraft {
                category_id: seed.other_category,
                ..recurrence_draft(
                    seed,
                    "Contador",
                    Frequency::Monthly,
                    "2027-03-01",
                    None,
                    "800",
                )
            },
            today,
        )
        .await
        .unwrap();
    // Nothing falls due yet, so only the horizon is noted.
    assert_eq!(stalled.generated_through(), Some(day("2027-01-05")));
    let current = categories
        .find(company, seed.other_category)
        .await
        .unwrap()
        .unwrap();
    let closed = current.changed(current.name(), false).unwrap();
    let record = category_entry(
        &closed,
        AuditAction::Updated,
        category_changes(&current, &closed),
    );
    categories.update(&closed, &record).await.unwrap();
    let report = service.generate_all_due(day("2027-02-15")).await.unwrap();
    // The job reads every company's recurrences, so others left in a shared database may fail too.
    assert!(report.failed >= 1);
    assert!(
        generated_by(entries, company, stalled.id())
            .await
            .is_empty()
    );
    assert_eq!(
        service
            .get(company, stalled.id())
            .await
            .unwrap()
            .generated_through(),
        Some(day("2027-01-05"))
    );
    // A recurrence can still be paused after its category was closed.
    service
        .update(
            company,
            &ana,
            stalled.id(),
            RecurrenceDraft {
                active: false,
                category_id: seed.other_category,
                ..recurrence_draft(
                    seed,
                    "Contador",
                    Frequency::Monthly,
                    "2027-03-01",
                    None,
                    "800",
                )
            },
            day("2027-02-15"),
        )
        .await
        .unwrap();
    service.generate_all_due(day("2027-02-15")).await.unwrap();

    // Brought back while its category is still closed, it is saved all the same: the entries cannot be
    // made, and the job tries again. It goes on from today, so what fell due while it was paused is not
    // made up once the category is usable again.
    let revived = service
        .update(
            company,
            &ana,
            stalled.id(),
            RecurrenceDraft {
                category_id: seed.other_category,
                ..recurrence_draft(
                    seed,
                    "Contador",
                    Frequency::Monthly,
                    "2027-03-01",
                    None,
                    "800",
                )
            },
            day("2027-02-15"),
        )
        .await
        .unwrap();
    assert!(revived.is_active());
    assert_eq!(revived.generated_through(), Some(day("2027-02-14")));
    assert!(
        generated_by(entries, company, stalled.id())
            .await
            .is_empty()
    );
}

// ---------- statement lines and reconciliation ----------

use conta_verde_application::{
    EntryFromLine, ReconciliationService, StatementLineQuery, StatementLineRepository, Suggestion,
};
use conta_verde_domain::{LineMatch, LineStatus, StatementLineDraft};

fn line_draft(
    day: &str,
    amount: &str,
    description: &str,
    bank_ref: Option<&str>,
) -> StatementLineDraft {
    StatementLineDraft {
        posted_on: day.into(),
        amount: amount.into(),
        description: description.into(),
        bank_ref: bank_ref.map(Into::into),
    }
}

fn file_of(lines: Vec<StatementLineDraft>) -> Vec<ImportRow<StatementLineDraft>> {
    lines
        .into_iter()
        .enumerate()
        .map(|(index, draft)| ImportRow {
            row: index + 1,
            draft,
        })
        .collect()
}

/// What can be matched to a line, as text: `settlement:<description>` or `transfer`.
fn suggested(suggestions: &[Suggestion]) -> Vec<String> {
    suggestions
        .iter()
        .map(|suggestion| match suggestion {
            Suggestion::Settlement {
                entry_description, ..
            } => format!("settlement:{entry_description}"),
            Suggestion::Transfer(_) => "transfer".to_owned(),
        })
        .collect()
}

/// A statement is imported once however often it is sent, to an active account of the company; lines are
/// suggested the settlements and transfers that fit them by account, amount, direction and day, matched
/// to at most one of them each (and each of them to at most one line), unmatched, ignored and restored,
/// and made into entries that are settled on the day; nothing of another company's shows, and what is
/// refused leaves nothing behind.
#[allow(clippy::too_many_arguments)]
pub async fn reconciles_statement_lines<S, E, T, F>(
    service: &ReconciliationService<S, E, T, F>,
    store: &S,
    entries: &FinancialEntryService<E, F>,
    transfers: &TransferService<T, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    S: StatementLineRepository,
    E: FinancialEntryRepository,
    T: TransferRepository,
    F: EntryReferences,
{
    let ana = actor();
    let (main, reserve) = (seed.bank_account, seed.other_bank_account);
    let statement = || {
        vec![
            line_draft("2026-11-05", "1250.50", "PIX  RECEBIDO Maria", Some("F1")),
            line_draft("2026-11-06", "-30", "TARIFA", Some("F2")),
            line_draft("2026-11-07", "-5.5", "CAFE", None),
            line_draft("2026-11-07", "-5.5", "CAFE", None),
            line_draft("2026-11-08", "200", "TED", Some("F4")),
        ]
    };
    let all = StatementLineQuery::default();
    let listed = |query: StatementLineQuery| async move {
        service
            .list(company, &query, None, Some(100))
            .await
            .unwrap()
            .items
    };

    // Nothing is written by a dry run, or by a file that has a line that is refused; a closed account and
    // another company's are refused outright.
    let dry = service
        .import(company, &ana, main, file_of(statement()), true)
        .await
        .unwrap();
    assert_eq!((dry.valid, dry.created), (5, 0));
    assert!(listed(all.clone()).await.is_empty());
    let mut broken = statement();
    broken.push(line_draft("2026-11-09", "0", "NADA", None));
    let refused = service
        .import(company, &ana, main, file_of(broken), false)
        .await
        .unwrap();
    assert_eq!((refused.created, refused.refused.len()), (0, 1));
    assert_eq!(refused.refused[0].row, 6);
    assert!(listed(all.clone()).await.is_empty());
    assert_eq!(
        service
            .import(
                company,
                &ana,
                seed.inactive_bank_account,
                file_of(statement()),
                false
            )
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("account")
    );
    assert_eq!(
        service
            .import(other_company, &ana, main, file_of(statement()), false)
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("account")
    );

    // Sent once, all of it is in; sent again, none is added, and two identical lines stay two.
    let imported = service
        .import(company, &ana, main, file_of(statement()), false)
        .await
        .unwrap();
    assert_eq!((imported.created, imported.skipped.len()), (5, 0));
    let again = service
        .import(company, &ana, main, file_of(statement()), false)
        .await
        .unwrap();
    assert_eq!((again.created, again.skipped.len()), (0, 5));
    let mut overlapping = vec![line_draft(
        "2026-11-05",
        "1250.50",
        "PIX RECEBIDO Maria",
        Some("F1"),
    )];
    overlapping.push(line_draft("2026-11-05", "1250.50", "OUTRO", Some("F5")));
    let overlap = service
        .import(company, &ana, main, file_of(overlapping), false)
        .await
        .unwrap();
    assert_eq!((overlap.created, overlap.skipped.len()), (1, 1));
    let lines = listed(all.clone()).await;
    assert_eq!(lines.len(), 6);
    // Newest day first; the description is as the bank wrote it, white space collapsed.
    assert_eq!(lines[0].posted_on().to_string(), "2026-11-08");
    let by = |description: &str| {
        lines
            .iter()
            .filter(|line| line.description() == description)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(by("PIX RECEBIDO Maria").len(), 1);
    assert_eq!(by("CAFE").len(), 2);
    let (pix, fee, ted, other) = (
        by("PIX RECEBIDO Maria").remove(0),
        by("TARIFA").remove(0),
        by("TED").remove(0),
        by("OUTRO").remove(0),
    );
    assert_eq!(
        (pix.status(), pix.amount().to_string()),
        (LineStatus::Pending, "1250.5".to_owned())
    );
    assert_eq!(store.history(company, pix.id()).await.unwrap().len(), 1);

    // The books: a sale and a bill settled near the lines, ones that do not fit, and a transfer.
    let register = |kind: EntryKind, description: &str, amount: &str| {
        entries.register(
            company,
            &ana,
            FinancialEntryDraft {
                kind,
                ..entry_draft(seed, description, "2026-11-01", &[("2026-11-04", amount)])
            },
        )
    };
    let settle = |entry: EntryId, day: &str, account: BankAccountId, amount: &str| {
        entries.settle(
            company,
            &ana,
            entry,
            SettlementDraft {
                settled_on: day.into(),
                bank_account_id: account,
                ..settlement_draft(seed, 1, amount)
            },
        )
    };
    let sale = register(EntryKind::Receivable, "Venda da Maria", "1250.50")
        .await
        .unwrap();
    let sale_settled = settle(sale.id(), "2026-11-04", main, "1250.50")
        .await
        .unwrap();
    let late_sale = register(EntryKind::Receivable, "Venda tardia", "1250.50")
        .await
        .unwrap();
    settle(late_sale.id(), "2026-11-20", main, "1250.50")
        .await
        .unwrap();
    let elsewhere = register(
        EntryKind::Receivable,
        "Venda no caixa da reserva",
        "1250.50",
    )
    .await
    .unwrap();
    settle(elsewhere.id(), "2026-11-05", reserve, "1250.50")
        .await
        .unwrap();
    let fee_entry = register(EntryKind::Payable, "Tarifa do banco", "30")
        .await
        .unwrap();
    let fee_settled = settle(fee_entry.id(), "2026-11-06", main, "30")
        .await
        .unwrap();
    let wrong_way = register(EntryKind::Receivable, "Recebimento de 30", "30")
        .await
        .unwrap();
    let wrong_way_settled = settle(wrong_way.id(), "2026-11-06", main, "30")
        .await
        .unwrap();
    let moved = transfers
        .register(
            company,
            &ana,
            transfer_draft(reserve, main, "200", "2026-11-08"),
        )
        .await
        .unwrap();

    // What is suggested fits the account, the amount, the direction and five days; nearest first.
    assert_eq!(
        suggested(&service.suggestions(company, pix.id()).await.unwrap()),
        ["settlement:Venda da Maria"]
    );
    assert_eq!(
        suggested(&service.suggestions(company, fee.id()).await.unwrap()),
        ["settlement:Tarifa do banco"]
    );
    assert_eq!(
        suggested(&service.suggestions(company, ted.id()).await.unwrap()),
        ["transfer"]
    );
    assert!(
        service
            .suggestions(company, by("CAFE")[0].id())
            .await
            .unwrap()
            .is_empty()
    );

    // A line is matched to what fits, not to what does not, and to one thing; a thing, to one line.
    assert_eq!(
        service
            .match_line(
                company,
                &ana,
                fee.id(),
                LineMatch::Settlement(wrong_way_settled.id())
            )
            .await
            .unwrap_err(),
        ApplicationError::MatchDoesNotFit
    );
    assert_eq!(
        service
            .match_line(company, &ana, pix.id(), LineMatch::Transfer(moved.id()))
            .await
            .unwrap_err(),
        ApplicationError::MatchDoesNotFit
    );
    assert_eq!(
        service.get(company, pix.id()).await.unwrap().status(),
        LineStatus::Pending
    );
    let matched = service
        .match_line(
            company,
            &ana,
            pix.id(),
            LineMatch::Settlement(sale_settled.id()),
        )
        .await
        .unwrap();
    assert_eq!(
        (matched.status(), matched.matched()),
        (
            LineStatus::Matched,
            Some(LineMatch::Settlement(sale_settled.id()))
        )
    );
    assert_eq!(
        service
            .match_line(
                company,
                &ana,
                pix.id(),
                LineMatch::Settlement(sale_settled.id())
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::LineIsNotPending)
    );
    assert_eq!(
        service
            .match_line(
                company,
                &ana,
                other.id(),
                LineMatch::Settlement(sale_settled.id())
            )
            .await
            .unwrap_err(),
        ApplicationError::AlreadyMatched
    );
    assert!(
        service
            .suggestions(company, other.id())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .suggestions(company, pix.id())
            .await
            .unwrap()
            .is_empty()
    );
    service
        .match_line(
            company,
            &ana,
            fee.id(),
            LineMatch::Settlement(fee_settled.id()),
        )
        .await
        .unwrap();
    service
        .match_line(company, &ana, ted.id(), LineMatch::Transfer(moved.id()))
        .await
        .unwrap();

    // Unmatching frees both: the line is pending and the settlement is suggested again, and every move is
    // in the trail.
    let freed = service.unmatch(company, &ana, pix.id()).await.unwrap();
    assert_eq!(
        (freed.status(), freed.matched()),
        (LineStatus::Pending, None)
    );
    assert_eq!(
        suggested(&service.suggestions(company, other.id()).await.unwrap()),
        ["settlement:Venda da Maria"]
    );
    assert_eq!(
        service.unmatch(company, &ana, pix.id()).await.unwrap_err(),
        ApplicationError::Domain(DomainError::LineIsNotMatched)
    );
    service
        .match_line(
            company,
            &ana,
            pix.id(),
            LineMatch::Settlement(sale_settled.id()),
        )
        .await
        .unwrap();
    assert_eq!(service.history(company, pix.id()).await.unwrap().len(), 4);

    // Ignored lines leave the reconciliation, and come back.
    let coffees = by("CAFE");
    let ignored = service
        .ignore(company, &ana, coffees[0].id())
        .await
        .unwrap();
    assert_eq!(ignored.status(), LineStatus::Ignored);
    assert_eq!(
        service
            .ignore(company, &ana, coffees[0].id())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::LineIsNotPending)
    );
    assert_eq!(
        service
            .restore(company, &ana, coffees[1].id())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::LineIsNotIgnored)
    );
    assert_eq!(
        listed(StatementLineQuery::default().with_status(LineStatus::Ignored))
            .await
            .len(),
        1
    );
    assert_eq!(
        listed(StatementLineQuery::default().with_status(LineStatus::Matched))
            .await
            .len(),
        3
    );
    assert_eq!(
        listed(StatementLineQuery::default().of_account(reserve))
            .await
            .len(),
        0
    );
    assert_eq!(
        service
            .restore(company, &ana, coffees[0].id())
            .await
            .unwrap()
            .status(),
        LineStatus::Pending
    );
    let page = service.list(company, &all, None, Some(4)).await.unwrap();
    assert_eq!(page.items.len(), 4);
    let rest = service
        .list(company, &all, page.next.as_ref(), Some(4))
        .await
        .unwrap();
    assert_eq!(rest.items.len(), 2);
    assert!(rest.next.is_none());

    // An entry made out of a line is settled on the day in its account, and the line is matched to it; a
    // category that cannot be used stops it before anything is written.
    let choice = |category| EntryFromLine {
        category_id: category,
        party_id: None,
        cost_center_id: None,
        payment_method_id: None,
        description: None,
    };
    assert_eq!(
        service
            .create_entry(
                company,
                &ana,
                coffees[1].id(),
                choice(seed.inactive_category)
            )
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("category")
    );
    assert_eq!(
        service
            .get(company, coffees[1].id())
            .await
            .unwrap()
            .status(),
        LineStatus::Pending
    );
    let (made, settlement) = service
        .create_entry(
            company,
            &ana,
            coffees[1].id(),
            EntryFromLine {
                description: Some("Café da manhã".into()),
                ..choice(seed.category)
            },
        )
        .await
        .unwrap();
    assert_eq!(made.matched(), Some(LineMatch::Settlement(settlement.id())));
    assert_eq!(settlement.cash().to_string(), "5.5");
    assert_eq!(settlement.settled_on().to_string(), "2026-11-07");
    assert_eq!(settlement.bank_account_id(), main);
    let created = entries.get(company, settlement.entry_id()).await.unwrap();
    assert_eq!(
        (
            created.kind(),
            created.description(),
            created.total().to_string()
        ),
        (EntryKind::Payable, "Café da manhã", "5.5".to_owned())
    );
    assert_eq!(
        service
            .create_entry(company, &ana, coffees[1].id(), choice(seed.category))
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::LineIsNotPending)
    );

    // A transfer is a line in each of two statements: matched once in each account, and not twice in one.
    let reserve_file = vec![
        line_draft("2026-11-08", "-200", "TED ENVIADO", Some("R1")),
        line_draft("2026-11-08", "-200", "TED DE NOVO", Some("R2")),
    ];
    let reserve_import = service
        .import(company, &ana, reserve, file_of(reserve_file), false)
        .await
        .unwrap();
    assert_eq!(reserve_import.created, 2);
    let reserve_lines = listed(StatementLineQuery::default().of_account(reserve)).await;
    let sent_line = reserve_lines
        .iter()
        .find(|line| line.description() == "TED ENVIADO")
        .unwrap();
    let repeated = reserve_lines
        .iter()
        .find(|line| line.description() == "TED DE NOVO")
        .unwrap();
    assert_eq!(
        suggested(&service.suggestions(company, sent_line.id()).await.unwrap()),
        ["transfer"]
    );
    service
        .match_line(
            company,
            &ana,
            sent_line.id(),
            LineMatch::Transfer(moved.id()),
        )
        .await
        .unwrap();
    assert!(
        service
            .suggestions(company, repeated.id())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service
            .match_line(
                company,
                &ana,
                repeated.id(),
                LineMatch::Transfer(moved.id())
            )
            .await
            .unwrap_err(),
        ApplicationError::AlreadyMatched
    );
    // The line in the other account is still matched to it.
    assert_eq!(
        service.get(company, ted.id()).await.unwrap().matched(),
        Some(LineMatch::Transfer(moved.id()))
    );

    // A move made from a line that someone else has moved since is refused by the store.
    let stale = service.get(company, other.id()).await.unwrap();
    let moved_on = stale.ignored().unwrap();
    let audit = AuditEntry {
        company_id: company,
        actor: ana.clone(),
        entity: AuditEntity::StatementLine,
        entity_id: stale.id().as_uuid(),
        action: AuditAction::Updated,
        changes: conta_verde_domain::statement_line_changes(&stale, &moved_on),
    };
    assert_eq!(
        store
            .update(&moved_on, LineStatus::Matched, &audit)
            .await
            .unwrap_err(),
        RepositoryError::LineChanged
    );
    assert_eq!(
        service.get(company, other.id()).await.unwrap().status(),
        LineStatus::Pending
    );
    store
        .update(&moved_on, LineStatus::Pending, &audit)
        .await
        .unwrap();
    assert_eq!(
        service.get(company, other.id()).await.unwrap().status(),
        LineStatus::Ignored
    );

    // Nothing of another company's.
    assert_eq!(
        service.get(other_company, pix.id()).await.unwrap_err(),
        ApplicationError::StatementLineNotFound
    );
    assert_eq!(
        service.history(other_company, pix.id()).await.unwrap_err(),
        ApplicationError::StatementLineNotFound
    );
    assert_eq!(
        service
            .ignore(other_company, &ana, coffees[0].id())
            .await
            .unwrap_err(),
        ApplicationError::StatementLineNotFound
    );
    assert!(
        service
            .list(other_company, &all, None, Some(100))
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(store.find(other_company, pix.id()).await.unwrap(), None);
}

// ---------- reports of the entries ----------

use conta_verde_application::{OverdueReport, ReportBasis, ReportGroup, ReportRow};

/// A report row as text: the group, receivable, payable and the number of entries.
fn report_rows(rows: &[ReportRow]) -> Vec<(String, String, String, u64)> {
    rows.iter()
        .map(|row| {
            (
                row.id.map_or("-".to_owned(), |id| id.to_string()),
                row.receivable.to_string(),
                row.payable.to_string(),
                row.entries,
            )
        })
        .collect()
}

fn overdue_of(report: &OverdueReport) -> (Vec<String>, String, String, u64) {
    (
        report
            .items
            .iter()
            .map(|item| {
                format!(
                    "{} {} {}",
                    item.due_date,
                    item.description,
                    item.amount - item.settled
                )
            })
            .collect(),
        report.receivable.to_string(),
        report.payable.to_string(),
        report.count,
    )
}

/// The entries are added up by category, cost center and party over a span, by the day of competence (their
/// totals) or by due day (their installments), biggest first and what has no cost center or party last; the
/// overdue list has what fell due before a day and still has something left, oldest first, cut at a limit
/// with the totals of all, and reversing a settlement gives the amount back; nothing of another company's
/// shows, and a span that is not one is refused.
pub async fn reports_the_entries<R, F>(
    service: &FinancialEntryService<R, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    R: FinancialEntryRepository,
    F: EntryReferences,
{
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    let draft = |kind: EntryKind,
                 description: &str,
                 category: CategoryId,
                 competence: &str,
                 parts: &[(&str, &str)]| {
        FinancialEntryDraft {
            kind,
            category_id: category,
            ..entry_draft(seed, description, competence, parts)
        }
    };
    let with_cost_center = |draft: FinancialEntryDraft| FinancialEntryDraft {
        cost_center_id: Some(seed.cost_center),
        ..draft
    };
    let with_party = |draft: FinancialEntryDraft| FinancialEntryDraft {
        party_id: Some(seed.party),
        ..draft
    };
    let sale = service
        .register(
            company,
            &ana,
            with_party(with_cost_center(draft(
                EntryKind::Receivable,
                "Venda 1",
                seed.category,
                "2026-10-10",
                &[("2026-10-20", "100"), ("2026-11-20", "50.5")],
            ))),
        )
        .await
        .unwrap();
    let rent = service
        .register(
            company,
            &ana,
            draft(
                EntryKind::Payable,
                "Aluguel",
                seed.other_category,
                "2026-10-05",
                &[("2026-10-05", "2500")],
            ),
        )
        .await
        .unwrap();
    service
        .register(
            company,
            &ana,
            draft(
                EntryKind::Receivable,
                "Venda 2",
                seed.category,
                "2026-11-02",
                &[("2026-11-12", "300")],
            ),
        )
        .await
        .unwrap();
    service
        .register(
            company,
            &ana,
            with_cost_center(draft(
                EntryKind::Payable,
                "Material",
                seed.category,
                "2026-09-30",
                &[("2026-10-30", "40")],
            )),
        )
        .await
        .unwrap();
    let settle = |entry: EntryId, installment: u16, applied: &str| {
        service.settle(
            company,
            &ana,
            entry,
            settlement_draft(seed, installment, applied),
        )
    };
    settle(sale.id(), 1, "100").await.unwrap();
    let part = settle(rent.id(), 1, "1000").await.unwrap();

    let october = (day("2026-10-01"), day("2026-10-31"));
    let report = |group, basis, span: (CalendarDate, CalendarDate)| {
        service.report(company, group, basis, span.0, span.1)
    };
    let id_of = |uuid: uuid::Uuid| uuid.to_string();

    // By category: by competence, what was entered in October, by the total of each entry; by due day, by
    // the installments that fall due in it (the entry of two installments counts in each month).
    let by_category = report(ReportGroup::Category, ReportBasis::Competence, october)
        .await
        .unwrap();
    assert_eq!(
        report_rows(&by_category),
        [
            (
                id_of(seed.other_category.as_uuid()),
                "0".into(),
                "2500".into(),
                1
            ),
            (
                id_of(seed.category.as_uuid()),
                "150.5".into(),
                "0".into(),
                1
            ),
        ]
    );
    let by_due = report(ReportGroup::Category, ReportBasis::Due, october)
        .await
        .unwrap();
    assert_eq!(
        report_rows(&by_due),
        [
            (
                id_of(seed.other_category.as_uuid()),
                "0".into(),
                "2500".into(),
                1
            ),
            (id_of(seed.category.as_uuid()), "100".into(), "40".into(), 2),
        ]
    );
    let november_due = report(
        ReportGroup::Category,
        ReportBasis::Due,
        (day("2026-11-01"), day("2026-11-30")),
    )
    .await
    .unwrap();
    assert_eq!(
        report_rows(&november_due),
        [(
            id_of(seed.category.as_uuid()),
            "350.5".into(),
            "0".into(),
            2
        )]
    );

    // By cost center and by party, where what has none is a group of its own and comes last among equals.
    let by_cost_center = report(ReportGroup::CostCenter, ReportBasis::Competence, october)
        .await
        .unwrap();
    assert_eq!(
        report_rows(&by_cost_center),
        [
            ("-".into(), "0".into(), "2500".into(), 1),
            (
                id_of(seed.cost_center.as_uuid()),
                "150.5".into(),
                "0".into(),
                1
            ),
        ]
    );
    let by_party = report(ReportGroup::Party, ReportBasis::Competence, october)
        .await
        .unwrap();
    assert_eq!(
        report_rows(&by_party),
        [
            ("-".into(), "0".into(), "2500".into(), 1),
            (id_of(seed.party.as_uuid()), "150.5".into(), "0".into(), 1),
        ]
    );
    assert!(
        report(
            ReportGroup::Category,
            ReportBasis::Competence,
            (day("2025-01-01"), day("2025-12-31"))
        )
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
        service
            .report(
                other_company,
                ReportGroup::Category,
                ReportBasis::Competence,
                october.0,
                october.1
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        service
            .report(
                company,
                ReportGroup::Category,
                ReportBasis::Competence,
                october.1,
                october.0
            )
            .await,
        Err(ApplicationError::InvalidSpan(_))
    ));

    // What is overdue on a day: due before it, with something left, oldest first; the totals are of all.
    let on = day("2026-11-15");
    let overdue = service.overdue(company, on, None, None).await.unwrap();
    assert_eq!(
        overdue_of(&overdue),
        (
            vec![
                "2026-10-05 Aluguel 1500".to_owned(),
                "2026-10-30 Material 40".to_owned(),
                "2026-11-12 Venda 2 300".to_owned()
            ],
            "300".to_owned(),
            "1540".to_owned(),
            3
        )
    );
    assert_eq!(overdue.items[0].amount.to_string(), "2500");
    assert_eq!(overdue.items[0].settled.to_string(), "1000");
    assert_eq!(overdue.items[0].installment, 1);
    assert_eq!(overdue.items[0].kind, EntryKind::Payable);
    let receivable = service
        .overdue(company, on, Some(EntryKind::Receivable), None)
        .await
        .unwrap();
    assert_eq!(
        overdue_of(&receivable),
        (
            vec!["2026-11-12 Venda 2 300".to_owned()],
            "300".to_owned(),
            "0".to_owned(),
            1
        )
    );
    let cut = service.overdue(company, on, None, Some(2)).await.unwrap();
    assert_eq!(
        (cut.items.len(), cut.count, cut.payable.to_string()),
        (2, 3, "1540".to_owned())
    );
    assert_eq!(
        service
            .overdue(company, day("2026-10-05"), None, None)
            .await
            .unwrap()
            .count,
        0
    );
    assert_eq!(
        service
            .overdue(other_company, on, None, None)
            .await
            .unwrap()
            .count,
        0
    );

    // Settling the rest takes it off the list; reversing the first settlement puts its amount back.
    settle(rent.id(), 1, "1500").await.unwrap();
    assert_eq!(
        service
            .overdue(company, on, None, None)
            .await
            .unwrap()
            .count,
        2
    );
    service
        .reverse(company, &ana, rent.id(), part.id())
        .await
        .unwrap();
    let after = service.overdue(company, on, None, None).await.unwrap();
    assert_eq!(overdue_of(&after).0[0], "2026-10-05 Aluguel 1000");

    // A sum is written without trailing zeros, however the database adds it up (0.5 + 0.5 is 1.0 in SQL).
    service
        .register(
            company,
            &ana,
            draft(
                EntryKind::Payable,
                "Meio e meio",
                seed.other_category,
                "2030-01-10",
                &[("2030-01-10", "0.5"), ("2030-02-10", "0.5")],
            ),
        )
        .await
        .unwrap();
    let halves = report(
        ReportGroup::Category,
        ReportBasis::Competence,
        (day("2030-01-01"), day("2030-01-31")),
    )
    .await
    .unwrap();
    assert_eq!(halves[0].payable.to_string(), "1");
}

use conta_verde_application::{AttachmentRepository, AttachmentService, BlobStore};
use conta_verde_domain::{Attachment, AttachmentDraft, AttachmentId, MAX_ENTRY_ATTACHMENTS};

/// A blob store keeps what is put under a company and an id, apart by company, and takes a delete
/// twice.
pub async fn keeps_blobs<B: BlobStore>(store: &B, company: CompanyId, other_company: CompanyId) {
    let id = AttachmentId::generate();
    assert_eq!(store.get(company, id).await.unwrap(), None);
    store.put(company, id, b"first").await.unwrap();
    assert_eq!(
        store.get(company, id).await.unwrap(),
        Some(b"first".to_vec())
    );
    // Putting again replaces; another company's id finds nothing.
    store.put(company, id, b"second, longer").await.unwrap();
    assert_eq!(
        store.get(company, id).await.unwrap(),
        Some(b"second, longer".to_vec())
    );
    assert_eq!(store.get(other_company, id).await.unwrap(), None);
    store.delete(other_company, id).await.unwrap();
    assert!(store.get(company, id).await.unwrap().is_some());
    store.delete(company, id).await.unwrap();
    assert_eq!(store.get(company, id).await.unwrap(), None);
    store.delete(company, id).await.unwrap();
}

pub async fn attaches_files<R, B, E, F>(
    service: &AttachmentService<R, B, E>,
    store: &R,
    blobs: &B,
    entries: &FinancialEntryService<E, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    R: AttachmentRepository,
    B: BlobStore,
    E: FinancialEntryRepository,
    F: EntryReferences,
{
    let ana = actor();
    let register = |description: &str| {
        entries.register(
            company,
            &ana,
            entry_draft(seed, description, "2026-11-10", &[("2026-11-20", "100")]),
        )
    };
    let entry = register("Compra de farinha").await.unwrap();
    let other_entry = register("Outra compra").await.unwrap();

    let invoice = attach_file(
        service,
        company,
        &ana,
        &entry,
        "Nota 123.pdf",
        "application/pdf",
        b"%PDF-1.4 nota",
    )
    .await
    .unwrap();
    let receipt = attach_file(
        service,
        company,
        &ana,
        &entry,
        "Recibo.png",
        "IMAGE/PNG",
        b"\x89PNG recibo",
    )
    .await
    .unwrap();
    assert_eq!(invoice.file_name(), "Nota 123.pdf");
    assert_eq!(receipt.content_type(), "image/png");
    assert_eq!(invoice.size(), 13);
    assert_eq!(
        service.list(company, entry.id()).await.unwrap(),
        vec![invoice.clone(), receipt.clone()],
        "in the order they were attached"
    );
    assert!(
        service
            .list(company, other_entry.id())
            .await
            .unwrap()
            .is_empty()
    );

    // The bytes come back with the record, and only for the company and the entry they belong to.
    let file = service
        .download(company, entry.id(), invoice.id())
        .await
        .unwrap();
    assert_eq!(file.attachment, invoice);
    assert_eq!(file.bytes, b"%PDF-1.4 nota");
    assert_eq!(blobs.get(other_company, invoice.id()).await.unwrap(), None);
    for (who, under) in [(other_company, entry.id()), (company, other_entry.id())] {
        assert_eq!(
            service
                .download(who, under, invoice.id())
                .await
                .unwrap_err(),
            ApplicationError::AttachmentNotFound
        );
    }
    assert_eq!(
        service.list(other_company, entry.id()).await.unwrap_err(),
        ApplicationError::EntryNotFound
    );
    assert_eq!(
        attach_to_unknown(service, company, &ana).await,
        ApplicationError::EntryNotFound
    );

    // What is refused is told why, and leaves nothing.
    for (name, kind, bytes, error) in [
        (
            "a.html",
            "text/html",
            &b"<b>"[..],
            DomainError::UnsupportedAttachmentType,
        ),
        (
            "a.pdf",
            "application/pdf",
            &b""[..],
            DomainError::InvalidAttachmentSize,
        ),
        (
            "a/b.pdf",
            "application/pdf",
            &b"x"[..],
            DomainError::InvalidAttachmentName,
        ),
    ] {
        assert_eq!(
            attach_file(service, company, &ana, &entry, name, kind, bytes)
                .await
                .unwrap_err(),
            ApplicationError::Domain(error)
        );
    }
    assert_eq!(service.list(company, entry.id()).await.unwrap().len(), 2);

    // Its trail has the creation; a write whose audit entry is about something else is refused whole.
    let created = store.history(company, invoice.id()).await.unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].entry.entity, AuditEntity::Attachment);
    assert_eq!(created[0].entry.action, AuditAction::Created);
    assert!(
        store
            .history(other_company, invoice.id())
            .await
            .unwrap()
            .is_empty()
    );
    let stray = Attachment::new(
        AttachmentId::generate(),
        company,
        entry.id(),
        AttachmentDraft {
            file_name: "Estranho.pdf".into(),
            content_type: "application/pdf".into(),
            size: 1,
        },
    )
    .unwrap();
    let wrong_audit = AuditEntry {
        entity_id: invoice.id().as_uuid(),
        ..created[0].entry.clone()
    };
    assert!(store.save(&stray, &wrong_audit).await.is_err());
    assert!(
        store
            .find(company, entry.id(), stray.id())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(store.history(company, invoice.id()).await.unwrap().len(), 1);

    // Removing takes it off the list and drops the bytes, and the trail keeps both moves.
    assert_eq!(
        service
            .remove(other_company, &ana, entry.id(), invoice.id())
            .await
            .unwrap_err(),
        ApplicationError::AttachmentNotFound
    );
    assert_eq!(
        service
            .remove(company, &ana, other_entry.id(), invoice.id())
            .await
            .unwrap_err(),
        ApplicationError::AttachmentNotFound
    );
    service
        .remove(company, &ana, entry.id(), invoice.id())
        .await
        .unwrap();
    assert_eq!(
        service.list(company, entry.id()).await.unwrap(),
        vec![receipt.clone()]
    );
    assert_eq!(blobs.get(company, invoice.id()).await.unwrap(), None);
    assert_eq!(
        service
            .download(company, entry.id(), invoice.id())
            .await
            .unwrap_err(),
        ApplicationError::AttachmentNotFound
    );
    assert_eq!(
        service
            .remove(company, &ana, entry.id(), invoice.id())
            .await
            .unwrap_err(),
        ApplicationError::AttachmentNotFound
    );
    let trail = store.history(company, invoice.id()).await.unwrap();
    assert_eq!(trail.len(), 2);
    assert_eq!(trail[1].entry.action, AuditAction::Updated);
    assert_eq!(
        trail[1]
            .entry
            .changes
            .iter()
            .map(|c| c.field.as_str())
            .collect::<Vec<_>>(),
        ["removed"]
    );
    assert!(
        store
            .find(company, entry.id(), invoice.id())
            .await
            .unwrap()
            .is_some_and(|stored| stored.is_removed()),
        "the record stays, removed"
    );

    // An entry holds a limited number at once, the ones removed not counting.
    for number in 1..MAX_ENTRY_ATTACHMENTS {
        attach_file(
            service,
            company,
            &ana,
            &entry,
            &format!("{number}.txt"),
            "text/plain",
            b"x",
        )
        .await
        .unwrap();
    }
    assert_eq!(
        service.list(company, entry.id()).await.unwrap().len(),
        MAX_ENTRY_ATTACHMENTS
    );
    assert_eq!(
        attach_file(service, company, &ana, &entry, "21.txt", "text/plain", b"x")
            .await
            .unwrap_err(),
        ApplicationError::TooManyAttachments
    );
    service
        .remove(company, &ana, entry.id(), receipt.id())
        .await
        .unwrap();
    attach_file(service, company, &ana, &entry, "21.txt", "text/plain", b"x")
        .await
        .unwrap();
}

async fn attach_file<R, B, E>(
    service: &AttachmentService<R, B, E>,
    company: CompanyId,
    ana: &Actor,
    entry: &FinancialEntry,
    name: &str,
    kind: &str,
    bytes: &[u8],
) -> Result<Attachment, ApplicationError>
where
    R: AttachmentRepository,
    B: BlobStore,
    E: FinancialEntryRepository,
{
    service
        .attach(company, ana, entry.id(), name, kind, bytes)
        .await
}

async fn attach_to_unknown<R, B, E>(
    service: &AttachmentService<R, B, E>,
    company: CompanyId,
    ana: &Actor,
) -> ApplicationError
where
    R: AttachmentRepository,
    B: BlobStore,
    E: FinancialEntryRepository,
{
    service
        .attach(
            company,
            ana,
            EntryId::generate(),
            "a.pdf",
            "application/pdf",
            b"x",
        )
        .await
        .unwrap_err()
}

/// What the sales contract tests point at: a customer, products (one retired) and payment terms (one
/// retired), all of one company.
pub struct SalesSeed {
    pub customer: PartyId,
    /// A party that is a seller, and so is not the customer.
    pub seller: PartyId,
    /// A party that is a supplier only.
    pub supplier: PartyId,
    pub product: ProductId,
    pub other_product: ProductId,
    pub inactive_product: ProductId,
    /// A service, which has no stock.
    pub service: ProductId,
    pub term: PaymentTermId,
    pub inactive_term: PaymentTermId,
}

pub async fn seed_sales_records<P, R, T>(
    parties: &P,
    products: &R,
    terms: &T,
    company: CompanyId,
) -> SalesSeed
where
    P: PartyRepository,
    R: ProductRepository,
    T: PaymentTermRepository,
{
    let customer = Party::new(
        PartyId::generate(),
        company,
        party_draft("Cliente Exemplo", None),
    )
    .unwrap();
    parties
        .save(&customer, &created_entry(&customer))
        .await
        .unwrap();
    let seller = Party::new(
        PartyId::generate(),
        company,
        PartyDraft {
            is_customer: false,
            is_seller: true,
            ..party_draft("Vendedora Vera", None)
        },
    )
    .unwrap();
    parties
        .save(&seller, &created_entry(&seller))
        .await
        .unwrap();
    let supplier = Party::new(
        PartyId::generate(),
        company,
        PartyDraft {
            is_customer: false,
            is_supplier: true,
            ..party_draft("Fornecedor Fábio", Some("11.222.333/0001-81"))
        },
    )
    .unwrap();
    parties
        .save(&supplier, &created_entry(&supplier))
        .await
        .unwrap();
    let mut saved = Vec::new();
    for (name, active, kind, sku) in [
        ("Café", true, ProductKind::Product, Some("CAFE-500")),
        ("Pão", true, ProductKind::Product, Some("PAO-1")),
        ("Descontinuado", false, ProductKind::Product, None),
        ("Consultoria", true, ProductKind::Service, None),
    ] {
        let product = Product::new(
            ProductId::generate(),
            company,
            product_draft(name, kind, sku, active),
        )
        .unwrap();
        let entry = product_entry(
            &product,
            AuditAction::Created,
            product_creation_changes(&product),
        );
        products.save(&product, &entry).await.unwrap();
        saved.push(product.id());
    }
    let term = new_term(company, "30/60", &[(30, "50"), (60, "50")]);
    let retired = PaymentTerm::new(
        PaymentTermId::generate(),
        company,
        term_draft("Antigo", &[(0, "100")], false),
    )
    .unwrap();
    for item in [&term, &retired] {
        save_term(terms, item).await.unwrap();
    }
    SalesSeed {
        customer: customer.id(),
        seller: seller.id(),
        supplier: supplier.id(),
        product: saved[0],
        other_product: saved[1],
        inactive_product: saved[2],
        service: saved[3],
        term: term.id(),
        inactive_term: retired.id(),
    }
}

pub async fn document_references_say_what_is_usable<R: DocumentReferences>(
    references: &R,
    company: CompanyId,
    other_company: CompanyId,
    seed: &SalesSeed,
) {
    assert!(
        references
            .customer_exists(company, seed.customer)
            .await
            .unwrap()
    );
    assert!(
        !references
            .customer_exists(other_company, seed.customer)
            .await
            .unwrap()
    );
    assert!(
        !references
            .customer_exists(company, PartyId::generate())
            .await
            .unwrap()
    );
    assert!(
        references
            .seller_is_usable(company, seed.seller)
            .await
            .unwrap()
    );
    assert!(
        !references
            .seller_is_usable(company, seed.customer)
            .await
            .unwrap()
    );
    assert!(
        !references
            .seller_is_usable(other_company, seed.seller)
            .await
            .unwrap()
    );
    assert!(
        references
            .supplier_is_usable(company, seed.supplier)
            .await
            .unwrap()
    );
    assert!(
        !references
            .supplier_is_usable(company, seed.customer)
            .await
            .unwrap()
    );
    assert!(
        !references
            .supplier_is_usable(other_company, seed.supplier)
            .await
            .unwrap()
    );
    let issuer = conta_verde_domain::Document::parse("11.222.333/0001-81").unwrap();
    assert_eq!(
        references
            .party_with_document(company, &issuer)
            .await
            .unwrap(),
        Some(seed.supplier)
    );
    assert_eq!(
        references
            .party_with_document(other_company, &issuer)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        references
            .product_with_code(company, "cafe-500")
            .await
            .unwrap(),
        Some(seed.product)
    );
    assert_eq!(
        references.product_with_code(company, "CAFE").await.unwrap(),
        None
    );
    assert_eq!(
        references
            .product_with_code(other_company, "CAFE-500")
            .await
            .unwrap(),
        None
    );
    assert!(
        references
            .payment_term(company, seed.term)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        references
            .payment_term(other_company, seed.term)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        references
            .product_is_usable(company, seed.product)
            .await
            .unwrap()
    );
    assert!(
        !references
            .product_is_usable(company, seed.inactive_product)
            .await
            .unwrap()
    );
    assert!(
        !references
            .product_is_usable(other_company, seed.product)
            .await
            .unwrap()
    );
    assert!(
        references
            .payment_term_is_usable(company, seed.term)
            .await
            .unwrap()
    );
    assert!(
        !references
            .payment_term_is_usable(company, seed.inactive_term)
            .await
            .unwrap()
    );
    assert!(
        !references
            .payment_term_is_usable(other_company, seed.term)
            .await
            .unwrap()
    );
}

pub fn sale_line(
    product: ProductId,
    description: &str,
    quantity: &str,
    unit_price: &str,
    discount: &str,
) -> LineDraft {
    LineDraft {
        product_id: product,
        description: description.into(),
        quantity: quantity.into(),
        unit_price: unit_price.into(),
        discount: discount.into(),
    }
}

fn quote_draft(seed: &SalesSeed, valid_until: &str) -> QuoteDraft {
    QuoteDraft {
        customer_id: seed.customer,
        issued_on: "2026-11-06".into(),
        valid_until: valid_until.into(),
        payment_term_id: Some(seed.term),
        notes: Some("Entrega em 5 dias".into()),
        lines: vec![
            sale_line(seed.product, "Café 500g", "3", "10.10", "0.30"),
            sale_line(seed.other_product, "Pão francês", "1.5", "3.33", "0"),
        ],
    }
}

fn token_of(link: &str) -> &str {
    link.rsplit('/').next().unwrap()
}

fn quote_numbers(page: &QuotePage) -> Vec<i64> {
    page.items.iter().map(|quote| quote.number()).collect()
}

/// A quote is written with the next number of its company and a trail, listed, filtered and paged,
/// changed while it is a draft or sent, sent with a link of its own that the next send and any change
/// take back, and answered once through the link, in the name of whoever sent it, never past its
/// validity. What it points at has to be usable, and nothing of another company's shows.
pub async fn runs_the_quote_lifecycle<Q, R, E, S>(
    service: &QuoteService<Q, R, E, S>,
    store: &Q,
    emails: &InMemoryEmailSender,
    company: CompanyId,
    other_company: CompanyId,
    seed: &SalesSeed,
) where
    Q: QuoteRepository,
    R: DocumentReferences,
    E: EmailSender,
    S: SecretService,
{
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();

    // Written: numbered per company, worth the sum of its rounded lines, with one entry in its trail.
    let first = service
        .register(company, &ana, quote_draft(seed, "2026-11-20"))
        .await
        .unwrap();
    assert_eq!(first.number(), 1);
    assert_eq!(first.status(), QuoteStatus::Draft);
    // 3 x 10.10 - 0.30 = 30 and 1.5 x 3.33 = 4.995 -> 5.
    assert_eq!(first.total().to_string(), "35");
    let stored = service.get(company, first.id()).await.unwrap();
    assert_eq!(stored, first);
    let trail = service.history(company, first.id()).await.unwrap();
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].entry.action, AuditAction::Created);
    assert_eq!(trail[0].entry.actor, ana);
    let second = service
        .register(company, &ana, quote_draft(seed, "2026-12-31"))
        .await
        .unwrap();
    assert_eq!(second.number(), 2);
    // Four writes at once never share a number.
    let (a, b, c, d) = tokio::join!(
        service.register(company, &ana, quote_draft(seed, "2026-11-20")),
        service.register(company, &ana, quote_draft(seed, "2026-11-20")),
        service.register(company, &ana, quote_draft(seed, "2026-11-20")),
        service.register(company, &ana, quote_draft(seed, "2026-11-20")),
    );
    let mut numbers: Vec<i64> = [a, b, c, d]
        .iter()
        .map(|quote| quote.as_ref().unwrap().number())
        .collect();
    numbers.sort_unstable();
    assert_eq!(numbers, [3, 4, 5, 6]);

    // What is refused leaves no number and no trace; a customer, a product or a term has to be usable.
    let mut bad = quote_draft(seed, "2026-11-20");
    bad.customer_id = PartyId::generate();
    assert_eq!(
        service.register(company, &ana, bad).await.unwrap_err(),
        ApplicationError::UnusableReference("customer")
    );
    let mut bad = quote_draft(seed, "2026-11-20");
    bad.lines[1].product_id = seed.inactive_product;
    assert_eq!(
        service.register(company, &ana, bad).await.unwrap_err(),
        ApplicationError::UnusableReference("product")
    );
    let mut bad = quote_draft(seed, "2026-11-20");
    bad.payment_term_id = Some(seed.inactive_term);
    assert_eq!(
        service.register(company, &ana, bad).await.unwrap_err(),
        ApplicationError::UnusableReference("payment term")
    );
    let mut bad = quote_draft(seed, "2026-11-20");
    bad.lines.clear();
    assert_eq!(
        service.register(company, &ana, bad).await.unwrap_err(),
        ApplicationError::Domain(DomainError::NoLines)
    );
    let next = service
        .register(company, &ana, quote_draft(seed, "2026-11-20"))
        .await
        .unwrap();
    assert_eq!(next.number(), 7);

    // Another company has none of it; its own numbers start over.
    assert_eq!(
        service.get(other_company, first.id()).await.unwrap_err(),
        ApplicationError::QuoteNotFound
    );
    assert_eq!(
        service
            .history(other_company, first.id())
            .await
            .unwrap_err(),
        ApplicationError::QuoteNotFound
    );
    let none = service
        .list(other_company, &QuoteQuery::default(), None, None)
        .await
        .unwrap();
    assert!(none.items.is_empty());

    // Listed newest first, by status, customer and search, and paged.
    let all = service
        .list(company, &QuoteQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(quote_numbers(&all), [7, 6, 5, 4, 3, 2, 1]);
    let ascending = service
        .list(
            company,
            &QuoteQuery::default().sorted(NumberSort::NumberAsc),
            None,
            Some(3),
        )
        .await
        .unwrap();
    assert_eq!(quote_numbers(&ascending), [1, 2, 3]);
    let rest = service
        .list(
            company,
            &QuoteQuery::default().sorted(NumberSort::NumberAsc),
            ascending.next.as_ref(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(quote_numbers(&rest), [4, 5, 6, 7]);
    assert!(rest.next.is_none());
    let by_text = |text: &str| QuoteQuery::default().searching(text);
    for (text, expected) in [
        ("pão fran", 7),
        ("6", 1),
        ("ENTREGA", 7),
        ("nada disso", 0),
        ("%", 0),
    ] {
        let found = service
            .list(company, &by_text(text), None, None)
            .await
            .unwrap();
        assert_eq!(found.items.len(), expected, "{text}");
    }
    let drafts = service
        .list(
            company,
            &QuoteQuery::default().with_status(QuoteStatus::Draft),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(drafts.items.len(), 7);
    let approved = service
        .list(
            company,
            &QuoteQuery::default().with_status(QuoteStatus::Approved),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(approved.items.is_empty());
    let of_customer = service
        .list(
            company,
            &QuoteQuery::default().with_customer(seed.customer),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(of_customer.items.len(), 7);
    let of_nobody = service
        .list(
            company,
            &QuoteQuery::default().with_customer(PartyId::generate()),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(of_nobody.items.is_empty());

    // Changed: the trail records the change, a draft that changes nothing records nothing.
    let mut edit = quote_draft(seed, "2026-11-25");
    edit.lines[0].quantity = "4".into();
    let changed = service
        .update(company, &ana, first.id(), edit.clone())
        .await
        .unwrap();
    assert_eq!(changed.number(), 1);
    assert_eq!(changed.total().to_string(), "45.1");
    let trail = service.history(company, first.id()).await.unwrap();
    assert_eq!(trail.len(), 2);
    let fields: Vec<&str> = trail[1]
        .entry
        .changes
        .iter()
        .map(|change| change.field.as_str())
        .collect();
    assert_eq!(fields, ["valid_until", "lines"]);
    service
        .update(company, &ana, first.id(), edit.clone())
        .await
        .unwrap();
    assert_eq!(service.history(company, first.id()).await.unwrap().len(), 2);
    let mut bad = edit.clone();
    bad.lines[0].product_id = seed.inactive_product;
    assert_eq!(
        service
            .update(company, &ana, first.id(), bad)
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("product")
    );
    assert_eq!(
        service
            .update(other_company, &ana, first.id(), edit.clone())
            .await
            .unwrap_err(),
        ApplicationError::QuoteNotFound
    );

    // A change the trail has no field of its own for is still a change: only a line's description.
    let mut reworded = edit.clone();
    reworded.lines[0].description = "Café torrado 500g".into();
    let reworded = service
        .update(company, &ana, first.id(), reworded)
        .await
        .unwrap();
    assert_eq!(reworded.lines()[0].description(), "Café torrado 500g");
    assert_eq!(
        service.get(company, first.id()).await.unwrap().lines()[0].description(),
        "Café torrado 500g"
    );
    assert_eq!(service.history(company, first.id()).await.unwrap().len(), 3);
    let edit = {
        let mut back = edit.clone();
        back.lines[0].description = "Café torrado 500g".into();
        back
    };

    // Sent: a link of its own, e-mailed, that leads to the quote.
    assert_eq!(
        service
            .send(
                company,
                &ana,
                first.id(),
                "not an address",
                "Padaria",
                day("2026-11-10")
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::InvalidEmail)
    );
    let sent = service
        .send(
            company,
            &ana,
            first.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    assert_eq!(sent.quote.status(), QuoteStatus::Sent);
    assert!(sent.emailed);
    assert!(sent.link.starts_with("https://erp.example/quote/"));
    let message = emails.sent().pop().unwrap();
    assert_eq!(message.to.as_str(), "cliente@exemplo.com");
    assert!(message.subject.contains("nº 1"));
    assert!(message.body.contains(&sent.link));
    assert!(message.body.contains("2026-11-25"));
    let first_token = token_of(&sent.link).to_owned();
    assert_eq!(service.by_link(&first_token).await.unwrap(), sent.quote);
    assert_eq!(
        service.by_link("not-a-token").await.unwrap_err(),
        ApplicationError::QuoteNotFound
    );
    let trail = service.history(company, first.id()).await.unwrap();
    assert_eq!(trail.len(), 4);
    assert!(
        trail[3]
            .entry
            .changes
            .iter()
            .any(|change| change.field == "link" && change.to.as_deref() == Some("made and sent"))
    );

    // Sent again, the earlier link stops working and the new one works.
    let resent = service
        .send(
            company,
            &ana,
            first.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    let second_token = token_of(&resent.link).to_owned();
    assert_ne!(first_token, second_token);
    assert_eq!(
        service.by_link(&first_token).await.unwrap_err(),
        ApplicationError::QuoteNotFound
    );
    assert!(service.by_link(&second_token).await.is_ok());

    // A change to a sent quote makes it a draft again and takes its link back; a change of nothing does not.
    service
        .update(company, &ana, first.id(), edit.clone())
        .await
        .unwrap();
    assert!(service.by_link(&second_token).await.is_ok());
    let mut other_edit = edit.clone();
    other_edit.notes = None;
    let redone = service
        .update(company, &ana, first.id(), other_edit)
        .await
        .unwrap();
    assert_eq!(redone.status(), QuoteStatus::Draft);
    assert_eq!(
        service.by_link(&second_token).await.unwrap_err(),
        ApplicationError::QuoteNotFound
    );
    assert_eq!(
        service
            .answer(&second_token, true, day("2026-11-10"))
            .await
            .unwrap_err(),
        ApplicationError::QuoteNotFound
    );

    // Answered through the link, once, in the name of whoever sent it, and the quote is then fixed.
    let resent = service
        .send(
            company,
            &ana,
            first.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    let token = token_of(&resent.link).to_owned();
    let answered = service
        .answer(&token, true, day("2026-11-25"))
        .await
        .unwrap();
    assert_eq!(answered.status(), QuoteStatus::Approved);
    let trail = service.history(company, first.id()).await.unwrap();
    let last = trail.last().unwrap();
    assert_eq!(last.entry.actor, ana);
    assert!(
        last.entry
            .changes
            .iter()
            .any(|change| change.field == "status" && change.to.as_deref() == Some("approved"))
    );
    assert!(
        last.entry
            .changes
            .iter()
            .any(|change| change.field == "answered_by")
    );
    // The link still shows it, answered; it cannot be answered again, changed or sent again.
    assert_eq!(
        service.by_link(&token).await.unwrap().status(),
        QuoteStatus::Approved
    );
    assert_eq!(
        service
            .answer(&token, false, day("2026-11-25"))
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::QuoteIsNotSent)
    );
    assert_eq!(
        service
            .update(company, &ana, first.id(), edit.clone())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::QuoteIsLocked)
    );
    assert_eq!(
        service
            .send(
                company,
                &ana,
                first.id(),
                "cliente@exemplo.com",
                "Padaria",
                day("2026-11-10")
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::QuoteIsLocked)
    );

    // Declined, and one past its validity is not approved (its last day is still good).
    let declined = service
        .send(
            company,
            &ana,
            second.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    let declined = service
        .answer(token_of(&declined.link), false, day("2026-12-31"))
        .await
        .unwrap();
    assert_eq!(declined.status(), QuoteStatus::Declined);
    let late = service
        .send(
            company,
            &ana,
            next.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .answer(token_of(&late.link), true, day("2026-11-21"))
            .await
            .unwrap_err(),
        ApplicationError::QuoteExpired
    );
    assert_eq!(
        service.get(company, next.id()).await.unwrap().status(),
        QuoteStatus::Sent
    );
    assert_eq!(
        service
            .answer(token_of(&late.link), true, day("2026-11-20"))
            .await
            .unwrap()
            .status(),
        QuoteStatus::Approved
    );

    // Declining is allowed after the validity; sending is not.
    let stale = service
        .register(company, &ana, quote_draft(seed, "2026-11-20"))
        .await
        .unwrap();
    let sent = service
        .send(
            company,
            &ana,
            stale.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .send(
                company,
                &ana,
                stale.id(),
                "cliente@exemplo.com",
                "Padaria",
                day("2026-11-21")
            )
            .await
            .unwrap_err(),
        ApplicationError::QuoteExpired
    );
    assert_eq!(
        service
            .answer(token_of(&sent.link), false, day("2026-12-01"))
            .await
            .unwrap()
            .status(),
        QuoteStatus::Declined
    );

    // An answer made to what the customer saw before is refused when the quote was changed and sent
    // again meanwhile: the store checks the link the answer came through, and writes only the status.
    let racing = service
        .register(company, &ana, quote_draft(seed, "2026-11-20"))
        .await
        .unwrap();
    let old_link = service
        .send(
            company,
            &ana,
            racing.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    let old_token = token_of(&old_link.link).to_owned();
    let seen = service.by_link(&old_token).await.unwrap();
    let mut changed = quote_draft(seed, "2026-11-25");
    changed.lines[0].quantity = "9".into();
    service
        .update(company, &ana, racing.id(), changed)
        .await
        .unwrap();
    service
        .send(
            company,
            &ana,
            racing.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    let digest = crate::Argon2SecretService::new()
        .unwrap()
        .digest_token(&old_token);
    let approval = seen.answered(true).unwrap();
    let audit = AuditEntry {
        company_id: company,
        actor: ana.clone(),
        entity: AuditEntity::Quote,
        entity_id: racing.id().as_uuid(),
        action: AuditAction::Updated,
        changes: Vec::new(),
    };
    assert_eq!(
        store.answer(&approval, &digest, &audit).await.unwrap_err(),
        RepositoryError::QuoteChanged
    );
    let after = service.get(company, racing.id()).await.unwrap();
    assert_eq!(after.status(), QuoteStatus::Sent);
    assert_eq!(after.lines()[0].quantity().to_string(), "9");
    assert_eq!(
        service.history(company, racing.id()).await.unwrap().len(),
        4
    );
}

fn sale_draft(entry_seed: &EntrySeed, seed: &SalesSeed) -> SaleDraft {
    SaleDraft {
        customer_id: seed.customer,
        seller_id: Some(seed.seller),
        sold_on: "2026-11-06".into(),
        payment_term_id: Some(seed.term),
        category_id: entry_seed.category,
        cost_center_id: Some(entry_seed.cost_center),
        notes: Some("Entrega em 5 dias".into()),
        lines: vec![
            sale_line(seed.product, "Café 500g", "3", "10.10", "0.30"),
            sale_line(seed.other_product, "Pão francês", "1.5", "3.33", "0"),
        ],
    }
}

fn return_draft(items: &[(u16, &str)]) -> SaleReturnDraft {
    SaleReturnDraft {
        returned_on: "2026-11-10".into(),
        reason: Some("Estragou".into()),
        items: items
            .iter()
            .map(|(line, quantity)| ReturnItemDraft {
                line: *line,
                quantity: (*quantity).into(),
            })
            .collect(),
    }
}

fn sale_numbers(sales: &conta_verde_application::SalePage) -> Vec<i64> {
    sales.items.iter().map(|sale| sale.number()).collect()
}

/// An entry is cancelled once and for good, refused while it has settlements that are not reversed, and
/// then counts nowhere: not in what is open by due day, not in the overdue installments, not in the reports,
/// and it takes no edit and no settlement. It is still listed, and says so.
#[allow(clippy::too_many_arguments)]
pub async fn voids_entries<R, F>(
    service: &FinancialEntryService<R, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    R: FinancialEntryRepository,
    F: EntryReferences,
{
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    let draft = entry_draft(
        seed,
        "Venda a cancelar",
        "2026-11-05",
        &[("2026-11-10", "100")],
    );
    let entry = service
        .register(company, &ana, draft.clone())
        .await
        .unwrap();
    let kept = service
        .register(
            company,
            &ana,
            entry_draft(
                seed,
                "Venda que fica",
                "2026-11-05",
                &[("2026-11-12", "40")],
            ),
        )
        .await
        .unwrap();
    let report = |basis| {
        service.report(
            company,
            ReportGroup::Category,
            basis,
            day("2026-11-01"),
            day("2026-11-30"),
        )
    };
    assert_eq!(
        report(ReportBasis::Competence).await.unwrap()[0]
            .receivable
            .to_string(),
        "140"
    );

    // Not while it has a settlement that is not reversed.
    let settled = service
        .settle(company, &ana, entry.id(), settlement_draft(seed, 1, "30"))
        .await
        .unwrap();
    assert_eq!(
        service.void(company, &ana, entry.id()).await.unwrap_err(),
        ApplicationError::EntryIsSettled
    );
    assert!(!service.get(company, entry.id()).await.unwrap().is_voided());
    service
        .reverse(company, &ana, entry.id(), settled.id())
        .await
        .unwrap();

    // Another company cannot cancel it.
    assert_eq!(
        service
            .void(other_company, &ana, entry.id())
            .await
            .unwrap_err(),
        ApplicationError::EntryNotFound
    );
    let trail_before = service.history(company, entry.id()).await.unwrap().len();
    let voided = service.void(company, &ana, entry.id()).await.unwrap();
    assert!(voided.is_voided());
    assert!(service.get(company, entry.id()).await.unwrap().is_voided());
    let trail = service.history(company, entry.id()).await.unwrap();
    assert_eq!(trail.len(), trail_before + 1);
    assert_eq!(trail.last().unwrap().entry.changes[0].field, "voided");

    // Once: it is not cancelled again, edited or settled.
    assert_eq!(
        service.void(company, &ana, entry.id()).await.unwrap_err(),
        ApplicationError::Domain(DomainError::EntryIsVoided)
    );
    assert_eq!(
        service
            .update(company, &ana, entry.id(), draft.clone())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::EntryIsVoided)
    );
    assert_eq!(
        service
            .settle(company, &ana, entry.id(), settlement_draft(seed, 1, "10"))
            .await
            .unwrap_err(),
        ApplicationError::EntryIsVoided
    );

    // It counts nowhere, and the others still do.
    assert_eq!(
        report(ReportBasis::Competence).await.unwrap()[0]
            .receivable
            .to_string(),
        "40"
    );
    assert_eq!(
        report(ReportBasis::Due).await.unwrap()[0]
            .receivable
            .to_string(),
        "40"
    );
    let overdue = service
        .overdue(company, day("2027-01-01"), None, None)
        .await
        .unwrap();
    assert_eq!(overdue.count, 1);
    assert_eq!(overdue.items[0].entry_id, kept.id());
    assert_eq!(overdue.receivable.to_string(), "40");

    // It is listed, and the list can be narrowed to it or without it.
    let all = service
        .list(company, &FinancialEntryQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(all.items.len(), 2);
    let only = |voided| FinancialEntryQuery::default().with_voided(voided);
    let cancelled = service
        .list(company, &only(true), None, None)
        .await
        .unwrap();
    assert_eq!(cancelled.items.len(), 1);
    assert_eq!(cancelled.items[0].id(), entry.id());
    let live = service
        .list(company, &only(false), None, None)
        .await
        .unwrap();
    assert_eq!(live.items.len(), 1);
    assert_eq!(live.items[0].id(), kept.id());
}

/// A sale is written with a number, an order that is open can change, a quote becomes one sale, a completed
/// sale makes its receivable once by its payment term, a return refunds the returned part with a payable, and
/// a cancelled sale cancels its receivable unless money was settled on it. Everything it points at has to be
/// usable and nothing of another company's shows.
#[allow(clippy::too_many_arguments)]
pub async fn runs_the_sale_lifecycle<S, Q, E, F, R, T, Q2, R2, Em, Se>(
    sales: &SaleService<S, Q, E, F, R, T>,
    store: &S,
    quotes: &QuoteService<Q2, R2, Em, Se>,
    entries: &FinancialEntryService<E, F>,
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
    Q2: QuoteRepository,
    R2: DocumentReferences,
    Em: EmailSender,
    Se: SecretService,
{
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();

    // Written as an open order, numbered per company, worth the sum of its lines.
    let first = sales
        .register(company, &ana, sale_draft(entry_seed, seed))
        .await
        .unwrap();
    assert_eq!((first.number(), first.status()), (1, SaleStatus::Open));
    assert_eq!(first.total().to_string(), "35");
    assert_eq!(sales.get(company, first.id()).await.unwrap(), first);
    let trail = sales.history(company, first.id()).await.unwrap();
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].entry.action, AuditAction::Created);
    let second = sales
        .register(company, &ana, sale_draft(entry_seed, seed))
        .await
        .unwrap();
    assert_eq!(second.number(), 2);

    // What it points at has to be usable, and what is refused leaves nothing.
    let refused = |error| ApplicationError::UnusableReference(error);
    for (what, change) in [
        (
            "customer",
            Box::new(|d: &mut SaleDraft| d.customer_id = PartyId::generate())
                as Box<dyn Fn(&mut SaleDraft)>,
        ),
        (
            "seller",
            Box::new(|d: &mut SaleDraft| d.seller_id = Some(d.customer_id)),
        ),
        (
            "payment term",
            Box::new(|d: &mut SaleDraft| d.payment_term_id = Some(seed.inactive_term)),
        ),
        (
            "product",
            Box::new(|d: &mut SaleDraft| d.lines[0].product_id = seed.inactive_product),
        ),
    ] {
        let mut bad = sale_draft(entry_seed, seed);
        change(&mut bad);
        assert_eq!(
            sales.register(company, &ana, bad).await.unwrap_err(),
            refused(what),
            "{what}"
        );
    }
    for (category, what) in [
        (entry_seed.inactive_category, "category"),
        (entry_seed.transfer_category, "category"),
    ] {
        let mut bad = sale_draft(entry_seed, seed);
        bad.category_id = category;
        assert_eq!(
            sales.register(company, &ana, bad).await.unwrap_err(),
            refused(what)
        );
    }
    let mut bad = sale_draft(entry_seed, seed);
    bad.cost_center_id = Some(entry_seed.inactive_cost_center);
    assert_eq!(
        sales.register(company, &ana, bad).await.unwrap_err(),
        refused("cost center")
    );
    let mut bad = sale_draft(entry_seed, seed);
    bad.lines.clear();
    assert_eq!(
        sales.register(company, &ana, bad).await.unwrap_err(),
        ApplicationError::Domain(DomainError::NoLines)
    );
    let third = sales
        .register(company, &ana, sale_draft(entry_seed, seed))
        .await
        .unwrap();
    assert_eq!(third.number(), 3);

    // Nothing of another company's shows.
    assert_eq!(
        sales.get(other_company, first.id()).await.unwrap_err(),
        ApplicationError::SaleNotFound
    );
    assert!(
        sales
            .list(other_company, &SaleQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );

    // Listed newest first, by status, customer, seller and text, and paged.
    let listed = sales
        .list(company, &SaleQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(sale_numbers(&listed), [3, 2, 1]);
    let page = sales
        .list(company, &SaleQuery::default(), None, Some(2))
        .await
        .unwrap();
    assert_eq!(sale_numbers(&page), [3, 2]);
    let rest = sales
        .list(company, &SaleQuery::default(), page.next.as_ref(), None)
        .await
        .unwrap();
    assert_eq!(sale_numbers(&rest), [1]);
    for (query, expected) in [
        (SaleQuery::default().with_status(SaleStatus::Open), 3),
        (SaleQuery::default().with_status(SaleStatus::Completed), 0),
        (SaleQuery::default().with_customer(seed.customer), 3),
        (SaleQuery::default().with_seller(seed.seller), 3),
        (SaleQuery::default().with_seller(seed.customer), 0),
        (SaleQuery::default().searching("pão fran"), 3),
        (SaleQuery::default().searching("2"), 1),
        (SaleQuery::default().searching("%"), 0),
    ] {
        let found = sales.list(company, &query, None, None).await.unwrap();
        assert_eq!(found.items.len(), expected, "{query:?}");
    }

    // An open order changes; one that changes nothing records nothing.
    let mut edit = sale_draft(entry_seed, seed);
    edit.lines[0].quantity = "4".into();
    let changed = sales
        .update(company, &ana, first.id(), edit.clone())
        .await
        .unwrap();
    assert_eq!(changed.total().to_string(), "45.1");
    sales
        .update(company, &ana, first.id(), edit.clone())
        .await
        .unwrap();
    assert_eq!(sales.history(company, first.id()).await.unwrap().len(), 2);
    let mut bad = edit.clone();
    bad.seller_id = Some(seed.customer);
    assert_eq!(
        sales
            .update(company, &ana, first.id(), bad)
            .await
            .unwrap_err(),
        refused("seller")
    );

    // Completed: its receivable is one entry split by the term (30/60, half and half), in the customer's
    // name, once however often it is asked for.
    let (done_a, done_b) = tokio::join!(
        sales.complete(company, &ana, first.id()),
        sales.complete(company, &ana, first.id())
    );
    assert_eq!(done_a.unwrap().status(), SaleStatus::Completed);
    done_b.unwrap();
    assert_eq!(
        sales
            .complete(company, &ana, first.id())
            .await
            .unwrap()
            .status(),
        SaleStatus::Completed
    );
    let receivable = sales
        .receivable(company, first.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receivable.kind(), EntryKind::Receivable);
    assert_eq!(receivable.description(), "Venda nº 1");
    assert_eq!(receivable.party_id(), Some(seed.customer));
    assert_eq!(receivable.category_id(), entry_seed.category);
    assert_eq!(receivable.cost_center_id(), Some(entry_seed.cost_center));
    assert_eq!(receivable.origin(), Some(EntryOrigin::Sale(first.id())));
    assert_eq!(
        receivable.installments_text(),
        "1:2026-12-06:22.55,2:2027-01-05:22.55"
    );
    assert_eq!(receivable.competence_date().to_string(), "2026-11-06");
    let everything = entries
        .list(company, &FinancialEntryQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(everything.items.len(), 1);
    // Its items are fixed, and it is not cancelled or completed again without cause.
    assert_eq!(
        sales
            .update(company, &ana, first.id(), edit)
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::SaleIsLocked)
    );
    assert!(
        sales
            .receivable(company, second.id())
            .await
            .unwrap()
            .is_none()
    );
    // A sale with no term has one installment due on the day of the sale.
    let mut single = sale_draft(entry_seed, seed);
    single.payment_term_id = None;
    let single = sales.register(company, &ana, single).await.unwrap();
    sales.complete(company, &ana, single.id()).await.unwrap();
    assert_eq!(
        sales
            .receivable(company, single.id())
            .await
            .unwrap()
            .unwrap()
            .installments_text(),
        "1:2026-11-06:35"
    );

    // Returns: the returned part of a line is refunded by a payable, cumulatively, never more than was sold.
    let refund = || Refund {
        category_id: entry_seed.other_category,
        due_on: "2026-11-20".into(),
    };
    assert_eq!(
        sales
            .register_return(
                company,
                &ana,
                second.id(),
                return_draft(&[(1, "1")]),
                refund()
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::SaleIsNotCompleted)
    );
    let made = sales
        .register_return(
            company,
            &ana,
            first.id(),
            return_draft(&[(1, "1"), (2, "0.5")]),
            refund(),
        )
        .await
        .unwrap();
    // 1 of 4 of the line worth 40.1 is 10.025 -> 10.03, and a third of the line worth 5 is 1.67.
    assert_eq!(made.sale_return.refund().to_string(), "11.7");
    let payable = made.refund.unwrap();
    assert_eq!(payable.kind(), EntryKind::Payable);
    assert_eq!(payable.party_id(), Some(seed.customer));
    assert_eq!(payable.total().to_string(), "11.7");
    assert_eq!(payable.installments_text(), "1:2026-11-20:11.7");
    assert_eq!(
        payable.origin(),
        Some(EntryOrigin::SaleReturn(made.sale_return.id()))
    );
    assert_eq!(
        sales
            .register_return(
                company,
                &ana,
                first.id(),
                return_draft(&[(1, "3.001")]),
                refund()
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::ReturnExceedsSold)
    );
    let rest = sales
        .register_return(
            company,
            &ana,
            first.id(),
            return_draft(&[(1, "3")]),
            refund(),
        )
        .await
        .unwrap();
    // The line is worth 40.1 (4 x 10.10 less 0.30): 10.03 and then 30.07, so that they add up to the line.
    assert_eq!(rest.sale_return.refund().to_string(), "30.07");
    let returns = sales.returns(company, first.id()).await.unwrap();
    assert_eq!(returns.len(), 2);
    assert!(returns.iter().all(|record| record.refund.is_some()));
    assert_eq!(returns[0].sale_return.id(), made.sale_return.id());
    assert_eq!(
        sales.returns(other_company, first.id()).await.unwrap_err(),
        ApplicationError::SaleNotFound
    );
    // The store itself refuses a return of more than was sold and not returned, and leaves no trace of it:
    // what the use case checks beforehand is also checked with the sale locked.
    // Line 2 sold 1.5 and 0.5 of it came back: a return of all 1.5 fits what the domain knows of the sale
    // alone, and not what the store has.
    let sold_sale = sales.get(company, first.id()).await.unwrap();
    let too_big = SaleReturn::new(
        SaleReturnId::generate(),
        &sold_sale,
        return_draft(&[(2, "1.5")]),
        &[],
    )
    .unwrap();
    let refused_audit = AuditEntry {
        company_id: company,
        actor: ana.clone(),
        entity: AuditEntity::SaleReturn,
        entity_id: too_big.id().as_uuid(),
        action: AuditAction::Created,
        changes: sale_return_creation_changes(&too_big),
    };
    assert_eq!(
        store
            .save_return(&too_big, 2, &refused_audit)
            .await
            .unwrap_err(),
        RepositoryError::ReturnTooMuch
    );
    // A return worked out from fewer returns than the store has is refused: its refund would be wrong.
    assert_eq!(
        store
            .save_return(&too_big, 1, &refused_audit)
            .await
            .unwrap_err(),
        RepositoryError::SaleChanged
    );
    let stored = store.returns_of(company, first.id()).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|made| made.id() != too_big.id()));

    // What is refunded stands: a sale with returns is not cancelled.
    assert_eq!(
        sales.cancel(company, &ana, first.id()).await.unwrap_err(),
        ApplicationError::SaleHasReturns
    );

    // Cancelled: an open order just is; a completed sale takes its receivable with it unless it was settled.
    assert_eq!(
        sales
            .cancel(company, &ana, second.id())
            .await
            .unwrap()
            .status(),
        SaleStatus::Cancelled
    );
    assert_eq!(
        sales.cancel(company, &ana, second.id()).await.unwrap_err(),
        ApplicationError::Domain(DomainError::SaleIsLocked)
    );
    let entry = sales
        .receivable(company, single.id())
        .await
        .unwrap()
        .unwrap();
    let settlement = entries
        .settle(
            company,
            &ana,
            entry.id(),
            settlement_draft(entry_seed, 1, "10"),
        )
        .await
        .unwrap();
    assert_eq!(
        sales.cancel(company, &ana, single.id()).await.unwrap_err(),
        ApplicationError::SaleHasSettlements
    );
    assert_eq!(
        sales.get(company, single.id()).await.unwrap().status(),
        SaleStatus::Completed
    );
    entries
        .reverse(company, &ana, entry.id(), settlement.id())
        .await
        .unwrap();
    assert_eq!(
        sales
            .cancel(company, &ana, single.id())
            .await
            .unwrap()
            .status(),
        SaleStatus::Cancelled
    );
    assert!(
        sales
            .receivable(company, single.id())
            .await
            .unwrap()
            .unwrap()
            .is_voided()
    );
    assert_eq!(
        sales
            .complete(company, &ana, single.id())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::SaleIsLocked)
    );
    let overdue = entries
        .overdue(
            company,
            day("2028-01-01"),
            Some(EntryKind::Receivable),
            None,
        )
        .await
        .unwrap();
    assert!(overdue.items.iter().all(|item| item.entry_id != entry.id()));

    // A quote becomes one sale: its lines and customer are copied, it is marked converted, and asking again
    // gives the same sale. A declined one does not.
    let quote = quotes
        .register(
            company,
            &ana,
            QuoteDraft {
                customer_id: seed.customer,
                issued_on: "2026-11-06".into(),
                valid_until: "2026-11-20".into(),
                payment_term_id: Some(seed.term),
                notes: None,
                lines: vec![sale_line(seed.product, "Café 500g", "2", "10", "0")],
            },
        )
        .await
        .unwrap();
    let conversion = || Conversion {
        sold_on: "2026-11-07".into(),
        category_id: entry_seed.category,
        cost_center_id: None,
        seller_id: Some(seed.seller),
        payment_term_id: None,
    };
    let bad_conversion = Conversion {
        category_id: entry_seed.inactive_category,
        ..conversion()
    };
    assert_eq!(
        sales
            .convert_quote(company, &ana, quote.id(), bad_conversion)
            .await
            .unwrap_err(),
        refused("category")
    );
    assert_eq!(
        quotes.get(company, quote.id()).await.unwrap().status(),
        QuoteStatus::Draft
    );
    let converted = sales
        .convert_quote(company, &ana, quote.id(), conversion())
        .await
        .unwrap();
    assert_eq!(converted.quote_id(), Some(quote.id()));
    assert_eq!(converted.payment_term_id(), Some(seed.term));
    assert_eq!(converted.total().to_string(), "20");
    assert_eq!(converted.lines()[0].description(), "Café 500g");
    assert_eq!(
        quotes.get(company, quote.id()).await.unwrap().status(),
        QuoteStatus::Converted
    );
    let again = sales
        .convert_quote(company, &ana, quote.id(), conversion())
        .await
        .unwrap();
    assert_eq!(again.id(), converted.id());
    assert_eq!(
        sales
            .list(company, &SaleQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .len(),
        5
    );
    assert_eq!(
        sales
            .convert_quote(other_company, &ana, quote.id(), conversion())
            .await
            .unwrap_err(),
        ApplicationError::QuoteNotFound
    );
    assert_eq!(
        quotes
            .update(
                company,
                &ana,
                quote.id(),
                QuoteDraft {
                    customer_id: seed.customer,
                    issued_on: "2026-11-06".into(),
                    valid_until: "2026-11-20".into(),
                    payment_term_id: None,
                    notes: None,
                    lines: vec![sale_line(seed.product, "X", "1", "1", "0")],
                }
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::QuoteIsLocked)
    );
    let declined = quotes
        .register(
            company,
            &ana,
            QuoteDraft {
                customer_id: seed.customer,
                issued_on: "2026-11-06".into(),
                valid_until: "2026-11-20".into(),
                payment_term_id: None,
                notes: None,
                lines: vec![sale_line(seed.product, "Café", "1", "1", "0")],
            },
        )
        .await
        .unwrap();
    let link = quotes
        .send(
            company,
            &ana,
            declined.id(),
            "cliente@exemplo.com",
            "Padaria",
            day("2026-11-10"),
        )
        .await
        .unwrap();
    quotes
        .answer(
            link.link.rsplit('/').next().unwrap(),
            false,
            day("2026-11-10"),
        )
        .await
        .unwrap();
    assert_eq!(
        sales
            .convert_quote(company, &ana, declined.id(), conversion())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::QuoteIsLocked)
    );
}

fn contract_draft(
    entry_seed: &EntrySeed,
    seed: &SalesSeed,
    starts_on: &str,
    ends_on: Option<&str>,
) -> ContractDraft {
    ContractDraft {
        name: "Manutenção mensal".into(),
        customer_id: seed.customer,
        seller_id: Some(seed.seller),
        payment_term_id: Some(seed.term),
        category_id: entry_seed.category,
        cost_center_id: Some(entry_seed.cost_center),
        notes: None,
        lines: vec![sale_line(seed.product, "Suporte", "2", "100.10", "0.20")],
        cycle: BillingCycle::Monthly,
        starts_on: starts_on.into(),
        ends_on: ends_on.map(Into::into),
        active: true,
    }
}

/// What the sales of a contract are, by cycle day: `(day, total, completed)`.
async fn billed_by<S, Q, E, F, R, T>(
    sales: &SaleService<S, Q, E, F, R, T>,
    company: CompanyId,
    contract: conta_verde_domain::ContractId,
) -> Vec<(String, String, bool)>
where
    S: SaleRepository,
    Q: QuoteRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
{
    let mut found: Vec<(String, String, bool)> = sales
        .list(
            company,
            &SaleQuery::default().of_contract(contract),
            None,
            Some(200),
        )
        .await
        .unwrap()
        .items
        .iter()
        .map(|sale| {
            let cycle = sale.cycle().unwrap();
            assert_eq!(cycle.contract_id, contract);
            // A cycle is sold on its own day.
            assert_eq!(sale.sold_on(), cycle.cycle_on);
            (
                cycle.cycle_on.to_string(),
                sale.total().to_string(),
                sale.status() == SaleStatus::Completed,
            )
        })
        .collect();
    found.sort();
    found
}

fn cycles(items: &[(&str, &str)]) -> Vec<(String, String, bool)> {
    items
        .iter()
        .map(|(day, total)| ((*day).to_owned(), (*total).to_owned(), true))
        .collect()
}

/// A contract bills itself: every cycle that has fallen due becomes a completed sale and its receivable, once
/// however often the job runs, from where it starts to where it ends, never while paused, and in the name of
/// whoever saved it. An adjustment changes the cycles that come after it, a termination ends it for good, and
/// what it points at has to be usable. Nothing of another company's shows.
#[allow(clippy::too_many_arguments)]
pub async fn bills_contracts_every_cycle<C, S, Q, E, F, R, T>(
    contracts: &ContractService<C, S, Q, E, F, R, T>,
    sales: &SaleService<S, Q, E, F, R, T>,
    company: CompanyId,
    other_company: CompanyId,
    entry_seed: &EntrySeed,
    seed: &SalesSeed,
) where
    C: ContractRepository,
    S: SaleRepository,
    Q: QuoteRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
{
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();

    // Written before its first cycle: nothing is due. The trail has its creation.
    let rent = contracts
        .register(
            company,
            &ana,
            contract_draft(entry_seed, seed, "2026-11-05", None),
            day("2026-11-04"),
        )
        .await
        .unwrap();
    assert_eq!(rent.status(), ContractStatus::Active);
    assert_eq!(rent.total().to_string(), "200");
    assert!(billed_by(sales, company, rent.id()).await.is_empty());
    assert_eq!(
        contracts.history(company, rent.id()).await.unwrap().len(),
        1
    );

    // Cycle one, on its day: a completed sale and its receivable (the term 30/60 splits the 200).
    // The job reads the contracts of every company, so what it reports is not this test's alone: what it made
    // for this company is read from its sales.
    contracts.bill_all_due(day("2026-11-05")).await.unwrap();
    assert_eq!(
        billed_by(sales, company, rent.id()).await,
        cycles(&[("2026-11-05", "200")])
    );
    let first = sales
        .list(
            company,
            &SaleQuery::default().of_contract(rent.id()),
            None,
            None,
        )
        .await
        .unwrap()
        .items
        .remove(0);
    assert_eq!(first.seller_id(), Some(seed.seller));
    let receivable = sales
        .receivable(company, first.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receivable.total().to_string(), "200");
    assert_eq!(
        receivable.installments_text(),
        "1:2026-12-05:100,2:2027-01-04:100"
    );
    assert_eq!(
        sales.history(company, first.id()).await.unwrap()[0]
            .entry
            .actor,
        ana
    );
    // The same day again, and the job running twice at once, add nothing.
    contracts.bill_all_due(day("2026-11-05")).await.unwrap();
    assert_eq!(
        billed_by(sales, company, rent.id()).await,
        cycles(&[("2026-11-05", "200")])
    );
    let (a, b) = tokio::join!(
        contracts.bill_all_due(day("2026-12-05")),
        contracts.bill_all_due(day("2026-12-05"))
    );
    let _ = (a.unwrap(), b.unwrap());
    assert_eq!(
        billed_by(sales, company, rent.id()).await,
        cycles(&[("2026-11-05", "200"), ("2026-12-05", "200")])
    );

    // An adjustment changes the cycles that come after it, not those already billed.
    let raised = contracts
        .adjust(company, &ana, rent.id(), "10")
        .await
        .unwrap();
    // 100.10 x 1.10 = 110.11 and 2 x 110.11 less 0.20.
    assert_eq!(raised.total().to_string(), "220.02");
    assert_eq!(
        contracts
            .adjust(company, &ana, rent.id(), "-100")
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::InvalidAdjustment)
    );
    contracts.bill_all_due(day("2027-01-05")).await.unwrap();
    assert_eq!(
        billed_by(sales, company, rent.id()).await,
        cycles(&[
            ("2026-11-05", "200"),
            ("2026-12-05", "200"),
            ("2027-01-05", "220.02")
        ])
    );
    contracts.bill_all_due(day("2027-01-05")).await.unwrap();
    let trail = contracts.history(company, rent.id()).await.unwrap();
    assert_eq!(trail.len(), 2);
    assert!(
        trail[1]
            .entry
            .changes
            .iter()
            .any(|change| change.field == "lines")
    );

    // Paused, it bills nothing; brought back, it goes on from today and what fell due meanwhile is skipped.
    let mut paused_draft = contract_draft(entry_seed, seed, "2026-11-05", None);
    paused_draft.lines = raised.lines().iter().map(|line| line.to_draft()).collect();
    paused_draft.active = false;
    let paused = contracts
        .update(
            company,
            &ana,
            rent.id(),
            paused_draft.clone(),
            day("2027-01-06"),
        )
        .await
        .unwrap();
    assert_eq!(paused.status(), ContractStatus::Paused);
    contracts.bill_all_due(day("2027-03-10")).await.unwrap();
    assert_eq!(billed_by(sales, company, rent.id()).await.len(), 3);
    paused_draft.active = true;
    contracts
        .update(company, &ana, rent.id(), paused_draft, day("2027-03-10"))
        .await
        .unwrap();
    // 5 February and 5 March fell due while it was paused; the next is 5 April.
    assert_eq!(billed_by(sales, company, rent.id()).await.len(), 3);
    contracts.bill_all_due(day("2027-04-05")).await.unwrap();
    assert_eq!(
        billed_by(sales, company, rent.id())
            .await
            .last()
            .map(|(day, _, _)| day.clone()),
        Some("2027-04-05".to_owned())
    );
    assert_eq!(billed_by(sales, company, rent.id()).await.len(), 4);

    // Terminated: nothing more is billed and nothing changes.
    let ended = contracts
        .terminate(company, &ana, rent.id(), "2027-04-20", day("2027-04-10"))
        .await
        .unwrap();
    assert_eq!(ended.status(), ContractStatus::Terminated);
    contracts.bill_all_due(day("2028-01-01")).await.unwrap();
    assert_eq!(billed_by(sales, company, rent.id()).await.len(), 4);
    for attempt in [
        contracts
            .terminate(company, &ana, rent.id(), "2027-04-21", day("2027-04-10"))
            .await
            .map(|_| ()),
        contracts
            .adjust(company, &ana, rent.id(), "5")
            .await
            .map(|_| ()),
        contracts
            .update(
                company,
                &ana,
                rent.id(),
                contract_draft(entry_seed, seed, "2026-11-05", None),
                day("2027-05-01"),
            )
            .await
            .map(|_| ()),
    ] {
        assert_eq!(
            attempt.unwrap_err(),
            ApplicationError::Domain(DomainError::ContractIsTerminated)
        );
    }

    // Terminated ahead of its last day, it is fixed but still bills the cycles that fall due up to that day,
    // each on its own, and none after it.
    let ahead = contracts
        .register(
            company,
            &ana,
            contract_draft(entry_seed, seed, "2027-05-05", None),
            day("2027-05-10"),
        )
        .await
        .unwrap();
    contracts
        .terminate(company, &ana, ahead.id(), "2027-07-31", day("2027-05-10"))
        .await
        .unwrap();
    assert_eq!(
        billed_by(sales, company, ahead.id()).await,
        cycles(&[("2027-05-05", "200")])
    );
    contracts.bill_all_due(day("2027-12-01")).await.unwrap();
    assert_eq!(
        billed_by(sales, company, ahead.id()).await,
        cycles(&[
            ("2027-05-05", "200"),
            ("2027-06-05", "200"),
            ("2027-07-05", "200")
        ])
    );

    // A sale of a cycle that a person cancelled is consumed: the cycles after it are still billed.
    let hole = contracts
        .register(
            company,
            &ana,
            contract_draft(entry_seed, seed, "2027-08-05", None),
            day("2027-08-05"),
        )
        .await
        .unwrap();
    let only = sales
        .list(
            company,
            &SaleQuery::default().of_contract(hole.id()),
            None,
            None,
        )
        .await
        .unwrap()
        .items
        .remove(0);
    sales.cancel(company, &ana, only.id()).await.unwrap();
    contracts.bill_all_due(day("2027-10-05")).await.unwrap();
    let after_hole: Vec<String> = sales
        .list(
            company,
            &SaleQuery::default().of_contract(hole.id()),
            None,
            None,
        )
        .await
        .unwrap()
        .items
        .iter()
        .map(|sale| sale.cycle().unwrap().cycle_on.to_string())
        .collect();
    assert_eq!(after_hole.len(), 3);
    assert!(after_hole.contains(&"2027-10-05".to_owned()));

    // One that starts in the past catches up when it is written, clipping the day of a short month, and
    // never past its last day.
    let late = contracts
        .register(
            company,
            &ana,
            contract_draft(entry_seed, seed, "2026-01-31", Some("2026-04-15")),
            day("2026-06-01"),
        )
        .await
        .unwrap();
    assert_eq!(
        billed_by(sales, company, late.id()).await,
        cycles(&[
            ("2026-01-31", "200"),
            ("2026-02-28", "200"),
            ("2026-03-31", "200")
        ])
    );
    assert_eq!(
        late.billed_through().map(|d| d.to_string()),
        Some("2026-04-15".to_owned())
    );

    // What it points at has to be usable, and what is refused makes nothing.
    let refused = |what| ApplicationError::UnusableReference(what);
    for (what, change) in [
        (
            "customer",
            Box::new(|d: &mut ContractDraft| d.customer_id = PartyId::generate())
                as Box<dyn Fn(&mut ContractDraft)>,
        ),
        (
            "seller",
            Box::new(|d: &mut ContractDraft| d.seller_id = Some(d.customer_id)),
        ),
        (
            "payment term",
            Box::new(|d: &mut ContractDraft| d.payment_term_id = Some(seed.inactive_term)),
        ),
        (
            "product",
            Box::new(|d: &mut ContractDraft| d.lines[0].product_id = seed.inactive_product),
        ),
    ] {
        let mut bad = contract_draft(entry_seed, seed, "2026-11-05", None);
        change(&mut bad);
        assert_eq!(
            contracts
                .register(company, &ana, bad, day("2026-11-05"))
                .await
                .unwrap_err(),
            refused(what),
            "{what}"
        );
    }
    let mut bad = contract_draft(entry_seed, seed, "2026-11-05", None);
    bad.category_id = entry_seed.transfer_category;
    assert_eq!(
        contracts
            .register(company, &ana, bad, day("2026-11-05"))
            .await
            .unwrap_err(),
        refused("category")
    );
    let mut bad = contract_draft(entry_seed, seed, "2026-11-05", Some("2026-11-04"));
    bad.name = "  ".into();
    assert!(
        contracts
            .register(company, &ana, bad, day("2026-11-05"))
            .await
            .is_err()
    );

    // Nothing of another company's shows; listed by name, status and text.
    assert_eq!(
        contracts.get(other_company, rent.id()).await.unwrap_err(),
        ApplicationError::ContractNotFound
    );
    assert!(
        contracts
            .list(other_company, &ContractQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let all = contracts
        .list(company, &ContractQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(all.items.len(), 4);
    let live = contracts
        .list(
            company,
            &ContractQuery::default().with_status(ContractStatus::Terminated),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(live.items.len(), 2);
    assert_eq!(
        contracts
            .list(
                company,
                &ContractQuery::default().searching("MENSAL"),
                None,
                None
            )
            .await
            .unwrap()
            .items
            .len(),
        4
    );
}

fn rule_draft(
    seller: Option<PartyId>,
    product: Option<ProductId>,
    rate: &str,
    starts_on: &str,
    ends_on: Option<&str>,
) -> CommissionRuleDraft {
    CommissionRuleDraft {
        seller_id: seller,
        product_id: product,
        rate: rate.into(),
        starts_on: starts_on.into(),
        ends_on: ends_on.map(Into::into),
        active: true,
    }
}

/// Commission rules are a register of their own: a rate between two days for a seller and a product or for
/// all, which has to name what is usable. The statement of a span adds up the completed sales of each seller
/// line by line, net of what came back, by the narrowest rule in force on the day of each sale; a sale
/// without a seller, an order and a cancelled sale earn nothing, and a rule that ends does not rewrite what it
/// earned before. Nothing of another company's shows.
pub async fn computes_commission_statements<CR, S2, R2, S, Q, E, F, R, T>(
    commissions: &CommissionService<CR, S2, R2>,
    sales: &SaleService<S, Q, E, F, R, T>,
    company: CompanyId,
    other_company: CompanyId,
    entry_seed: &EntrySeed,
    seed: &SalesSeed,
) where
    CR: CommissionRuleRepository,
    S2: SaleRepository,
    R2: DocumentReferences,
    S: SaleRepository,
    Q: QuoteRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
{
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    let vera = seed.seller;

    // What a rule is, and what it may name.
    let general = commissions
        .register(
            company,
            &ana,
            rule_draft(None, None, "5", "2026-01-01", None),
        )
        .await
        .unwrap();
    let vera_coffee = commissions
        .register(
            company,
            &ana,
            rule_draft(Some(vera), Some(seed.product), "8", "2026-01-01", None),
        )
        .await
        .unwrap();
    assert_eq!(vera_coffee.rate().to_string(), "8");
    assert_eq!(
        commissions
            .history(company, general.id())
            .await
            .unwrap()
            .len(),
        1
    );
    for (rate, error) in [
        ("0", ApplicationError::Domain(DomainError::InvalidPercent)),
        (
            "100.5",
            ApplicationError::Domain(DomainError::InvalidPercent),
        ),
    ] {
        assert_eq!(
            commissions
                .register(
                    company,
                    &ana,
                    rule_draft(None, None, rate, "2026-01-01", None)
                )
                .await
                .unwrap_err(),
            error
        );
    }
    assert_eq!(
        commissions
            .register(
                company,
                &ana,
                rule_draft(Some(seed.customer), None, "5", "2026-01-01", None)
            )
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("seller")
    );
    assert_eq!(
        commissions
            .register(
                company,
                &ana,
                rule_draft(None, Some(seed.inactive_product), "5", "2026-01-01", None)
            )
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("product")
    );
    assert_eq!(
        commissions
            .get(other_company, general.id())
            .await
            .unwrap_err(),
        ApplicationError::CommissionRuleNotFound
    );
    assert!(
        commissions
            .list(other_company, &CommissionRuleQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let listed = |query: CommissionRuleQuery| {
        let commissions = &commissions;
        async move {
            commissions
                .list(company, &query, None, None)
                .await
                .unwrap()
                .items
                .len()
        }
    };
    assert_eq!(listed(CommissionRuleQuery::default()).await, 2);
    assert_eq!(
        listed(CommissionRuleQuery::default().with_seller(vera)).await,
        1
    );
    assert_eq!(
        listed(CommissionRuleQuery::default().with_product(seed.product)).await,
        1
    );
    assert_eq!(
        listed(CommissionRuleQuery::default().with_active(false)).await,
        0
    );
    // A change records only what changed; the same body again records nothing.
    commissions
        .update(
            company,
            &ana,
            general.id(),
            rule_draft(None, None, "5", "2026-01-01", None),
        )
        .await
        .unwrap();
    assert_eq!(
        commissions
            .history(company, general.id())
            .await
            .unwrap()
            .len(),
        1
    );

    // Sales: one that earns, one that does not for want of a seller, an order, and a cancelled one.
    let sold = |sold_on: &'static str| {
        let mut draft = sale_draft(entry_seed, seed);
        draft.sold_on = sold_on.into();
        draft
    };
    let a = sales
        .register(company, &ana, sold("2026-11-06"))
        .await
        .unwrap();
    let b = sales
        .register(company, &ana, sold("2026-12-15"))
        .await
        .unwrap();
    let mut nobody = sold("2026-11-07");
    nobody.seller_id = None;
    let c = sales.register(company, &ana, nobody).await.unwrap();
    let d = sales
        .register(company, &ana, sold("2026-11-08"))
        .await
        .unwrap();
    let _order = sales
        .register(company, &ana, sold("2026-11-09"))
        .await
        .unwrap();
    for sale in [&a, &b, &c, &d] {
        sales.complete(company, &ana, sale.id()).await.unwrap();
    }
    sales.cancel(company, &ana, d.id()).await.unwrap();

    let in_november = commissions
        .statement(company, None, day("2026-11-01"), day("2026-11-30"))
        .await
        .unwrap();
    assert_eq!(in_november.sellers.len(), 1);
    let one = &in_november.sellers[0];
    assert_eq!(one.seller_id, vera);
    assert_eq!(one.sales.len(), 1);
    assert_eq!(one.sales[0].sale.number(), a.number());
    // 30 of coffee at 8% and 5 of bread at the general 5%: 2.4 and 0.25.
    assert_eq!(
        (one.base.to_string(), one.commission.to_string()),
        ("35".to_owned(), "2.65".to_owned())
    );
    assert_eq!(
        (
            in_november.base.to_string(),
            in_november.commission.to_string()
        ),
        ("35".to_owned(), "2.65".to_owned())
    );

    // What came back is taken off the base before the rate: one coffee of three refunds 10.
    sales
        .register_return(
            company,
            &ana,
            a.id(),
            return_draft(&[(1, "1")]),
            Refund {
                category_id: entry_seed.other_category,
                due_on: "2026-11-20".into(),
            },
        )
        .await
        .unwrap();
    let returned = commissions
        .statement(company, Some(vera), day("2026-11-01"), day("2026-11-30"))
        .await
        .unwrap();
    assert_eq!(
        (returned.base.to_string(), returned.commission.to_string()),
        ("25".to_owned(), "1.85".to_owned())
    );
    // The seller filter and the span narrow it; another company's has nothing; a backwards span is refused.
    for (seller, from, to) in [
        (Some(seed.customer), "2026-11-01", "2026-12-31"),
        (None, "2026-10-01", "2026-10-31"),
        (None, "2026-11-07", "2026-11-30"),
    ] {
        let none = commissions
            .statement(company, seller, day(from), day(to))
            .await
            .unwrap();
        assert!(none.sellers.is_empty(), "{seller:?} {from} {to}");
        assert_eq!(none.commission.to_string(), "0");
    }
    assert!(
        commissions
            .statement(other_company, None, day("2026-01-01"), day("2026-12-31"))
            .await
            .unwrap()
            .sellers
            .is_empty()
    );
    assert!(matches!(
        commissions
            .statement(company, None, day("2026-12-01"), day("2026-11-01"))
            .await
            .unwrap_err(),
        ApplicationError::InvalidSpan(_)
    ));

    // A rule that ends changes what comes after it, never what it earned before.
    commissions
        .update(
            company,
            &ana,
            general.id(),
            rule_draft(None, None, "5", "2026-01-01", Some("2026-11-30")),
        )
        .await
        .unwrap();
    assert_eq!(
        commissions
            .history(company, general.id())
            .await
            .unwrap()
            .len(),
        2
    );
    let both = commissions
        .statement(company, None, day("2026-11-01"), day("2026-12-31"))
        .await
        .unwrap();
    assert_eq!(both.sellers.len(), 1);
    assert_eq!(both.sellers[0].sales.len(), 2);
    // November as before (1.6 and 0.25); December: coffee at 8% is 2.4 and bread has no rule left.
    assert_eq!(
        (both.base.to_string(), both.commission.to_string()),
        ("60".to_owned(), "4.25".to_owned())
    );
    let rows: Vec<(i64, String)> = both.sellers[0]
        .sales
        .iter()
        .map(|row| (row.sale.number(), row.commission.to_string()))
        .collect();
    assert_eq!(
        rows,
        [
            (a.number(), "1.85".to_owned()),
            (b.number(), "2.4".to_owned())
        ]
    );
}

// ---- price lists

fn price_list_draft(name: &str, prices: &[(ProductId, &str)], active: bool) -> PriceListDraft {
    PriceListDraft {
        name: name.into(),
        items: prices
            .iter()
            .map(|(product_id, price)| PriceListItemDraft {
                product_id: *product_id,
                price: (*price).into(),
            })
            .collect(),
        active,
    }
}

async fn listed_price_list_names<R: PriceListRepository, F: DocumentReferences>(
    service: &PriceListService<R, F>,
    company: CompanyId,
    query: &PriceListQuery,
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

/// A list is stored with its prices in the order they were written and its trail, scoped by company; an edit
/// replaces the prices as a whole and records only what changed; a refused write leaves the list, its prices and
/// its trail as they were.
pub async fn runs_the_price_list_lifecycle<R: PriceListRepository, F: DocumentReferences>(
    service: &PriceListService<R, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &SalesSeed,
) {
    let who = actor();
    let (coffee, bread) = (seed.product, seed.other_product);
    let created = service
        .register(
            company,
            &who,
            price_list_draft("  Atacado ", &[(coffee, "10.50"), (bread, " 3 ")], true),
        )
        .await
        .unwrap();
    assert_eq!(created.name(), "Atacado");
    // What comes back is what went in: the prices, in order, as the same numbers whichever adapter keeps them.
    assert_eq!(service.get(company, created.id()).await.unwrap(), created);
    assert_eq!(
        created.items_text(),
        format!("{}:10.5\n{}:3", coffee.as_uuid(), bread.as_uuid())
    );
    assert_eq!(
        service.get(other_company, created.id()).await,
        Err(ApplicationError::PriceListNotFound)
    );
    assert_eq!(
        service.history(other_company, created.id()).await,
        Err(ApplicationError::PriceListNotFound)
    );
    let trail = |id| async move { service.history(company, id).await.unwrap().len() };
    assert_eq!(trail(created.id()).await, 1);

    // The same name, whatever its case, is refused: no list, no prices, no entry.
    assert_eq!(
        service
            .register(
                company,
                &who,
                price_list_draft("ATACADO", &[(coffee, "1")], true)
            )
            .await,
        Err(ApplicationError::Repository(RepositoryError::DuplicateName))
    );
    // A product that cannot be used, or that is not the company's, is refused.
    for unusable in [seed.inactive_product, ProductId::generate()] {
        assert_eq!(
            service
                .register(
                    company,
                    &who,
                    price_list_draft("Outra", &[(unusable, "1")], true)
                )
                .await,
            Err(ApplicationError::UnusableReference("product"))
        );
    }

    // The same prices written differently are no change and write nothing.
    service
        .update(
            company,
            &who,
            created.id(),
            price_list_draft("Atacado", &[(coffee, "10.500"), (bread, "3.0")], true),
        )
        .await
        .unwrap();
    assert_eq!(trail(created.id()).await, 1);

    // An edit replaces the prices as a whole, and the trail shows what changed.
    let updated = service
        .update(
            company,
            &who,
            created.id(),
            price_list_draft("Atacado", &[(bread, "2.75")], false),
        )
        .await
        .unwrap();
    assert_eq!(service.get(company, created.id()).await.unwrap(), updated);
    assert_eq!(updated.items().len(), 1);
    let history = service.history(company, created.id()).await.unwrap();
    assert_eq!(history.len(), 2);
    let changed: Vec<&str> = history[1]
        .entry
        .changes
        .iter()
        .map(|change| change.field.as_str())
        .collect();
    assert_eq!(changed, ["items", "active"]);

    // A refused edit (a product twice, a clashing name, an unusable product) changes nothing and records nothing.
    let other = service
        .register(
            company,
            &who,
            price_list_draft("Varejo", &[(coffee, "12")], true),
        )
        .await
        .unwrap();
    for (draft, expected) in [
        (
            price_list_draft("Varejo", &[(coffee, "1"), (coffee, "2")], true),
            ApplicationError::Domain(DomainError::DuplicatePriceListProduct),
        ),
        (
            price_list_draft("atacado", &[(coffee, "12")], true),
            ApplicationError::Repository(RepositoryError::DuplicateName),
        ),
        (
            price_list_draft(
                "Varejo",
                &[(coffee, "12"), (seed.inactive_product, "1")],
                true,
            ),
            ApplicationError::UnusableReference("product"),
        ),
    ] {
        assert_eq!(
            service.update(company, &who, other.id(), draft).await,
            Err(expected)
        );
    }
    assert_eq!(service.get(company, other.id()).await.unwrap(), other);
    assert_eq!(trail(other.id()).await, 1);
    assert_eq!(
        service
            .update(
                other_company,
                &who,
                other.id(),
                price_list_draft("Varejo", &[(coffee, "12")], true)
            )
            .await,
        Err(ApplicationError::PriceListNotFound)
    );

    // Filters, search and order follow ADR 0018, and a page holds each list's own prices.
    let all = PriceListQuery::default();
    let names = |query: PriceListQuery| async move {
        listed_price_list_names(service, company, &query, 1).await
    };
    assert_eq!(names(all.clone()).await, ["Atacado", "Varejo"]);
    assert_eq!(
        names(all.clone().sorted(NameSort::NameDesc)).await,
        ["Varejo", "Atacado"]
    );
    assert_eq!(names(all.clone().with_active(true)).await, ["Varejo"]);
    assert_eq!(names(all.clone().with_active(false)).await, ["Atacado"]);
    assert_eq!(names(all.clone().searching("VAR")).await, ["Varejo"]);
    assert!(names(all.clone().searching("%")).await.is_empty());
    let page = service.list(company, &all, None, None).await.unwrap();
    let texts: Vec<usize> = page.items.iter().map(|list| list.items().len()).collect();
    assert_eq!(texts, [1, 1]);
    assert!(
        service
            .list(other_company, &all, None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

/// Charges for the installments of receivables ([ADR 0045]): made at the provider for what is left of an
/// installment, one open at a time, called off at the provider and here, and never of another company's.
#[allow(clippy::too_many_arguments)]
pub async fn issues_and_cancels_charges<C, E, P, F, S>(
    service: &ChargeService<C, E, P, FakePaymentProvider, S>,
    store: &C,
    entries: &FinancialEntryService<E, F>,
    settings: &CollectionService<S, F>,
    provider: &FakePaymentProvider,
    parties: &P,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    C: ChargeRepository,
    E: FinancialEntryRepository,
    P: PartyRepository,
    F: EntryReferences,
    S: CollectionSettingsRepository,
{
    let ana = actor();
    let register = |description: &str, party: Option<PartyId>, kind: EntryKind| {
        let mut draft = entry_draft(
            seed,
            description,
            "2026-11-10",
            &[("2026-12-10", "100"), ("2027-01-10", "50.50")],
        );
        draft.party_id = party;
        draft.kind = kind;
        entries.register(company, &ana, draft)
    };
    let entry = register("Venda 12", Some(seed.party), EntryKind::Receivable)
        .await
        .unwrap();

    // Without an account to land in, nothing is asked of the provider.
    assert_eq!(
        service
            .issue(company, &ana, entry.id(), 1, ChargeMethod::Pix, None)
            .await
            .unwrap_err(),
        ApplicationError::CollectionNotConfigured
    );
    assert!(provider.created().is_empty());
    settings
        .replace(company, &ana, seed.bank_account, None)
        .await
        .unwrap();

    // A Pix charge for what the installment is worth, as the provider made it, audited as it was created.
    let pix = service
        .issue(company, &ana, entry.id(), 1, ChargeMethod::Pix, None)
        .await
        .unwrap();
    assert_eq!(pix.status(), ChargeStatus::Open);
    assert_eq!(pix.amount().to_string(), "100");
    assert_eq!(pix.due_on().to_string(), "2026-12-10");
    assert_eq!(pix.provider(), "fake");
    assert_eq!(pix.issued_by(), &ana);
    assert!(pix.payment().pix_payload.is_some());
    let asked = provider.created();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].charge_id, pix.id());
    assert_eq!(asked[0].description, "Venda 12 (1/2)");
    assert_eq!(asked[0].payer.name, "Mercado Boa Compra");
    assert_eq!(
        service.get(company, entry.id(), pix.id()).await.unwrap(),
        pix
    );
    let trail = service
        .history(company, entry.id(), pix.id())
        .await
        .unwrap();
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].entry.action, AuditAction::Created);
    assert_eq!(trail[0].entry.entity, AuditEntity::Charge);

    // One open charge per installment: the second is refused before the provider is asked.
    assert_eq!(
        service
            .issue(company, &ana, entry.id(), 1, ChargeMethod::Boleto, None)
            .await
            .unwrap_err(),
        ApplicationError::Repository(RepositoryError::ChargeAlreadyOpen)
    );
    assert_eq!(provider.created().len(), 1, "the provider is not asked");
    assert!(provider.cancelled().is_empty());
    assert_eq!(
        service.list(company, entry.id()).await.unwrap(),
        vec![pix.clone()]
    );

    // Another installment takes its own, on the day the person says.
    let boleto = service
        .issue(
            company,
            &ana,
            entry.id(),
            2,
            ChargeMethod::Boleto,
            CalendarDate::parse("2027-01-15"),
        )
        .await
        .unwrap();
    assert_eq!(boleto.amount().to_string(), "50.5");
    assert_eq!(boleto.due_on().to_string(), "2027-01-15");
    assert!(boleto.payment().digitable_line.is_some());

    // Calling off goes to the provider first, then is recorded; one that is off cannot be again.
    let cancelled = service
        .cancel(company, &ana, entry.id(), pix.id())
        .await
        .unwrap();
    assert_eq!(cancelled.status(), ChargeStatus::Cancelled);
    assert!(provider.cancelled().contains(&pix.external_id().to_owned()));
    assert_eq!(
        service.get(company, entry.id(), pix.id()).await.unwrap(),
        cancelled
    );
    assert_eq!(
        service
            .cancel(company, &ana, entry.id(), pix.id())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::ChargeIsNotOpen)
    );
    let trail = service
        .history(company, entry.id(), pix.id())
        .await
        .unwrap();
    assert_eq!(
        trail
            .iter()
            .map(|record| record.entry.action)
            .collect::<Vec<_>>(),
        [AuditAction::Created, AuditAction::Updated]
    );
    assert_eq!(trail[1].entry.changes.len(), 1);
    assert_eq!(trail[1].entry.changes[0].field, "status");

    // The installment is charged again for what is left of it after a part was settled.
    entries
        .settle(company, &ana, entry.id(), settlement_draft(seed, 1, "40"))
        .await
        .unwrap();
    let card = service
        .issue(company, &ana, entry.id(), 1, ChargeMethod::CardLink, None)
        .await
        .unwrap();
    assert_eq!(card.amount().to_string(), "60");
    assert!(card.payment().payment_url.is_some());
    assert_eq!(
        service.list(company, entry.id()).await.unwrap(),
        vec![cancelled, boleto.clone(), card.clone()],
        "in the order they were issued"
    );
    service
        .cancel(company, &ana, entry.id(), card.id())
        .await
        .unwrap();
    entries
        .settle(company, &ana, entry.id(), settlement_draft(seed, 1, "60"))
        .await
        .unwrap();
    assert_eq!(
        service
            .issue(company, &ana, entry.id(), 1, ChargeMethod::Pix, None)
            .await
            .unwrap_err(),
        ApplicationError::NothingToCharge
    );

    // What cannot be charged: an installment the entry lacks, a payable, an entry without a customer, a
    // cancelled entry. None of them reaches the provider.
    let asked = provider.created().len();
    let payable = register("Compra", Some(seed.party), EntryKind::Payable)
        .await
        .unwrap();
    let nobody = register("Venda sem cliente", None, EntryKind::Receivable)
        .await
        .unwrap();
    let voided = register("Venda cancelada", Some(seed.party), EntryKind::Receivable)
        .await
        .unwrap();
    entries.void(company, &ana, voided.id()).await.unwrap();
    for (entry_id, number, error) in [
        (
            entry.id(),
            9,
            ApplicationError::UnusableReference("installment"),
        ),
        (payable.id(), 1, ApplicationError::ChargeNeedsReceivable),
        (nobody.id(), 1, ApplicationError::ChargeNeedsParty),
        (voided.id(), 1, ApplicationError::EntryIsVoided),
    ] {
        assert_eq!(
            service
                .issue(company, &ana, entry_id, number, ChargeMethod::Pix, None)
                .await
                .unwrap_err(),
            error
        );
    }
    assert_eq!(provider.created().len(), asked);

    // The payer's address travels with the charge, for the providers whose boleto needs it.
    let mut living = party_draft("Cliente com endereço", None);
    living.address = Some(AddressDraft {
        postal_code: "01310-100".into(),
        street: "Av. Paulista".into(),
        number: Some("1000".into()),
        complement: None,
        district: Some("Bela Vista".into()),
        city: "São Paulo".into(),
        state: "SP".into(),
    });
    let resident = Party::new(PartyId::generate(), company, living).unwrap();
    parties
        .save(&resident, &created_entry(&resident))
        .await
        .unwrap();
    let mut draft = entry_draft(
        seed,
        "Venda com endereço",
        "2026-11-10",
        &[("2026-12-10", "10")],
    );
    draft.party_id = Some(resident.id());
    let located = entries.register(company, &ana, draft).await.unwrap();
    service
        .issue(company, &ana, located.id(), 1, ChargeMethod::Boleto, None)
        .await
        .unwrap();
    let address = provider.created().pop().unwrap().payer.address.unwrap();
    assert_eq!(address.street, "Av. Paulista, 1000");
    assert_eq!(address.city, "São Paulo");
    assert_eq!(address.state, "SP");
    assert_eq!(address.postal_code, "01310100");

    // A provider that fails leaves nothing behind, and says why.
    service
        .cancel(company, &ana, entry.id(), boleto.id())
        .await
        .unwrap();
    provider.failing_with(ProviderError::Unavailable("timeout".into()));
    assert_eq!(
        service
            .issue(company, &ana, entry.id(), 2, ChargeMethod::Pix, None)
            .await
            .unwrap_err(),
        ApplicationError::Provider(ProviderError::Unavailable("timeout".into()))
    );
    provider.works_again();
    assert_eq!(service.list(company, entry.id()).await.unwrap().len(), 3);

    // Nothing of another company's is seen, issued or called off.
    for error in [
        service.list(other_company, entry.id()).await.unwrap_err(),
        service
            .issue(other_company, &ana, entry.id(), 2, ChargeMethod::Pix, None)
            .await
            .unwrap_err(),
    ] {
        assert_eq!(error, ApplicationError::EntryNotFound);
    }
    for error in [
        service
            .get(
                other_company,
                entry.id(),
                boleto_id(service, company, entry.id()).await,
            )
            .await
            .unwrap_err(),
        service
            .cancel(
                other_company,
                &ana,
                entry.id(),
                boleto_id(service, company, entry.id()).await,
            )
            .await
            .unwrap_err(),
    ] {
        assert_eq!(error, ApplicationError::ChargeNotFound);
    }
    assert!(
        store
            .list_by_entry(other_company, entry.id())
            .await
            .unwrap()
            .is_empty()
    );
}

async fn boleto_id<C, E, P, S>(
    service: &ChargeService<C, E, P, FakePaymentProvider, S>,
    company: CompanyId,
    entry: conta_verde_domain::EntryId,
) -> conta_verde_domain::ChargeId
where
    C: ChargeRepository,
    E: FinancialEntryRepository,
    P: PartyRepository,
    S: CollectionSettingsRepository,
{
    service
        .list(company, entry)
        .await
        .unwrap()
        .iter()
        .find(|charge| charge.method() == ChargeMethod::Boleto)
        .unwrap()
        .id()
}

/// The collection settings ([ADR 0046]): one per company, replaced as a whole, pointing at a usable account
/// and category of the company, audited, and nothing of another company's.
pub async fn configures_collection<S, F>(
    service: &CollectionService<S, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    S: CollectionSettingsRepository,
    F: EntryReferences,
{
    let ana = actor();
    assert_eq!(service.get(company).await.unwrap(), None);

    // Usable references only: an inactive account, the transfers category, another company's records.
    for (account, category, what) in [
        (seed.inactive_bank_account, None, "bank account"),
        (seed.bank_account, Some(seed.inactive_category), "category"),
        (seed.bank_account, Some(seed.transfer_category), "category"),
    ] {
        assert_eq!(
            service
                .replace(company, &ana, account, category)
                .await
                .unwrap_err(),
            ApplicationError::UnusableReference(what)
        );
    }
    assert_eq!(
        service
            .replace(other_company, &ana, seed.bank_account, None)
            .await
            .unwrap_err(),
        ApplicationError::UnusableReference("bank account")
    );
    assert_eq!(service.get(company).await.unwrap(), None);
    assert!(service.history(company).await.unwrap().is_empty());

    let first = service
        .replace(company, &ana, seed.bank_account, None)
        .await
        .unwrap();
    assert_eq!(first.bank_account_id(), seed.bank_account);
    assert_eq!(first.fee_category_id(), None);
    assert_eq!(service.get(company).await.unwrap(), Some(first));

    // Replaced as a whole; the same settings again write nothing.
    let second = service
        .replace(
            company,
            &ana,
            seed.other_bank_account,
            Some(seed.other_category),
        )
        .await
        .unwrap();
    assert_eq!(service.get(company).await.unwrap(), Some(second.clone()));
    service
        .replace(
            company,
            &ana,
            seed.other_bank_account,
            Some(seed.other_category),
        )
        .await
        .unwrap();
    let trail = service.history(company).await.unwrap();
    assert_eq!(
        trail
            .iter()
            .map(|record| record.entry.action)
            .collect::<Vec<_>>(),
        [AuditAction::Created, AuditAction::Updated]
    );
    assert_eq!(trail[1].entry.changes.len(), 2);

    // Another company has its own, and none.
    assert_eq!(service.get(other_company).await.unwrap(), None);
    assert!(service.history(other_company).await.unwrap().is_empty());
}

/// What a webhook does ([ADR 0046]): a signed payment settles the installment through the settlement path and
/// enters the fee, once however many times it arrives or is interrupted; what is not signed, not known or
/// another company's is refused and leaves nothing; what no longer fits is kept and flagged.
#[allow(clippy::too_many_arguments)]
pub async fn settles_charges_from_webhooks<C, Ev, S, E, F, P>(
    webhooks: &PaymentWebhookService<FakePaymentProvider, C, Ev, S, E, F>,
    charges: &ChargeService<C, E, P, FakePaymentProvider, S>,
    entries: &FinancialEntryService<E, F>,
    settings: &CollectionService<S, F>,
    events: &Ev,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    C: ChargeRepository,
    Ev: PaymentEventRepository,
    S: CollectionSettingsRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    P: PartyRepository,
{
    use conta_verde_domain::{Charge, FinancialEntry};

    let ana = actor();
    settings
        .replace(company, &ana, seed.bank_account, Some(seed.other_category))
        .await
        .unwrap();
    let new_entry = |description: &'static str, amounts: &'static [&'static str]| {
        let parts: Vec<(&str, &str)> = amounts.iter().map(|a| ("2026-12-10", *a)).collect();
        let mut draft = entry_draft(seed, description, "2026-11-10", &parts);
        draft.party_id = Some(seed.party);
        entries.register(company, &ana, draft)
    };
    let charge_for = |entry: &FinancialEntry, number: u16| {
        charges.issue(company, &ana, entry.id(), number, ChargeMethod::Pix, None)
    };
    let pay = |charge: &Charge, event: &str, paid: &str, fee: &str| {
        let (headers, body) = FakePaymentProvider::notice(
            event,
            charge.external_id(),
            "paid",
            Some((paid, "2026-12-09", fee)),
        );
        async move { webhooks.receive(company, "fake", &headers, &body).await }
    };
    let settlements_of = |entry: &FinancialEntry| entries.settlements(company, entry.id());
    let status_of = |charge: &Charge| {
        let id = charge.id();
        let entry_id = charge.entry_id();
        async move { charges.get(company, entry_id, id).await.unwrap() }
    };

    // A payment of exactly the charge settles the installment, from the account, in the issuer's name,
    // and enters the fee as a payable settled the same day.
    let sale = new_entry("Venda 12", &["100", "50.50"]).await.unwrap();
    let first = charge_for(&sale, 1).await.unwrap();
    assert_eq!(pay(&first, "evt_1", "100", "1.99").await, Ok(1));
    let made = settlements_of(&sale).await.unwrap();
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].installment(), 1);
    assert_eq!(made[0].applied().to_string(), "100");
    assert_eq!(made[0].cash().to_string(), "100");
    assert_eq!(made[0].settled_on().to_string(), "2026-12-09");
    assert_eq!(made[0].bank_account_id(), seed.bank_account);
    assert_eq!(made[0].charge_id(), Some(first.id()));
    let paid = status_of(&first).await;
    assert_eq!(paid.status(), ChargeStatus::Paid);
    let receipt = paid.receipt().unwrap();
    assert_eq!(receipt.paid_amount.to_string(), "100");
    assert_eq!(receipt.fee.to_string(), "1.99");
    assert_eq!(receipt.paid_on.to_string(), "2026-12-09");
    assert_eq!(receipt.note, None);
    let fee = entries
        .made_by(company, EntryOrigin::ChargeFee(first.id()))
        .await
        .unwrap()
        .expect("the fee is an entry");
    assert_eq!(fee.kind(), EntryKind::Payable);
    assert_eq!(fee.description(), "Taxa de cobrança: Venda 12 (1/2)");
    assert_eq!(fee.category_id(), seed.other_category);
    assert_eq!(fee.total().to_string(), "1.99");
    let fee_settlements = settlements_of(&fee).await.unwrap();
    assert_eq!(fee_settlements.len(), 1);
    assert_eq!(fee_settlements[0].cash().to_string(), "1.99");
    assert_eq!(fee_settlements[0].bank_account_id(), seed.bank_account);
    assert_eq!(
        outcome_of(events, company, &first, "evt_1").await,
        (PaymentEventOutcome::Applied, None)
    );

    // The same notice again settles nothing more, and neither is another call of it.
    assert_eq!(pay(&first, "evt_1", "100", "1.99").await, Ok(1));
    assert_eq!(settlements_of(&sale).await.unwrap().len(), 1);
    assert_eq!(settlements_of(&fee).await.unwrap().len(), 1);

    // Another notice that says it was paid again is kept and flagged, never settled again.
    assert_eq!(pay(&first, "evt_dup", "100", "1.99").await, Ok(1));
    assert_eq!(settlements_of(&sale).await.unwrap().len(), 1);
    assert_eq!(
        outcome_of(events, company, &first, "evt_dup").await,
        (
            PaymentEventOutcome::NotApplied,
            Some("the charge was already paid".to_owned())
        )
    );

    // A signed notice that changes nothing is acknowledged, and stores nothing.
    let (headers, body) = FakePaymentProvider::notice("evt_ping", "fake_none", "ping", None);
    assert_eq!(
        webhooks.receive(company, "fake", &headers, &body).await,
        Ok(0)
    );

    // Not signed, not a notice, a charge nobody knows, a provider that is not the one, and another
    // company's address: all refused, and nothing is stored.
    let second = charge_for(&sale, 2).await.unwrap();
    let (headers, body) = FakePaymentProvider::notice(
        "evt_forged",
        second.external_id(),
        "paid",
        Some(("50.5", "2026-12-09", "0")),
    );
    let mut forged = body.clone();
    forged.push(b' ');
    assert_eq!(
        webhooks.receive(company, "fake", &headers, &forged).await,
        Err(ApplicationError::Webhook(WebhookError::InvalidSignature))
    );
    let (unknown_headers, unknown_body) = FakePaymentProvider::notice(
        "evt_unknown",
        "fake_nobody",
        "paid",
        Some(("1", "2026-12-09", "0")),
    );
    // The provider that is not the configured one is not found; a charge that is not the company's is left out
    // (answered Ok(0), so a provider account that also has payments made elsewhere does not pause its queue).
    assert_eq!(
        webhooks.receive(company, "other", &headers, &body).await,
        Err(ApplicationError::ChargeNotFound)
    );
    for (who, headers, body) in [
        (company, &unknown_headers, &unknown_body),
        (other_company, &headers, &body),
    ] {
        assert_eq!(webhooks.receive(who, "fake", headers, body).await, Ok(0));
    }
    assert_eq!(status_of(&second).await.status(), ChargeStatus::Open);
    for event in ["evt_forged", "evt_unknown"] {
        let probe = PaymentEvent::new(
            PaymentEventId::generate(),
            company,
            second.id(),
            "fake",
            event,
            PaymentEventKind::Paid,
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            events.record(&probe).await.unwrap(),
            Recorded::New(_)
        ));
    }

    // Paid over the charge, the difference is interest, and with no fee there is no fee entry.
    assert_eq!(pay(&second, "evt_2", "52", "0").await, Ok(1));
    let over = settlements_of(&sale).await.unwrap();
    let over = over.iter().find(|s| s.installment() == 2).unwrap();
    assert_eq!(over.applied().to_string(), "50.5");
    assert_eq!(over.interest().to_string(), "1.5");
    assert_eq!(over.cash().to_string(), "52");
    assert!(
        entries
            .made_by(company, EntryOrigin::ChargeFee(second.id()))
            .await
            .unwrap()
            .is_none()
    );

    // Paid under it, the difference is a discount: the cash is what was paid.
    let cheap = new_entry("Venda com desconto", &["100"]).await.unwrap();
    let discounted = charge_for(&cheap, 1).await.unwrap();
    assert_eq!(pay(&discounted, "evt_3", "95", "1").await, Ok(1));
    let under = settlements_of(&cheap).await.unwrap();
    assert_eq!(under[0].applied().to_string(), "100");
    assert_eq!(under[0].discount().to_string(), "5");
    assert_eq!(under[0].cash().to_string(), "95");

    // Settled by hand while the charge was open: the payment is kept, flagged, and not settled twice.
    let by_hand = new_entry("Venda quitada na mão", &["100"]).await.unwrap();
    let late = charge_for(&by_hand, 1).await.unwrap();
    entries
        .settle(
            company,
            &ana,
            by_hand.id(),
            settlement_draft(seed, 1, "100"),
        )
        .await
        .unwrap();
    assert_eq!(pay(&late, "evt_4", "100", "2").await, Ok(1));
    assert_eq!(settlements_of(&by_hand).await.unwrap().len(), 1);
    let flagged = status_of(&late).await;
    assert_eq!(flagged.status(), ChargeStatus::Paid);
    assert_eq!(
        flagged.receipt().unwrap().note.as_deref(),
        Some("the installment was already settled")
    );
    assert_eq!(
        outcome_of(events, company, &late, "evt_4").await,
        (
            PaymentEventOutcome::NotApplied,
            Some("the installment was already settled".to_owned())
        )
    );
    assert!(
        entries
            .made_by(company, EntryOrigin::ChargeFee(late.id()))
            .await
            .unwrap()
            .is_some(),
        "the provider kept its fee all the same"
    );

    // Part of the installment was settled by hand: only what is left is settled, and the cash is what was paid.
    let partly = new_entry("Venda parcial", &["100"]).await.unwrap();
    let rest = charge_for(&partly, 1).await.unwrap();
    entries
        .settle(company, &ana, partly.id(), settlement_draft(seed, 1, "40"))
        .await
        .unwrap();
    assert_eq!(pay(&rest, "evt_5", "100", "0").await, Ok(1));
    let parts = settlements_of(&partly).await.unwrap();
    assert_eq!(parts.len(), 2);
    let settled_by_charge = parts.iter().find(|s| s.charge_id().is_some()).unwrap();
    assert_eq!(settled_by_charge.applied().to_string(), "60");
    assert_eq!(settled_by_charge.cash().to_string(), "100");

    // A cancelled entry takes no settlement: the payment is kept and flagged.
    let gone = new_entry("Venda cancelada", &["100"]).await.unwrap();
    let orphan = charge_for(&gone, 1).await.unwrap();
    entries.void(company, &ana, gone.id()).await.unwrap();
    assert_eq!(pay(&orphan, "evt_6", "100", "0").await, Ok(1));
    assert_eq!(
        status_of(&orphan).await.receipt().unwrap().note.as_deref(),
        Some("the entry was cancelled")
    );

    // Expired and cancelled at the provider; a payment that still arrives on a charge called off is a payment.
    let slow = new_entry("Venda vencida", &["100"]).await.unwrap();
    let expiring = charge_for(&slow, 1).await.unwrap();
    let (headers, body) =
        FakePaymentProvider::notice("evt_7", expiring.external_id(), "expired", None);
    assert_eq!(
        webhooks.receive(company, "fake", &headers, &body).await,
        Ok(1)
    );
    assert_eq!(status_of(&expiring).await.status(), ChargeStatus::Expired);
    let called_off = charge_for(&slow, 1).await.unwrap();
    let (headers, body) =
        FakePaymentProvider::notice("evt_8", called_off.external_id(), "cancelled", None);
    assert_eq!(
        webhooks.receive(company, "fake", &headers, &body).await,
        Ok(1)
    );
    assert_eq!(
        status_of(&called_off).await.status(),
        ChargeStatus::Cancelled
    );
    assert_eq!(pay(&called_off, "evt_9", "100", "0").await, Ok(1));
    assert_eq!(status_of(&called_off).await.status(), ChargeStatus::Paid);
    assert_eq!(settlements_of(&slow).await.unwrap().len(), 1);

    // A notice interrupted after the settlement is made is finished, not repeated: the charge settled
    // already, and the fee, the receipt and the outcome that were missing are made.
    let crashed = new_entry("Venda interrompida", &["100"]).await.unwrap();
    let half = charge_for(&crashed, 1).await.unwrap();
    entries
        .settle_charge(
            company,
            &ana,
            crashed.id(),
            SettlementDraft {
                settled_on: "2026-12-09".into(),
                ..settlement_draft(seed, 1, "100")
            },
            half.id(),
        )
        .await
        .unwrap();
    let (_, body) = FakePaymentProvider::notice(
        "evt_crash",
        half.external_id(),
        "paid",
        Some(("100", "2026-12-09", "3")),
    );
    let pending = PaymentEvent::new(
        PaymentEventId::generate(),
        company,
        half.id(),
        "fake",
        "evt_crash",
        PaymentEventKind::Paid,
        body,
    )
    .unwrap();
    assert!(matches!(
        events.record(&pending).await.unwrap(),
        Recorded::New(_)
    ));
    assert_eq!(pay(&half, "evt_crash", "100", "3").await, Ok(1));
    assert_eq!(settlements_of(&crashed).await.unwrap().len(), 1);
    assert_eq!(status_of(&half).await.status(), ChargeStatus::Paid);
    assert!(
        entries
            .made_by(company, EntryOrigin::ChargeFee(half.id()))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        outcome_of(events, company, &half, "evt_crash").await,
        (PaymentEventOutcome::Applied, None)
    );

    // A notice of a payment of nothing is kept and flagged, and the charge stays open.
    let nothing = new_entry("Venda sem valor", &["100"]).await.unwrap();
    let zero = charge_for(&nothing, 1).await.unwrap();
    assert_eq!(pay(&zero, "evt_zero", "0", "0").await, Ok(1));
    assert_eq!(status_of(&zero).await.status(), ChargeStatus::Open);
    assert!(settlements_of(&nothing).await.unwrap().is_empty());
    assert_eq!(
        outcome_of(events, company, &zero, "evt_zero").await,
        (
            PaymentEventOutcome::NotApplied,
            Some("the notice says nothing was paid".to_owned())
        )
    );

    // A charge settles once, even after its settlement is reversed: the payment is not applied again.
    let to_reverse = settlements_of(&sale)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.charge_id() == Some(first.id()))
        .unwrap();
    entries
        .reverse(company, &ana, sale.id(), to_reverse.id())
        .await
        .unwrap();
    assert_eq!(pay(&first, "evt_again", "100", "1.99").await, Ok(1));
    assert!(
        settlements_of(&sale)
            .await
            .unwrap()
            .iter()
            .all(|s| s.charge_id() != Some(first.id()) || s.is_reversed())
    );
}

/// What came of the notice the provider sent under this id: its outcome and note.
async fn outcome_of<Ev: PaymentEventRepository>(
    events: &Ev,
    company: CompanyId,
    charge: &conta_verde_domain::Charge,
    event: &str,
) -> (PaymentEventOutcome, Option<String>) {
    let probe = PaymentEvent::new(
        PaymentEventId::generate(),
        company,
        charge.id(),
        "fake",
        event,
        PaymentEventKind::Paid,
        Vec::new(),
    )
    .unwrap();
    match events.record(&probe).await.unwrap() {
        Recorded::Existing(stored) => (stored.outcome(), stored.note().map(str::to_owned)),
        Recorded::New(_) => panic!("the event {event} was not stored"),
    }
}

/// The payment page of a charge ([ADR 0047]): a link made for an open charge, shown once and kept as a
/// digest, leading to what the customer pays with while there is something to pay, and to nothing of another
/// company's.
#[allow(clippy::too_many_arguments)]
pub async fn serves_payment_pages<L, C, E, F, P, S, Sec>(
    pages: &PaymentPageService<L, C, E, Sec>,
    links: &L,
    charges: &ChargeService<C, E, P, FakePaymentProvider, S>,
    entries: &FinancialEntryService<E, F>,
    settings: &CollectionService<S, F>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    L: PaymentLinkRepository,
    C: ChargeRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    P: PartyRepository,
    S: CollectionSettingsRepository,
    Sec: SecretService,
{
    let ana = actor();
    settings
        .replace(company, &ana, seed.bank_account, None)
        .await
        .unwrap();
    let new_entry = |description: &str| {
        let mut draft = entry_draft(
            seed,
            description,
            "2026-11-10",
            &[("2026-12-10", "100"), ("2027-01-10", "50.50")],
        );
        draft.party_id = Some(seed.party);
        entries.register(company, &ana, draft)
    };
    let sale = new_entry("Venda 12").await.unwrap();
    let charge = charges
        .issue(company, &ana, sale.id(), 1, ChargeMethod::Pix, None)
        .await
        .unwrap();

    // The address holds the token; the page shows what the customer pays with, and what for.
    let link = pages
        .new_link(company, sale.id(), charge.id())
        .await
        .unwrap();
    let token = link
        .strip_prefix("https://app.example/pay/")
        .expect("under the public address");
    assert!(token.len() >= 32);
    let page = pages.page(token).await.unwrap();
    assert_eq!(page.state, PageState::Payable);
    assert_eq!(page.charge.id(), charge.id());
    assert_eq!(page.description, "Venda 12");
    assert_eq!(page.installments, 2);
    assert_eq!(
        page.charge.payment().pix_payload,
        charge.payment().pix_payload
    );

    // Only the digest is kept: the token itself finds nothing in the store, and a made-up one nothing at all.
    assert_eq!(links.find(token).await.unwrap(), None);
    for wrong in ["", "nope", &format!("{token}x"), &token[1..]] {
        assert_eq!(
            pages.page(wrong).await.unwrap_err(),
            ApplicationError::ChargeNotFound,
            "{wrong}"
        );
    }

    // A second link leads to the same page, and the first still works.
    let again = pages
        .new_link(company, sale.id(), charge.id())
        .await
        .unwrap();
    assert_ne!(again, link);
    let second = again.strip_prefix("https://app.example/pay/").unwrap();
    assert_eq!(pages.page(second).await.unwrap().charge.id(), charge.id());
    assert_eq!(pages.page(token).await.unwrap().state, PageState::Payable);

    // Another company's charge gets no link, and another entry's id does not find it.
    assert_eq!(
        pages
            .new_link(other_company, sale.id(), charge.id())
            .await
            .unwrap_err(),
        ApplicationError::ChargeNotFound
    );
    let other_entry = new_entry("Outra venda").await.unwrap();
    assert_eq!(
        pages
            .new_link(company, other_entry.id(), charge.id())
            .await
            .unwrap_err(),
        ApplicationError::ChargeNotFound
    );

    // Settled by hand while the charge was open: nothing left to pay on the page.
    entries
        .settle(company, &ana, sale.id(), settlement_draft(seed, 1, "100"))
        .await
        .unwrap();
    assert_eq!(
        pages.page(token).await.unwrap().state,
        PageState::Unavailable
    );

    // A part settled another way: the old amount would overpay, so the page offers nothing; reversing the part
    // makes it payable again.
    let partly = new_entry("Venda parcial").await.unwrap();
    let partial = charges
        .issue(company, &ana, partly.id(), 1, ChargeMethod::Pix, None)
        .await
        .unwrap();
    let partial_link = pages
        .new_link(company, partly.id(), partial.id())
        .await
        .unwrap();
    let partial_token = partial_link
        .strip_prefix("https://app.example/pay/")
        .unwrap();
    let part = entries
        .settle(company, &ana, partly.id(), settlement_draft(seed, 1, "30"))
        .await
        .unwrap();
    assert_eq!(
        pages.page(partial_token).await.unwrap().state,
        PageState::Unavailable
    );
    entries
        .reverse(company, &ana, partly.id(), part.id())
        .await
        .unwrap();
    assert_eq!(
        pages.page(partial_token).await.unwrap().state,
        PageState::Payable
    );
    // Settled by the charge's own payment, before the webhook marked it paid: the customer is told it was paid.
    entries
        .settle_charge(
            company,
            &ana,
            partly.id(),
            settlement_draft(seed, 1, "100"),
            partial.id(),
        )
        .await
        .unwrap();
    assert_eq!(
        pages.page(partial_token).await.unwrap().state,
        PageState::Paid
    );

    // A charge that is called off has nothing to pay, and no new link.
    let other = charges
        .issue(company, &ana, sale.id(), 2, ChargeMethod::Boleto, None)
        .await
        .unwrap();
    let other_link = pages
        .new_link(company, sale.id(), other.id())
        .await
        .unwrap();
    let other_token = other_link.strip_prefix("https://app.example/pay/").unwrap();
    assert_eq!(
        pages.page(other_token).await.unwrap().state,
        PageState::Payable
    );
    charges
        .cancel(company, &ana, sale.id(), other.id())
        .await
        .unwrap();
    assert_eq!(
        pages.page(other_token).await.unwrap().state,
        PageState::Unavailable
    );
    assert_eq!(
        pages
            .new_link(company, sale.id(), other.id())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::ChargeIsNotOpen)
    );

    // A cancelled entry's open charge cannot be paid either.
    let gone = new_entry("Venda cancelada").await.unwrap();
    let doomed = charges
        .issue(company, &ana, gone.id(), 1, ChargeMethod::CardLink, None)
        .await
        .unwrap();
    let doomed_link = pages
        .new_link(company, gone.id(), doomed.id())
        .await
        .unwrap();
    let doomed_token = doomed_link
        .strip_prefix("https://app.example/pay/")
        .unwrap();
    entries.void(company, &ana, gone.id()).await.unwrap();
    assert_eq!(
        pages.page(doomed_token).await.unwrap().state,
        PageState::Unavailable
    );
}

/// Registers a customer who can be reminded by e-mail and by WhatsApp (a contact with a phone).
///
/// The address and the phone are of this one only: the pass reaches **every** company's reminders, and
/// tests that share a database look at what went to their own customer.
async fn reachable_customer<P: PartyRepository>(
    parties: &P,
    company: CompanyId,
) -> (PartyId, String, String) {
    let tag = Uuid::new_v4().as_u128();
    let address = format!("cliente-{tag:x}@example.com");
    let phone = format!("5581{:09}", tag % 1_000_000_000);
    let mut draft = party_draft("Cliente Lembrado", None);
    draft.email = Some(address.clone());
    draft.contacts = vec![ContactDraft {
        name: "Ana".into(),
        phone: Some(format!("+{phone}")),
        ..ContactDraft::default()
    }];
    let party = Party::new(PartyId::generate(), company, draft).unwrap();
    parties.save(&party, &created_entry(&party)).await.unwrap();
    (party.id(), address, phone)
}

/// The reminders ([ADR 0048]) on a test clock: the plan is audited and replaced as a whole; a rule reaches
/// the installments of receivables that still owe, once each however often the pass runs; what has nowhere
/// to go, is paid, is cancelled, is a payable or is not due yet is left alone; the e-mail carries a link to
/// the open charge, and WhatsApp the phone of a contact.
#[allow(clippy::too_many_arguments)]
pub async fn sends_reminders_on_schedule<D, E, P, K, N, I, M, S, F, C>(
    service: &DunningService<D, E, P, K, N, I, M, InMemoryMessageSender>,
    entries: &FinancialEntryService<E, F>,
    parties: &P,
    charges: &ChargeService<C, E, P, FakePaymentProvider, S>,
    settings: &CollectionService<S, F>,
    emails: &InMemoryEmailSender,
    messages: &InMemoryMessageSender,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    D: DunningPlanRepository,
    E: FinancialEntryRepository,
    P: PartyRepository,
    K: InstallmentLinks,
    N: NotificationRepository,
    I: IdentityRepository,
    M: EmailSender,
    S: CollectionSettingsRepository,
    F: EntryReferences,
    C: ChargeRepository,
{
    use conta_verde_domain::{DunningChannel, DunningRule, EntryKind, NotificationStatus};
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    let (customer, address, phone) = reachable_customer(parties, company).await;
    let mine = || -> Vec<EmailMessage> {
        emails
            .sent()
            .into_iter()
            .filter(|message| message.to.as_str() == address)
            .collect()
    };
    let texts = || -> Vec<TextMessage> {
        messages
            .sent()
            .into_iter()
            .filter(|message| message.to == phone)
            .collect()
    };
    let rule = |days, channel| DunningRule { days, channel };
    let entry = |kind: EntryKind, party: Option<PartyId>, due: &str, amount: &str| {
        let mut draft = entry_draft(seed, "Venda 12", "2026-11-10", &[(due, amount)]);
        draft.kind = kind;
        draft.party_id = party;
        entries.register(company, &ana, draft)
    };
    settings
        .replace(company, &ana, seed.bank_account, None)
        .await
        .unwrap();

    // Nothing is sent before there is a plan, or while it is off.
    let due = entry(EntryKind::Receivable, Some(customer), "2026-12-10", "100")
        .await
        .unwrap();
    service.send_due(day("2026-12-07")).await.unwrap();
    let plan = service
        .replace_plan(company, &ana, false, vec![rule(0, DunningChannel::Email)])
        .await
        .unwrap();
    assert!(!plan.is_enabled());
    service.send_due(day("2026-12-10")).await.unwrap();
    assert!(mine().is_empty());
    assert!(texts().is_empty());

    // The plan is replaced as a whole and audited; the same plan again writes nothing; the rules come back
    // in the order they fall.
    let plan = service
        .replace_plan(
            company,
            &ana,
            true,
            vec![
                rule(3, DunningChannel::Email),
                rule(0, DunningChannel::Whatsapp),
                rule(-3, DunningChannel::Email),
                rule(0, DunningChannel::Email),
            ],
        )
        .await
        .unwrap();
    assert_eq!(plan.rules_text(), "-3 email\n0 email\n0 whatsapp\n3 email");
    assert_eq!(service.plan(company).await.unwrap(), plan);
    service
        .replace_plan(company, &ana, true, plan.rules().to_vec())
        .await
        .unwrap();
    let trail = service.plan_history(company).await.unwrap();
    assert_eq!(
        trail
            .iter()
            .map(|record| record.entry.action)
            .collect::<Vec<_>>(),
        [AuditAction::Created, AuditAction::Updated]
    );
    assert_eq!(
        service
            .replace_plan(
                company,
                &ana,
                true,
                vec![
                    rule(0, DunningChannel::Email),
                    rule(0, DunningChannel::Email)
                ]
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::DuplicateDunningRule)
    );
    assert!(!service.plan(other_company).await.unwrap().is_enabled());

    // What is left alone: a customer with nowhere to be reached, a payable, a cancelled entry, one that is
    // paid, one not due yet.
    let unreachable = entry(EntryKind::Receivable, Some(seed.party), "2026-12-10", "10")
        .await
        .unwrap();
    entry(EntryKind::Payable, Some(customer), "2026-12-10", "10")
        .await
        .unwrap();
    let cancelled = entry(EntryKind::Receivable, Some(customer), "2026-12-10", "10")
        .await
        .unwrap();
    entries.void(company, &ana, cancelled.id()).await.unwrap();
    let paid = entry(EntryKind::Receivable, Some(customer), "2026-12-10", "10")
        .await
        .unwrap();
    entries
        .settle(company, &ana, paid.id(), settlement_draft(seed, 1, "10"))
        .await
        .unwrap();
    entry(EntryKind::Receivable, Some(customer), "2026-12-20", "10")
        .await
        .unwrap();

    // The open charge of the due installment: its payment page link goes in the e-mail.
    charges
        .issue(company, &ana, due.id(), 1, ChargeMethod::Pix, None)
        .await
        .unwrap();

    // Three days before the due day: one e-mail, with the link; the customer with no address is counted.
    service.send_due(day("2026-12-07")).await.unwrap();
    let sent = mine();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to.as_str(), address);
    assert!(
        sent[0].subject.contains("parcela 1 (vence em 3 dias)"),
        "{}",
        sent[0].subject
    );
    assert!(sent[0].body.contains("Valor: R$ 100,00"));
    assert!(sent[0].body.contains("Vencimento: 10/12/2026"));
    assert!(sent[0].body.contains("https://app.example/pay/"));
    let reminders = service.notifications(company, due.id()).await.unwrap();
    assert_eq!(reminders.len(), 1);
    assert_eq!(reminders[0].days(), -3);
    assert_eq!(reminders[0].channel(), DunningChannel::Email);
    assert_eq!(reminders[0].status(), NotificationStatus::Sent);
    assert_eq!(reminders[0].recipient(), address);
    assert!(
        service
            .notifications(company, unreachable.id())
            .await
            .unwrap()
            .is_empty()
    );

    // Running the pass again, or a day later (still inside what the rule reaches), sends nothing twice.
    for again in ["2026-12-07", "2026-12-08", "2026-12-09"] {
        service.send_due(day(again)).await.unwrap();
    }
    assert_eq!(mine().len(), 1);

    // On the due day: the e-mail and the WhatsApp message, to the contact's phone with the country code.
    service.send_due(day("2026-12-10")).await.unwrap();
    assert_eq!(mine().len(), 2);
    assert!(mine()[1].subject.contains("vence hoje"));
    let chats = texts();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].to, phone);
    assert!(chats[0].text.contains("vence hoje"));
    assert!(!chats[0].text.contains('\n'));
    assert_eq!(
        service
            .notifications(company, due.id())
            .await
            .unwrap()
            .len(),
        3
    );

    // Three days late, what is paid in the meantime is no longer reminded of and what is part paid says
    // what is left.
    entries
        .settle(company, &ana, due.id(), settlement_draft(seed, 1, "100"))
        .await
        .unwrap();
    let late = entry(EntryKind::Receivable, Some(customer), "2026-12-10", "50")
        .await
        .unwrap();
    entries
        .settle(company, &ana, late.id(), settlement_draft(seed, 1, "20"))
        .await
        .unwrap();
    service.send_due(day("2026-12-13")).await.unwrap();
    assert_eq!(mine().len(), 3);
    let last = mine().pop().unwrap();
    assert!(
        last.subject.contains("venceu há 3 dias"),
        "{}",
        last.subject
    );
    assert!(last.body.contains("Valor: R$ 30,00"));
    assert!(!last.body.contains("https://"), "no open charge, no link");
    assert_eq!(
        service
            .notifications(company, due.id())
            .await
            .unwrap()
            .len(),
        3
    );

    // Nothing of another company's.
    assert_eq!(
        service
            .notifications(other_company, late.id())
            .await
            .unwrap_err(),
        ApplicationError::EntryNotFound
    );
}

/// A reminder that failed is tried again on the next passes, three times in all, and then left; the
/// customer is never sent one twice.
pub async fn retries_a_failed_reminder_a_few_times<D, E, P, K, N, I, M, W, F>(
    service: &DunningService<D, E, P, K, N, I, M, W>,
    entries: &FinancialEntryService<E, F>,
    parties: &P,
    company: CompanyId,
    seed: &EntrySeed,
) where
    D: DunningPlanRepository,
    E: FinancialEntryRepository,
    P: PartyRepository,
    K: InstallmentLinks,
    N: NotificationRepository,
    I: IdentityRepository,
    M: EmailSender,
    W: MessageSender,
    F: EntryReferences,
{
    use conta_verde_domain::{DunningChannel, DunningRule, NotificationStatus};
    let ana = actor();
    let (customer, _, _) = reachable_customer(parties, company).await;
    let mut draft = entry_draft(seed, "Venda", "2031-11-10", &[("2031-12-10", "100")]);
    draft.party_id = Some(customer);
    let sale = entries.register(company, &ana, draft).await.unwrap();
    service
        .replace_plan(
            company,
            &ana,
            true,
            vec![DunningRule {
                days: 0,
                channel: DunningChannel::Email,
            }],
        )
        .await
        .unwrap();
    // A year no other test uses: the pass reaches every company's reminders.
    let day = CalendarDate::parse("2031-12-10").unwrap();
    for (pass, expected_attempts) in [(1, 1), (2, 2), (3, 3), (4, 3), (5, 3)] {
        service.send_due(day).await.unwrap();
        let reminders = service.notifications(company, sale.id()).await.unwrap();
        assert_eq!(reminders.len(), 1, "pass {pass}");
        assert_eq!(reminders[0].attempts(), expected_attempts, "pass {pass}");
    }
    let reminders = service.notifications(company, sale.id()).await.unwrap();
    assert_eq!(reminders.len(), 1);
    assert_eq!(reminders[0].status(), NotificationStatus::Failed);
    assert_eq!(reminders[0].attempts(), 3);
    assert_eq!(reminders[0].note(), Some("the message was not sent"));
}

/// Who owes what late ([ADR 0051]): the installments of receivables that fell due before the day and still owe,
/// by customer and by how late; nothing that is paid, cancelled, a payable, due on the day or after it, or of
/// another company.
pub async fn reports_delinquency<R, F, P>(
    entries: &FinancialEntryService<R, F>,
    parties: &P,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    R: FinancialEntryRepository,
    F: EntryReferences,
    P: PartyRepository,
{
    use conta_verde_domain::EntryKind;
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    let (a, _, _) = reachable_customer(parties, company).await;
    let (b, _, _) = reachable_customer(parties, company).await;
    let entry = |kind: EntryKind, party: Option<PartyId>, due: &str, amount: &str| {
        let mut draft = entry_draft(seed, "Venda", "2026-09-01", &[(due, amount)]);
        draft.kind = kind;
        draft.party_id = party;
        entries.register(company, &ana, draft)
    };
    let on = day("2027-01-31");

    // Nothing is late before anything is registered, and a day with nothing before it is an empty report.
    assert_eq!(
        entries
            .delinquency(company, on)
            .await
            .unwrap()
            .owed
            .to_string(),
        "0"
    );
    assert_eq!(
        entries
            .delinquency(company, CalendarDate::EARLIEST)
            .await
            .unwrap()
            .installments,
        0
    );

    entry(EntryKind::Receivable, Some(a), "2027-01-20", "100")
        .await
        .unwrap();
    let part_paid = entry(EntryKind::Receivable, Some(a), "2026-12-20", "50")
        .await
        .unwrap();
    entries
        .settle(
            company,
            &ana,
            part_paid.id(),
            settlement_draft(seed, 1, "20"),
        )
        .await
        .unwrap();
    entry(EntryKind::Receivable, Some(a), "2026-10-01", "10")
        .await
        .unwrap();
    entry(EntryKind::Receivable, Some(b), "2027-01-30", "5")
        .await
        .unwrap();
    entry(EntryKind::Receivable, None, "2027-01-01", "7")
        .await
        .unwrap();
    // Left out: due on the day and after it, paid, a payable, cancelled, and another company's.
    entry(EntryKind::Receivable, Some(b), "2027-01-31", "99")
        .await
        .unwrap();
    entry(EntryKind::Receivable, Some(b), "2027-02-10", "99")
        .await
        .unwrap();
    let paid = entry(EntryKind::Receivable, Some(b), "2027-01-01", "40")
        .await
        .unwrap();
    entries
        .settle(company, &ana, paid.id(), settlement_draft(seed, 1, "40"))
        .await
        .unwrap();
    entry(EntryKind::Payable, Some(b), "2027-01-01", "1000")
        .await
        .unwrap();
    let cancelled = entry(EntryKind::Receivable, Some(b), "2027-01-01", "500")
        .await
        .unwrap();
    entries.void(company, &ana, cancelled.id()).await.unwrap();
    assert_eq!(
        entries
            .delinquency(other_company, on)
            .await
            .unwrap()
            .installments,
        0
    );

    let report = entries.delinquency(company, on).await.unwrap();
    assert_eq!(report.on, on);
    assert_eq!(report.owed.to_string(), "152");
    assert_eq!(report.installments, 5);
    assert_eq!(
        report.by_bucket.map(|amount| amount.to_string()),
        ["112", "30", "0", "10"]
    );
    assert_eq!(report.installments_by_bucket, [3, 1, 0, 1]);
    let owed: Vec<String> = report
        .customers
        .iter()
        .map(|c| c.owed.to_string())
        .collect();
    assert_eq!(owed, ["140", "7", "5"]);
    assert_eq!(report.customers[0].party_id, Some(a));
    assert_eq!(report.customers[0].oldest_due, day("2026-10-01"));
    assert_eq!(report.customers[1].party_id, None);
    assert_eq!(report.customers[2].party_id, Some(b));

    // Ten days earlier the installment due on the 30th is not late yet, and what is, is less late.
    let earlier = entries
        .delinquency(company, day("2027-01-21"))
        .await
        .unwrap();
    assert_eq!(earlier.installments, 4);
    assert_eq!(earlier.owed.to_string(), "147");
}

fn purchase_draft(entry_seed: &EntrySeed, seed: &SalesSeed) -> PurchaseOrderDraft {
    PurchaseOrderDraft {
        supplier_id: seed.supplier,
        ordered_on: "2026-11-06".into(),
        expected_on: Some("2026-11-20".into()),
        payment_term_id: Some(seed.term),
        category_id: entry_seed.category,
        cost_center_id: Some(entry_seed.cost_center),
        notes: Some("Entregar no depósito".into()),
        lines: vec![
            sale_line(seed.product, "Café 500g", "10", "8.10", "0.50"),
            sale_line(seed.other_product, "Pão francês", "3", "3.33", "0"),
        ],
    }
}

fn receipt_draft(day: &str, items: &[(u16, &str)]) -> PurchaseReceiptDraft {
    PurchaseReceiptDraft {
        received_on: day.into(),
        notes: None,
        items: items
            .iter()
            .map(|(line, quantity)| ReceiptItemDraft {
                line: *line,
                quantity: (*quantity).into(),
            })
            .collect(),
    }
}

fn purchase_numbers(orders: &PurchaseOrderPage) -> Vec<i64> {
    orders.items.iter().map(|order| order.number()).collect()
}

/// A purchase order from a draft to everything received: numbering, what it points at, listing, the life of
/// the order, and the payable each receipt makes, valued cumulatively so partial receipts add up.
#[allow(clippy::too_many_lines)]
pub async fn runs_the_purchase_lifecycle<P, E, F, R, T>(
    purchases: &PurchaseService<P, E, F, R, T>,
    store: &P,
    entries: &FinancialEntryService<E, F>,
    company: CompanyId,
    other_company: CompanyId,
    entry_seed: &EntrySeed,
    seed: &SalesSeed,
) where
    P: PurchaseOrderRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
{
    let ana = actor();

    // Written as a draft, numbered per company, worth the sum of its lines.
    let first = purchases
        .register(company, &ana, purchase_draft(entry_seed, seed))
        .await
        .unwrap();
    assert_eq!((first.number(), first.status()), (1, PurchaseStatus::Draft));
    assert_eq!(first.total().to_string(), "90.49");
    assert_eq!(purchases.get(company, first.id()).await.unwrap(), first);
    let trail = purchases.history(company, first.id()).await.unwrap();
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].entry.action, AuditAction::Created);
    let second = purchases
        .register(company, &ana, purchase_draft(entry_seed, seed))
        .await
        .unwrap();
    assert_eq!(second.number(), 2);

    // What it points at has to be usable, and what is refused leaves nothing.
    let refused = |what| ApplicationError::UnusableReference(what);
    for (what, change) in [
        (
            "supplier",
            Box::new(|d: &mut PurchaseOrderDraft| d.supplier_id = seed.customer)
                as Box<dyn Fn(&mut PurchaseOrderDraft)>,
        ),
        (
            "payment term",
            Box::new(|d: &mut PurchaseOrderDraft| d.payment_term_id = Some(seed.inactive_term)),
        ),
        (
            "product",
            Box::new(|d: &mut PurchaseOrderDraft| d.lines[0].product_id = seed.inactive_product),
        ),
        (
            "category",
            Box::new(|d: &mut PurchaseOrderDraft| d.category_id = entry_seed.inactive_category),
        ),
        (
            "category",
            Box::new(|d: &mut PurchaseOrderDraft| d.category_id = entry_seed.transfer_category),
        ),
        (
            "cost center",
            Box::new(|d: &mut PurchaseOrderDraft| {
                d.cost_center_id = Some(entry_seed.inactive_cost_center);
            }),
        ),
    ] {
        let mut bad = purchase_draft(entry_seed, seed);
        change(&mut bad);
        assert_eq!(
            purchases.register(company, &ana, bad).await.unwrap_err(),
            refused(what),
            "{what}"
        );
    }
    let mut bad = purchase_draft(entry_seed, seed);
    bad.lines.clear();
    assert_eq!(
        purchases.register(company, &ana, bad).await.unwrap_err(),
        ApplicationError::Domain(DomainError::NoLines)
    );
    let mut bad = purchase_draft(entry_seed, seed);
    bad.expected_on = Some("2026-11-05".into());
    assert_eq!(
        purchases.register(company, &ana, bad).await.unwrap_err(),
        ApplicationError::Domain(DomainError::ExpectedBeforeOrdered)
    );
    // What was refused left no order behind and used no number.
    assert_eq!(
        purchase_numbers(
            &purchases
                .list(company, &PurchaseOrderQuery::default(), None, None)
                .await
                .unwrap()
        ),
        [2, 1]
    );
    let third = purchases
        .register(company, &ana, purchase_draft(entry_seed, seed))
        .await
        .unwrap();
    assert_eq!(third.number(), 3);

    // Nothing of another company's shows, and none of it can be used from there.
    assert_eq!(
        purchases.get(other_company, first.id()).await.unwrap_err(),
        ApplicationError::PurchaseNotFound
    );
    assert_eq!(
        purchases
            .approve(other_company, &ana, first.id())
            .await
            .unwrap_err(),
        ApplicationError::PurchaseNotFound
    );
    assert!(
        purchases
            .list(other_company, &PurchaseOrderQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );

    // Listed newest first, by status, supplier and text, and paged.
    let listed = purchases
        .list(company, &PurchaseOrderQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(purchase_numbers(&listed), [3, 2, 1]);
    let page = purchases
        .list(company, &PurchaseOrderQuery::default(), None, Some(2))
        .await
        .unwrap();
    assert_eq!(purchase_numbers(&page), [3, 2]);
    let rest = purchases
        .list(
            company,
            &PurchaseOrderQuery::default(),
            page.next.as_ref(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(purchase_numbers(&rest), [1]);
    for (query, expected) in [
        (
            PurchaseOrderQuery::default().with_status(PurchaseStatus::Draft),
            3,
        ),
        (
            PurchaseOrderQuery::default().with_status(PurchaseStatus::Received),
            0,
        ),
        (
            PurchaseOrderQuery::default().with_supplier(seed.supplier),
            3,
        ),
        (
            PurchaseOrderQuery::default().with_supplier(seed.customer),
            0,
        ),
        (PurchaseOrderQuery::default().searching("pão fran"), 3),
        (PurchaseOrderQuery::default().searching("2"), 1),
        (PurchaseOrderQuery::default().searching("%"), 0),
        // The orders of the seed were all made on 2026-11-06, and the days are included.
        (
            PurchaseOrderQuery::default()
                .ordered_from(CalendarDate::parse("2026-11-06").unwrap())
                .ordered_until(CalendarDate::parse("2026-11-06").unwrap()),
            3,
        ),
        (
            PurchaseOrderQuery::default().ordered_from(CalendarDate::parse("2026-11-07").unwrap()),
            0,
        ),
        (
            PurchaseOrderQuery::default().ordered_until(CalendarDate::parse("2026-11-05").unwrap()),
            0,
        ),
    ] {
        let found = purchases.list(company, &query, None, None).await.unwrap();
        assert_eq!(found.items.len(), expected, "{query:?}");
    }

    // The counts per status ignore the query's own status, follow its search and supplier, and stay in the company.
    let counts = |orders: Vec<(PurchaseStatus, u64)>| {
        let mut orders = orders;
        orders.sort_by_key(|(status, _)| status.as_str());
        orders
    };
    for (query, expected) in [
        (
            PurchaseOrderQuery::default().with_status(PurchaseStatus::Received),
            vec![(PurchaseStatus::Draft, 3)],
        ),
        (
            PurchaseOrderQuery::default().searching("2"),
            vec![(PurchaseStatus::Draft, 1)],
        ),
        (
            PurchaseOrderQuery::default().with_supplier(seed.customer),
            vec![],
        ),
        (
            PurchaseOrderQuery::default().ordered_from(CalendarDate::parse("2026-11-07").unwrap()),
            vec![],
        ),
        (
            PurchaseOrderQuery::default().ordered_until(CalendarDate::parse("2026-11-06").unwrap()),
            vec![(PurchaseStatus::Draft, 3)],
        ),
    ] {
        assert_eq!(
            counts(purchases.status_counts(company, &query).await.unwrap()),
            expected,
            "{query:?}"
        );
    }
    assert!(
        purchases
            .status_counts(other_company, &PurchaseOrderQuery::default())
            .await
            .unwrap()
            .is_empty()
    );

    // A draft changes; one that changes nothing records nothing.
    let mut edit = purchase_draft(entry_seed, seed);
    edit.lines[0].quantity = "12".into();
    let changed = purchases
        .update(company, &ana, first.id(), edit.clone())
        .await
        .unwrap();
    // 12 x 8.10 - 0.50 = 96.70, and 3 x 3.33 = 9.99.
    assert_eq!(changed.total().to_string(), "106.69");
    purchases
        .update(company, &ana, first.id(), edit.clone())
        .await
        .unwrap();
    assert_eq!(
        purchases.history(company, first.id()).await.unwrap().len(),
        2
    );

    // Nothing arrives at a draft; a draft is cancelled once.
    assert_eq!(
        purchases
            .receive(
                company,
                &ana,
                first.id(),
                receipt_draft("2026-11-10", &[(1, "1")])
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::PurchaseIsNotReceivable)
    );
    let cancelled = purchases.cancel(company, &ana, third.id()).await.unwrap();
    assert_eq!(cancelled.status(), PurchaseStatus::Cancelled);
    assert_eq!(
        purchases
            .cancel(company, &ana, third.id())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::PurchaseIsLocked)
    );

    // Approved: its items are fixed.
    let approved = purchases.approve(company, &ana, first.id()).await.unwrap();
    assert_eq!(approved.status(), PurchaseStatus::Approved);
    assert_eq!(
        counts(
            purchases
                .status_counts(company, &PurchaseOrderQuery::default())
                .await
                .unwrap()
        ),
        vec![
            (PurchaseStatus::Approved, 1),
            (PurchaseStatus::Cancelled, 1),
            (PurchaseStatus::Draft, 1),
        ]
    );
    for error in [
        purchases
            .approve(company, &ana, first.id())
            .await
            .unwrap_err(),
        purchases
            .update(company, &ana, first.id(), edit)
            .await
            .unwrap_err(),
    ] {
        assert_eq!(
            error,
            ApplicationError::Domain(DomainError::PurchaseIsLocked)
        );
    }

    // The first receipt: 5 of 12 coffees and every bread. The value of a line is taken cumulatively
    // (96.70 x 5/12 = 40.29) and the payable is split by the term (30/60, half and half) from the day it arrived.
    let one = purchases
        .receive(
            company,
            &ana,
            first.id(),
            receipt_draft("2026-11-10", &[(1, "5"), (2, "3")]),
        )
        .await
        .unwrap();
    let values: Vec<String> = one
        .receipt
        .items()
        .iter()
        .map(|item| item.value.to_string())
        .collect();
    assert_eq!(values, ["40.29", "9.99"]);
    assert_eq!(one.receipt.total().to_string(), "50.28");
    let payable = one.payable.clone().unwrap();
    assert_eq!(payable.kind(), EntryKind::Payable);
    assert_eq!(payable.description(), "Compra nº 1");
    assert_eq!(payable.party_id(), Some(seed.supplier));
    assert_eq!(payable.category_id(), entry_seed.category);
    assert_eq!(payable.cost_center_id(), Some(entry_seed.cost_center));
    assert_eq!(
        payable.origin(),
        Some(EntryOrigin::PurchaseReceipt(one.receipt.id()))
    );
    assert_eq!(payable.competence_date().to_string(), "2026-11-10");
    assert_eq!(
        payable.installments_text(),
        "1:2026-12-10:25.14,2:2027-01-09:25.14"
    );
    assert_eq!(
        purchases.get(company, first.id()).await.unwrap().status(),
        PurchaseStatus::PartiallyReceived
    );

    // More than was left is refused, and so are lines the order does not have; nothing is left behind.
    for (items, error) in [
        (&[(1, "7.001")][..], DomainError::ReceiptExceedsOrdered),
        (&[(3, "1")][..], DomainError::ReceiptLineUnknown),
        (&[(2, "0.001")][..], DomainError::ReceiptExceedsOrdered),
    ] {
        assert_eq!(
            purchases
                .receive(
                    company,
                    &ana,
                    first.id(),
                    receipt_draft("2026-11-12", items)
                )
                .await
                .unwrap_err(),
            ApplicationError::Domain(error),
            "{items:?}"
        );
    }
    assert_eq!(
        purchases.receipts(company, first.id()).await.unwrap().len(),
        1
    );
    // A partly received order is not cancelled, but it can be closed once what is left is given up.
    assert_eq!(
        purchases
            .cancel(company, &ana, first.id())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::PurchaseIsLocked)
    );

    // The rest arrives: the line adds up to its total exactly (96.70 - 40.29 = 56.41), and the order is received.
    let two = purchases
        .receive(
            company,
            &ana,
            first.id(),
            receipt_draft("2026-11-15", &[(1, "7")]),
        )
        .await
        .unwrap();
    assert_eq!(two.receipt.total().to_string(), "56.41");
    assert_eq!(
        two.payable.as_ref().unwrap().installments_text(),
        "1:2026-12-15:28.2,2:2027-01-14:28.21"
    );
    assert_eq!(
        purchases.get(company, first.id()).await.unwrap().status(),
        PurchaseStatus::Received
    );
    assert_eq!(
        purchases
            .receive(
                company,
                &ana,
                first.id(),
                receipt_draft("2026-11-16", &[(1, "1")])
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::PurchaseIsNotReceivable)
    );
    assert_eq!(
        purchases
            .close(company, &ana, first.id())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::PurchaseIsLocked)
    );

    // Each receipt has its payable, once however often it is finished.
    let records = purchases.receipts(company, first.id()).await.unwrap();
    assert_eq!(
        records
            .iter()
            .map(|record| record.receipt.id())
            .collect::<Vec<_>>(),
        [one.receipt.id(), two.receipt.id()]
    );
    assert!(records.iter().all(|record| record.payable.is_some()));
    let finished = purchases
        .finish_receipt(company, &ana, first.id(), one.receipt.id())
        .await
        .unwrap();
    assert_eq!(finished.payable.unwrap().id(), payable.id());
    assert_eq!(
        purchases
            .finish_receipt(company, &ana, first.id(), PurchaseReceiptId::generate())
            .await
            .unwrap_err(),
        ApplicationError::PurchaseReceiptNotFound
    );
    let payables = entries
        .list(company, &FinancialEntryQuery::default(), None, None)
        .await
        .unwrap();
    assert_eq!(payables.items.len(), 2);
    // What a receipt made is not edited as an entry of its own.
    let mut same = FinancialEntryDraft {
        kind: EntryKind::Payable,
        description: "Outra".into(),
        party_id: Some(seed.supplier),
        category_id: entry_seed.category,
        cost_center_id: None,
        competence_date: "2026-11-10".into(),
        installments: vec![EntryInstallmentDraft {
            due_date: "2026-12-10".into(),
            amount: "50.28".into(),
        }],
    };
    assert_eq!(
        entries
            .update(company, &ana, payable.id(), same.clone())
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::EntryFollowsASale)
    );
    same.description = "Compra nº 1".into();

    // Two receipts of what is left of the same line at once: one lands, the other is refused.
    let approved = purchases.approve(company, &ana, second.id()).await.unwrap();
    assert_eq!(approved.status(), PurchaseStatus::Approved);
    let (a, b) = tokio::join!(
        purchases.receive(
            company,
            &ana,
            second.id(),
            receipt_draft("2026-11-10", &[(1, "10")])
        ),
        purchases.receive(
            company,
            &ana,
            second.id(),
            receipt_draft("2026-11-10", &[(1, "10")])
        )
    );
    assert_eq!(a.is_ok() as u8 + b.is_ok() as u8, 1, "{a:?} {b:?}");
    assert_eq!(
        purchases
            .receipts(company, second.id())
            .await
            .unwrap()
            .len(),
        1
    );
    // A partly received order is closed: what is left is given up.
    assert_eq!(
        purchases.get(company, second.id()).await.unwrap().status(),
        PurchaseStatus::PartiallyReceived
    );
    let closed = purchases.close(company, &ana, second.id()).await.unwrap();
    assert_eq!(closed.status(), PurchaseStatus::Received);

    // A receipt of goods that cost nothing (all discounted) makes no payable; an order with no term is due the
    // day the goods arrive.
    let mut mixed = purchase_draft(entry_seed, seed);
    mixed.payment_term_id = None;
    mixed.lines = vec![
        sale_line(seed.product, "Café 500g", "1", "10", "0"),
        sale_line(seed.other_product, "Brinde", "1", "5", "5"),
    ];
    let mixed = purchases.register(company, &ana, mixed).await.unwrap();
    purchases.approve(company, &ana, mixed.id()).await.unwrap();
    let gift = purchases
        .receive(
            company,
            &ana,
            mixed.id(),
            receipt_draft("2026-11-10", &[(2, "1")]),
        )
        .await
        .unwrap();
    assert!(gift.payable.is_none());
    assert_eq!(gift.receipt.total().to_string(), "0");
    let paid = purchases
        .receive(
            company,
            &ana,
            mixed.id(),
            receipt_draft("2026-11-11", &[(1, "1")]),
        )
        .await
        .unwrap();
    assert_eq!(paid.payable.unwrap().installments_text(), "1:2026-11-11:10");
    assert_eq!(
        purchases.get(company, mixed.id()).await.unwrap().status(),
        PurchaseStatus::Received
    );

    // The trail of the first order: written, edited, approved, partly received, received.
    let trail = purchases.history(company, first.id()).await.unwrap();
    assert_eq!(trail.len(), 5);
    assert_eq!(
        store
            .find(company, first.id())
            .await
            .unwrap()
            .unwrap()
            .number(),
        1
    );
}

fn stock_draft(
    product: ProductId,
    kind: conta_verde_domain::StockMovementKind,
    quantity: &str,
    day: &str,
) -> conta_verde_domain::StockMovementDraft {
    conta_verde_domain::StockMovementDraft {
        product_id: product,
        kind,
        quantity: quantity.into(),
        value: None,
        unit_cost: None,
        occurred_on: day.into(),
        note: None,
    }
}

/// What the ledger of a company holds, by hand: the worked example of the average cost, the floor of a manual
/// exit, reversals, listing, and two writes racing for the same stock.
#[allow(clippy::too_many_lines)]
pub async fn runs_the_stock_ledger<M, P, St>(
    stock: &StockService<M, P, St>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &SalesSeed,
) where
    M: StockMovementRepository,
    P: ProductRepository,
    St: StockSettingsRepository,
{
    use conta_verde_domain::StockMovementKind::{Adjustment, In, Out};
    let ana = actor();
    let coffee = seed.product;
    let state = |balance: conta_verde_domain::StockBalance| {
        (
            balance.quantity().to_string(),
            balance.value().to_string(),
            balance.average_cost().map(|cost| cost.to_string()),
        )
    };
    let bought = |quantity: &str, unit_cost: &str, day: &str| {
        let mut draft = stock_draft(coffee, In, quantity, day);
        draft.unit_cost = Some(unit_cost.into());
        draft
    };

    // The worked example: 10 bought at 5 and 10 at 7 are 20 worth 120, average 6.
    let first = stock
        .record(company, &ana, bought("10", "5", "2026-11-01"))
        .await
        .unwrap();
    assert_eq!(first.value().to_string(), "50");
    stock
        .record(company, &ana, bought("10", "7", "2026-11-06"))
        .await
        .unwrap();
    assert_eq!(
        state(stock.balance_of(company, coffee).await.unwrap()),
        ("20".into(), "120".into(), Some("6".into()))
    );
    // Selling 5 takes them at the average, 30; the average stays 6.
    let five = stock
        .record(company, &ana, stock_draft(coffee, Out, "5", "2026-11-06"))
        .await
        .unwrap();
    assert_eq!(
        (five.quantity().to_string(), five.value().to_string()),
        ("-5".into(), "-30".into())
    );
    assert_eq!(
        state(stock.balance_of(company, coffee).await.unwrap()),
        ("15".into(), "90".into(), Some("6".into()))
    );

    // A manual exit never takes more than there is, and what is refused leaves nothing.
    let before = stock
        .list(company, &StockMovementQuery::default(), None, None)
        .await
        .unwrap()
        .items
        .len();
    assert_eq!(
        stock
            .record(
                company,
                &ana,
                stock_draft(coffee, Out, "15.001", "2026-11-06")
            )
            .await
            .unwrap_err(),
        ApplicationError::StockInsufficient {
            product: "Café".into(),
            available: "15".into()
        }
    );
    assert_eq!(
        stock
            .record(
                company,
                &ana,
                stock_draft(coffee, Adjustment, "-16", "2026-11-06")
            )
            .await
            .unwrap_err(),
        ApplicationError::StockInsufficient {
            product: "Café".into(),
            available: "15".into()
        }
    );
    assert_eq!(
        stock
            .list(company, &StockMovementQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .len(),
        before
    );
    // What cannot be stocked or written is refused.
    assert_eq!(
        stock
            .record(
                company,
                &ana,
                stock_draft(seed.service, In, "1", "2026-11-06")
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::NotStocked)
    );
    assert_eq!(
        stock
            .record(
                company,
                &ana,
                stock_draft(ProductId::generate(), Out, "1", "2026-11-06")
            )
            .await
            .unwrap_err(),
        ApplicationError::ProductNotFound
    );
    assert_eq!(
        stock
            .record(
                other_company,
                &ana,
                stock_draft(coffee, Out, "1", "2026-11-06")
            )
            .await
            .unwrap_err(),
        ApplicationError::ProductNotFound
    );
    assert_eq!(
        stock
            .record(company, &ana, stock_draft(coffee, In, "1", "2026-11-06"))
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::StockCostRequired)
    );

    // An adjustment up is worth the average; one down takes at the average.
    let up = stock
        .record(
            company,
            &ana,
            stock_draft(coffee, Adjustment, "3", "2026-11-07"),
        )
        .await
        .unwrap();
    assert_eq!(up.value().to_string(), "18");
    let down = stock
        .record(
            company,
            &ana,
            stock_draft(coffee, Adjustment, "-3", "2026-11-07"),
        )
        .await
        .unwrap();
    assert_eq!(down.value().to_string(), "-18");

    // Undone by a movement of the opposite quantity and value, once; a reversal is not undone.
    let undone = stock
        .reverse(
            company,
            &ana,
            five.id(),
            "2026-11-08",
            Some("Engano".into()),
        )
        .await
        .unwrap();
    assert_eq!(
        (
            undone.kind(),
            undone.quantity().to_string(),
            undone.value().to_string()
        ),
        (In, "5".into(), "30".into())
    );
    assert_eq!(undone.reverses(), Some(five.id()));
    assert_eq!(
        state(stock.balance_of(company, coffee).await.unwrap()),
        ("20".into(), "120".into(), Some("6".into()))
    );
    assert_eq!(
        stock
            .reverse(company, &ana, five.id(), "2026-11-08", None)
            .await
            .unwrap_err(),
        ApplicationError::Repository(RepositoryError::DuplicateReversal)
    );
    assert_eq!(
        stock
            .reverse(company, &ana, undone.id(), "2026-11-08", None)
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::StockMovementIsAReversal)
    );
    assert_eq!(
        stock
            .reverse(other_company, &ana, first.id(), "2026-11-08", None)
            .await
            .unwrap_err(),
        ApplicationError::StockMovementNotFound
    );
    // An `in` whose goods are gone is not undone.
    stock
        .record(company, &ana, stock_draft(coffee, Out, "20", "2026-11-09"))
        .await
        .unwrap();
    assert_eq!(
        stock
            .reverse(company, &ana, first.id(), "2026-11-09", None)
            .await
            .unwrap_err(),
        ApplicationError::StockInsufficient {
            product: "Café".into(),
            available: "0".into()
        }
    );
    assert_eq!(
        state(stock.balance_of(company, coffee).await.unwrap()),
        ("0".into(), "0".into(), None)
    );

    // The ledger is listed newest day first, narrowed and paged; nothing of another company's shows.
    let all = stock
        .list(company, &StockMovementQuery::default(), None, None)
        .await
        .unwrap();
    let days: Vec<String> = all
        .items
        .iter()
        .map(|movement| movement.occurred_on().to_string())
        .collect();
    let mut newest_first = days.clone();
    newest_first.sort();
    newest_first.reverse();
    assert_eq!(days, newest_first);
    let page = stock
        .list(company, &StockMovementQuery::default(), None, Some(3))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 3);
    let rest = stock
        .list(
            company,
            &StockMovementQuery::default(),
            page.next.as_ref(),
            Some(200),
        )
        .await
        .unwrap();
    assert_eq!(page.items.len() + rest.items.len(), all.items.len());
    assert!(rest.next.is_none());
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    for (query, expected) in [
        (
            StockMovementQuery::default().of_product(coffee),
            all.items.len(),
        ),
        (
            StockMovementQuery::default().of_product(seed.other_product),
            0,
        ),
        (StockMovementQuery::default().of_kind(In), 3),
        (StockMovementQuery::default().from(day("2026-11-08")), 2),
        (StockMovementQuery::default().to(day("2026-11-01")), 1),
        (
            StockMovementQuery::default()
                .from(day("2026-11-06"))
                .to(day("2026-11-06")),
            2,
        ),
    ] {
        let found = stock.list(company, &query, None, Some(200)).await.unwrap();
        assert_eq!(found.items.len(), expected, "{query:?}");
    }
    assert!(
        stock
            .list(other_company, &StockMovementQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );

    // The counts per kind add up to the ledger, follow the product and days and never the kind asked for.
    let kinds = |query: StockMovementQuery| {
        let stock = &stock;
        async move {
            let mut counts = stock.kind_counts(company, &query).await.unwrap();
            counts.sort_by_key(|(kind, _)| kind.as_str());
            counts
        }
    };
    let everything = kinds(StockMovementQuery::default()).await;
    assert_eq!(
        everything.iter().map(|(_, count)| count).sum::<u64>(),
        all.items.len() as u64
    );
    assert_eq!(
        kinds(StockMovementQuery::default().of_kind(In)).await,
        everything
    );
    assert_eq!(
        kinds(StockMovementQuery::default().from(day("2026-11-08")))
            .await
            .iter()
            .map(|(_, count)| count)
            .sum::<u64>(),
        2
    );
    assert!(
        kinds(StockMovementQuery::default().of_product(seed.other_product))
            .await
            .is_empty()
    );
    assert!(
        stock
            .kind_counts(other_company, &StockMovementQuery::default())
            .await
            .unwrap()
            .is_empty()
    );

    // What each product had on a day, and what has stock at all: services are left out.
    let positions = stock
        .positions(
            company,
            ProductQuery::default(),
            None,
            None,
            Some(day("2026-11-03")),
            None,
        )
        .await
        .unwrap();
    let names: Vec<&str> = positions.items.iter().map(|p| p.product.name()).collect();
    assert_eq!(names, ["Café", "Descontinuado", "Pão"]);
    assert_eq!(
        state(positions.items[0].balance),
        ("10".into(), "50".into(), Some("5".into()))
    );
    assert_eq!(
        state(positions.items[2].balance),
        ("0".into(), "0".into(), None)
    );

    // The situations filter the same products page by page, and the summary counts them without a page.
    let on = Some(day("2026-11-03"));
    for situation in [
        StockSituation::InStock,
        StockSituation::BelowMinimum,
        StockSituation::Depleted,
    ] {
        let expected: Vec<&str> = positions
            .items
            .iter()
            .filter(|position| position.is(situation))
            .map(|position| position.product.name())
            .collect();
        let mut seen: Vec<String> = Vec::new();
        let mut after = None;
        loop {
            let page = stock
                .positions(
                    company,
                    ProductQuery::default(),
                    after.as_ref(),
                    Some(1),
                    on,
                    Some(situation),
                )
                .await
                .unwrap();
            seen.extend(page.items.iter().map(|p| p.product.name().to_owned()));
            match page.next {
                Some(cursor) => after = Some(cursor),
                None => break,
            }
        }
        assert_eq!(seen, expected, "{situation:?}");
    }
    let summary = stock
        .position_summary(company, ProductQuery::default(), on)
        .await
        .unwrap();
    let count = |situation| {
        positions
            .items
            .iter()
            .filter(|position| position.is(situation))
            .count() as u64
    };
    assert_eq!(summary.total, 3);
    assert_eq!(summary.in_stock, count(StockSituation::InStock));
    assert_eq!(summary.below_minimum, count(StockSituation::BelowMinimum));
    assert_eq!(summary.depleted, count(StockSituation::Depleted));
    assert!(positions.items[0].is(StockSituation::InStock));
    assert!(positions.items[2].is(StockSituation::Depleted));
    assert_eq!(
        summary.value,
        positions
            .items
            .iter()
            .map(|position| position.balance.value())
            .sum::<Decimal>()
            .normalize()
    );

    // Two writes for the same stock at once: one lands, the other is valued again against what is left and
    // refused.
    stock
        .record(company, &ana, bought("20", "5", "2026-11-10"))
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        stock.record(company, &ana, stock_draft(coffee, Out, "12", "2026-11-10")),
        stock.record(company, &ana, stock_draft(coffee, Out, "12", "2026-11-10"))
    );
    assert_eq!(u8::from(a.is_ok()) + u8::from(b.is_ok()), 1, "{a:?} {b:?}");
    assert_eq!(
        state(stock.balance_of(company, coffee).await.unwrap()),
        ("8".into(), "40".into(), Some("5".into()))
    );
}

/// The exit criterion of Phase 5 for stock: goods received enter at their cost, a sale takes them at the average
/// cost (worked out below), a return puts back what the sale took, and cancelling a sale undoes its exits; each
/// step is written once however often it is asked for.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn stock_follows_purchases_and_sales<P, E, F, R, T, S, Q, M, Pr, St>(
    purchases: &PurchaseService<P, E, F, R, T>,
    sales: &SaleService<S, Q, E, F, R, T>,
    stock: &StockService<M, Pr, St>,
    company: CompanyId,
    entry_seed: &EntrySeed,
    seed: &SalesSeed,
) where
    P: PurchaseOrderRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
    S: SaleRepository,
    Q: QuoteRepository,
    M: StockMovementRepository,
    Pr: ProductRepository,
    St: StockSettingsRepository,
{
    let ana = actor();
    let state = |balance: conta_verde_domain::StockBalance| {
        (
            balance.quantity().to_string(),
            balance.value().to_string(),
            balance.average_cost().map(|cost| cost.to_string()),
        )
    };
    let coffee = || async { stock.balance_of(company, seed.product).await.unwrap() };
    let bread = || async { stock.balance_of(company, seed.other_product).await.unwrap() };

    // 10 coffees at 5 (50) and 3 breads at 3.33 (9.99) are received.
    let mut order = purchase_draft(entry_seed, seed);
    order.lines = vec![
        sale_line(seed.product, "Café 500g", "10", "5", "0"),
        sale_line(seed.other_product, "Pão francês", "3", "3.33", "0"),
    ];
    let order = purchases.register(company, &ana, order).await.unwrap();
    purchases.approve(company, &ana, order.id()).await.unwrap();
    let received = purchases
        .receive(
            company,
            &ana,
            order.id(),
            receipt_draft("2026-11-02", &[(1, "10"), (2, "3")]),
        )
        .await
        .unwrap();
    assert_eq!(
        state(coffee().await),
        ("10".into(), "50".into(), Some("5".into()))
    );
    assert_eq!(
        state(bread().await),
        ("3".into(), "9.99".into(), Some("3.33".into()))
    );
    // Finishing the receipt again enters nothing twice.
    purchases
        .finish_receipt(company, &ana, order.id(), received.receipt.id())
        .await
        .unwrap();
    assert_eq!(state(coffee().await).0, "10");

    // 10 more coffees at 7 (70): 20 worth 120, average 6.
    let mut more = purchase_draft(entry_seed, seed);
    more.lines = vec![sale_line(seed.product, "Café 500g", "10", "7", "0")];
    let more = purchases.register(company, &ana, more).await.unwrap();
    purchases.approve(company, &ana, more.id()).await.unwrap();
    purchases
        .receive(
            company,
            &ana,
            more.id(),
            receipt_draft("2026-11-03", &[(1, "10")]),
        )
        .await
        .unwrap();
    assert_eq!(
        state(coffee().await),
        ("20".into(), "120".into(), Some("6".into()))
    );

    // A sale of 3 coffees and 1.5 breads, completed: the coffees leave at the average (120 x 3/20 = 18) and the
    // breads at theirs (9.99 x 1.5/3 = 4.995, which rounds up to 5). Completing again takes nothing twice.
    let sale = sales
        .register(company, &ana, sale_draft(entry_seed, seed))
        .await
        .unwrap();
    for _ in 0..2 {
        sales.complete(company, &ana, sale.id()).await.unwrap();
    }
    assert_eq!(
        state(coffee().await),
        ("17".into(), "102".into(), Some("6".into()))
    );
    assert_eq!(
        state(bread().await),
        ("1.5".into(), "4.99".into(), Some("3.326667".into()))
    );
    let exits = stock
        .list(
            company,
            &StockMovementQuery::default().of_kind(conta_verde_domain::StockMovementKind::Out),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(exits.items.len(), 2);
    assert!(
        exits
            .items
            .iter()
            .all(|movement| movement.occurred_on().to_string() == "2026-11-06")
    );

    // A return of 1 coffee puts back 1/3 of what the sale took (18 / 3 = 6); the next, of 2, the rest (12).
    let refund = Refund {
        category_id: entry_seed.category,
        due_on: "2026-11-20".into(),
    };
    sales
        .register_return(
            company,
            &ana,
            sale.id(),
            return_draft(&[(1, "1")]),
            refund.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        state(coffee().await),
        ("18".into(), "108".into(), Some("6".into()))
    );
    sales
        .register_return(company, &ana, sale.id(), return_draft(&[(1, "2")]), refund)
        .await
        .unwrap();
    assert_eq!(
        state(coffee().await),
        ("20".into(), "120".into(), Some("6".into()))
    );

    // Another sale, cancelled: its exits are undone and the stock is as it was; asking again changes nothing.
    let other = sales
        .register(company, &ana, sale_draft(entry_seed, seed))
        .await
        .unwrap();
    sales.complete(company, &ana, other.id()).await.unwrap();
    assert_eq!(state(coffee().await).0, "17");
    sales.cancel(company, &ana, other.id()).await.unwrap();
    assert_eq!(
        state(coffee().await),
        ("20".into(), "120".into(), Some("6".into()))
    );
    assert_eq!(state(bread().await).0, "1.5");
    // A sale of a service moves nothing.
    let mut consulting = sale_draft(entry_seed, seed);
    consulting.lines = vec![sale_line(seed.service, "Consultoria", "1", "100", "0")];
    let consulting = sales.register(company, &ana, consulting).await.unwrap();
    sales
        .complete(company, &ana, consulting.id())
        .await
        .unwrap();
    assert_eq!(state(coffee().await).0, "20");
}

/// Minimum stock alerts and the block of a sale without balance ([ADR 0055]).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn blocks_a_sale_without_balance_when_the_company_says_so<S, Q, E, F, R, T, M, Pr, St>(
    sales: &SaleService<S, Q, E, F, R, T>,
    stock: &StockService<M, Pr, St>,
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
    M: StockMovementRepository,
    Pr: ProductRepository,
    St: StockSettingsRepository,
{
    use conta_verde_domain::StockMovementKind::In;
    let ana = actor();
    let bought = |product: ProductId, quantity: &str| {
        let mut draft = stock_draft(product, In, quantity, "2026-11-01");
        draft.unit_cost = Some("5".into());
        draft
    };
    let short = |product: &str, available: &str| ApplicationError::StockInsufficient {
        product: product.into(),
        available: available.into(),
    };

    // A company that never said allows a sale without balance; saying so is audited, and saying the same twice
    // is not.
    assert!(
        !stock
            .settings(company)
            .await
            .unwrap()
            .blocks_sales_without_balance()
    );
    assert!(stock.settings_history(company).await.unwrap().is_empty());

    // The alerts: active products with a minimum (2.5 each) and less than it, by name. A product that is retired
    // and a service have none.
    let alerts = |alerts: Vec<conta_verde_application::StockAlert>| -> Vec<(String, String)> {
        alerts
            .iter()
            .map(|alert| (alert.product.name().to_owned(), alert.quantity.to_string()))
            .collect()
    };
    assert_eq!(
        alerts(stock.alerts(company).await.unwrap()),
        [("Café".into(), "0".into()), ("Pão".into(), "0".into())]
    );
    stock
        .record(company, &ana, bought(seed.product, "2"))
        .await
        .unwrap();
    assert_eq!(
        alerts(stock.alerts(company).await.unwrap()),
        [("Café".into(), "2".into()), ("Pão".into(), "0".into())]
    );
    stock
        .record(company, &ana, bought(seed.product, "3"))
        .await
        .unwrap();
    assert_eq!(
        alerts(stock.alerts(company).await.unwrap()),
        [("Pão".into(), "0".into())]
    );
    assert!(stock.alerts(other_company).await.unwrap().is_empty());

    // The block is on: a sale that the stock cannot cover is refused before it changes, naming what is short.
    assert!(
        stock
            .replace_settings(company, &ana, true)
            .await
            .unwrap()
            .blocks_sales_without_balance()
    );
    stock.replace_settings(company, &ana, true).await.unwrap();
    assert_eq!(stock.settings_history(company).await.unwrap().len(), 1);
    let sale = sales
        .register(company, &ana, sale_draft(entry_seed, seed))
        .await
        .unwrap();
    assert_eq!(
        sales.complete(company, &ana, sale.id()).await.unwrap_err(),
        short("Pão", "0")
    );
    assert_eq!(
        sales.get(company, sale.id()).await.unwrap().status(),
        SaleStatus::Open
    );
    assert!(
        sales
            .receivable(company, sale.id())
            .await
            .unwrap()
            .is_none()
    );
    stock
        .record(company, &ana, bought(seed.other_product, "1"))
        .await
        .unwrap();
    assert_eq!(
        sales.complete(company, &ana, sale.id()).await.unwrap_err(),
        short("Pão", "1")
    );
    stock
        .record(company, &ana, bought(seed.other_product, "0.5"))
        .await
        .unwrap();
    sales.complete(company, &ana, sale.id()).await.unwrap();
    let state = |balance: conta_verde_domain::StockBalance| balance.quantity().to_string();
    assert_eq!(
        state(stock.balance_of(company, seed.product).await.unwrap()),
        "2"
    );
    assert_eq!(
        state(stock.balance_of(company, seed.other_product).await.unwrap()),
        "0"
    );

    // The same product on two lines is asked for together: 2 and 2 of the 2 that are left.
    let mut twice = sale_draft(entry_seed, seed);
    twice.lines = vec![
        sale_line(seed.product, "Café 500g", "1", "10", "0"),
        sale_line(seed.product, "Café 250g", "2", "10", "0"),
    ];
    let twice = sales.register(company, &ana, twice).await.unwrap();
    assert_eq!(
        sales.complete(company, &ana, twice.id()).await.unwrap_err(),
        short("Café", "2")
    );

    // The block is off again: the sale is made and the balance goes below zero.
    stock.replace_settings(company, &ana, false).await.unwrap();
    assert_eq!(stock.settings_history(company).await.unwrap().len(), 2);
    sales.complete(company, &ana, twice.id()).await.unwrap();
    assert_eq!(
        state(stock.balance_of(company, seed.product).await.unwrap()),
        "-1"
    );
    assert!(
        !stock
            .settings(other_company)
            .await
            .unwrap()
            .blocks_sales_without_balance()
    );
}

fn count_draft(lines: &[(ProductId, &str)]) -> conta_verde_domain::InventoryCountDraft {
    conta_verde_domain::InventoryCountDraft {
        counted_on: "2026-11-30".into(),
        notes: Some("Fim do mês".into()),
        lines: lines
            .iter()
            .map(|(product, counted)| conta_verde_domain::CountLineDraft {
                product_id: *product,
                counted: (*counted).into(),
            })
            .collect(),
    }
}

/// An inventory count from its writing to its application ([ADR 0056]): looked at against what the stock holds
/// now, applied once as adjustments valued at the average, and fixed after that.
#[allow(clippy::too_many_lines)]
pub async fn runs_the_inventory_count<C, M, P, St>(
    counts: &conta_verde_application::InventoryCountService<C, StockService<M, P, St>>,
    stock: &StockService<M, P, St>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &SalesSeed,
) where
    C: conta_verde_application::InventoryCountRepository,
    M: StockMovementRepository,
    P: ProductRepository,
    St: StockSettingsRepository,
{
    use conta_verde_application::InventoryCountQuery;
    use conta_verde_domain::{CountStatus, StockMovementKind::In};
    let ana = actor();
    let (coffee, bread) = (seed.product, seed.other_product);
    let entered = |product: ProductId, quantity: &str, unit_cost: &str| {
        let mut draft = stock_draft(product, In, quantity, "2026-11-01");
        draft.unit_cost = Some(unit_cost.into());
        draft
    };
    stock
        .record(company, &ana, entered(coffee, "10", "5"))
        .await
        .unwrap();
    stock
        .record(company, &ana, entered(bread, "2", "3"))
        .await
        .unwrap();
    let figures = |view: &conta_verde_application::CountView| -> Vec<(String, String, String)> {
        view.lines
            .iter()
            .map(|line| {
                (
                    line.counted.to_string(),
                    line.expected.map_or("-".into(), |value| value.to_string()),
                    line.difference
                        .map_or("-".into(), |value| value.to_string()),
                )
            })
            .collect()
    };

    // Written open and numbered per company; what cannot be counted is refused.
    let first = counts
        .register(company, &ana, count_draft(&[(coffee, "8"), (bread, "2")]))
        .await
        .unwrap();
    assert_eq!((first.number(), first.status()), (1, CountStatus::Open));
    for (lines, error) in [
        (
            vec![(seed.service, "1")],
            ApplicationError::Domain(DomainError::NotStocked),
        ),
        (
            vec![(ProductId::generate(), "1")],
            ApplicationError::ProductNotFound,
        ),
        (
            vec![(coffee, "1"), (coffee, "2")],
            ApplicationError::Domain(DomainError::DuplicateCountLine),
        ),
        (vec![], ApplicationError::Domain(DomainError::NoCountLines)),
    ] {
        assert_eq!(
            counts
                .register(company, &ana, count_draft(&lines))
                .await
                .unwrap_err(),
            error
        );
    }
    assert_eq!(
        counts.view(other_company, first.id()).await.unwrap_err(),
        ApplicationError::CountNotFound
    );
    assert_eq!(
        counts
            .list(company, &InventoryCountQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .len(),
        1
    );

    // While it is open it says what the stock holds now, and the difference to what was found.
    let view = counts.view(company, first.id()).await.unwrap();
    assert_eq!(
        figures(&view),
        [
            ("8".into(), "10".into(), "-2".into()),
            ("2".into(), "2".into(), "0".into())
        ]
    );
    let changed = counts
        .update(
            company,
            &ana,
            first.id(),
            count_draft(&[(coffee, "8"), (bread, "3")]),
        )
        .await
        .unwrap();
    assert_eq!(changed.number(), 1);
    counts
        .update(
            company,
            &ana,
            first.id(),
            count_draft(&[(coffee, "8"), (bread, "3")]),
        )
        .await
        .unwrap();
    assert_eq!(counts.history(company, first.id()).await.unwrap().len(), 2);
    stock
        .record(
            company,
            &ana,
            stock_draft(
                coffee,
                conta_verde_domain::StockMovementKind::Out,
                "1",
                "2026-11-20",
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        figures(&counts.view(company, first.id()).await.unwrap()),
        [
            ("8".into(), "9".into(), "-1".into()),
            ("3".into(), "2".into(), "1".into())
        ]
    );

    // Applied: the differences are written once as adjustments on the day of the count, at the average cost
    // (9 coffees worth 45 lose one: -5; 2 breads worth 6 gain one: +3).
    let applied = counts.apply(company, &ana, first.id()).await.unwrap();
    assert_eq!(applied.status(), CountStatus::Applied);
    let adjustments = stock
        .list(
            company,
            &StockMovementQuery::default()
                .of_kind(conta_verde_domain::StockMovementKind::Adjustment),
            None,
            None,
        )
        .await
        .unwrap();
    let mut seen: Vec<(String, String, String)> = adjustments
        .items
        .iter()
        .map(|movement| {
            (
                movement.quantity().to_string(),
                movement.value().to_string(),
                movement.occurred_on().to_string(),
            )
        })
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        [
            ("-1".into(), "-5".into(), "2026-11-30".into()),
            ("1".into(), "3".into(), "2026-11-30".into())
        ]
    );
    assert_eq!(
        stock
            .balance_of(company, coffee)
            .await
            .unwrap()
            .quantity()
            .to_string(),
        "8"
    );
    assert_eq!(
        stock
            .balance_of(company, bread)
            .await
            .unwrap()
            .quantity()
            .to_string(),
        "3"
    );
    assert_eq!(
        figures(&counts.view(company, first.id()).await.unwrap()),
        [
            ("8".into(), "9".into(), "-1".into()),
            ("3".into(), "2".into(), "1".into())
        ]
    );
    // Applying again writes nothing, and an applied count is fixed.
    counts.apply(company, &ana, first.id()).await.unwrap();
    assert_eq!(
        stock
            .list(
                company,
                &StockMovementQuery::default()
                    .of_kind(conta_verde_domain::StockMovementKind::Adjustment),
                None,
                None
            )
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    for error in [
        counts.cancel(company, &ana, first.id()).await.unwrap_err(),
        counts
            .update(company, &ana, first.id(), count_draft(&[(coffee, "1")]))
            .await
            .unwrap_err(),
    ] {
        assert_eq!(error, ApplicationError::Domain(DomainError::CountIsLocked));
    }
    assert_eq!(counts.history(company, first.id()).await.unwrap().len(), 3);

    // A cancelled count writes nothing and is not applied.
    let second = counts
        .register(company, &ana, count_draft(&[(coffee, "1")]))
        .await
        .unwrap();
    assert_eq!(second.number(), 2);
    assert_eq!(
        counts
            .cancel(company, &ana, second.id())
            .await
            .unwrap()
            .status(),
        CountStatus::Cancelled
    );
    assert_eq!(
        counts.apply(company, &ana, second.id()).await.unwrap_err(),
        ApplicationError::Domain(DomainError::CountIsLocked)
    );
    assert_eq!(
        figures(&counts.view(company, second.id()).await.unwrap()),
        [("1".into(), "-".into(), "-".into())]
    );
    assert_eq!(
        counts
            .list(
                company,
                &InventoryCountQuery::default().with_status(CountStatus::Cancelled),
                None,
                None
            )
            .await
            .unwrap()
            .items
            .len(),
        1
    );

    // Two applications at once write the adjustments once.
    let third = counts
        .register(company, &ana, count_draft(&[(coffee, "1")]))
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        counts.apply(company, &ana, third.id()),
        counts.apply(company, &ana, third.id())
    );
    assert!(a.is_ok() && b.is_ok(), "{a:?} {b:?}");
    assert_eq!(
        stock
            .balance_of(company, coffee)
            .await
            .unwrap()
            .quantity()
            .to_string(),
        "1"
    );
    assert_eq!(
        stock
            .list(
                company,
                &StockMovementQuery::default()
                    .of_kind(conta_verde_domain::StockMovementKind::Adjustment),
                None,
                None
            )
            .await
            .unwrap()
            .items
            .len(),
        3
    );

    // The count says what was found on its day: goods that came in after it are not undone by it. 10 more come in on
    // December 1st; a count of the 30th that found 1 agrees with what the stock held through the 30th.
    let mut later = entered(coffee, "10", "5");
    later.occurred_on = "2026-12-01".into();
    stock.record(company, &ana, later).await.unwrap();
    let fourth = counts
        .register(company, &ana, count_draft(&[(coffee, "1")]))
        .await
        .unwrap();
    assert_eq!(
        figures(&counts.view(company, fourth.id()).await.unwrap()),
        [("1".into(), "1".into(), "0".into())]
    );
    counts.apply(company, &ana, fourth.id()).await.unwrap();
    assert_eq!(
        stock
            .list(
                company,
                &StockMovementQuery::default()
                    .of_kind(conta_verde_domain::StockMovementKind::Adjustment),
                None,
                None
            )
            .await
            .unwrap()
            .items
            .len(),
        3
    );
    assert_eq!(
        stock
            .balance_of(company, coffee)
            .await
            .unwrap()
            .quantity()
            .to_string(),
        "11"
    );
}

/// A supplier's NF-e entered as a purchase ([ADR 0057]): payables on the installments the invoice gives, stock at
/// what each item costs, and the same invoice again changes nothing.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn enters_a_supplier_invoice_once<P, E, F, R, T, M, Pr, St>(
    purchases: &PurchaseService<P, E, F, R, T>,
    stock: &StockService<M, Pr, St>,
    entries: &FinancialEntryService<E, F>,
    company: CompanyId,
    entry_seed: &EntrySeed,
    seed: &SalesSeed,
) where
    P: PurchaseOrderRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: DocumentReferences,
    T: DocumentStock,
    M: StockMovementRepository,
    Pr: ProductRepository,
    St: StockSettingsRepository,
{
    use crate::nfe_xml::{parse_nfe, tests::sample};
    use conta_verde_application::InvoiceEntry;
    let ana = actor();
    let cnpj = "11222333000181";
    let entry = || InvoiceEntry {
        category_id: entry_seed.category,
        cost_center_id: Some(entry_seed.cost_center),
        payment_term_id: None,
        received_on: Some("2026-11-10".into()),
        supplier_id: None,
        products: Vec::new(),
    };
    let draft =
        |series: u32, number: u32| parse_nfe(sample(cnpj, series, number).as_bytes()).unwrap();

    // Looked at before it is entered: the supplier and the products its codes find, and that it is new.
    let preview = purchases
        .preview_invoice(company, draft(1, 4321))
        .await
        .unwrap();
    assert_eq!(preview.supplier, Some(seed.supplier));
    assert_eq!(
        preview.products,
        [Some(seed.product), Some(seed.other_product)]
    );
    assert!(preview.entered.is_none());
    assert_eq!(preview.invoice.total().to_string(), "111.5");

    // Entered: a purchase received whole, its payable on the installments the invoice gives, the goods in stock at
    // what each item costs (100 + 2.5 of IPI for the coffee, 9.99 - 0.99 for the bread).
    let outcome = purchases
        .enter_invoice(company, &ana, draft(1, 4321), entry())
        .await
        .unwrap();
    assert!(!outcome.already_entered);
    assert_eq!(outcome.order.status(), PurchaseStatus::Received);
    assert_eq!(outcome.order.supplier_id(), seed.supplier);
    assert_eq!(outcome.order.total().to_string(), "111.5");
    assert_eq!(
        outcome.order.invoice_key().map(|key| key.as_str()),
        Some(preview.invoice.access_key().as_str())
    );
    let payable = outcome.receipt.payable.clone().unwrap();
    assert_eq!(payable.party_id(), Some(seed.supplier));
    assert_eq!(payable.competence_date().to_string(), "2026-11-10");
    assert_eq!(
        payable.installments_text(),
        "1:2026-12-06:55.75,2:2027-01-06:55.75"
    );
    let held = |balance: conta_verde_domain::StockBalance| {
        (balance.quantity().to_string(), balance.value().to_string())
    };
    assert_eq!(
        held(stock.balance_of(company, seed.product).await.unwrap()),
        ("10".into(), "102.5".into())
    );
    assert_eq!(
        held(stock.balance_of(company, seed.other_product).await.unwrap()),
        ("3".into(), "9".into())
    );
    assert_eq!(
        purchases
            .preview_invoice(company, draft(1, 4321))
            .await
            .unwrap()
            .entered
            .map(|order| order.id()),
        Some(outcome.order.id())
    );

    // An order made from an invoice is received by entering the invoice, never on its own.
    assert_eq!(
        purchases
            .receive(
                company,
                &ana,
                outcome.order.id(),
                receipt_draft("2026-11-12", &[(1, "1")])
            )
            .await
            .unwrap_err(),
        ApplicationError::Domain(DomainError::OrderFollowsAnInvoice)
    );

    // The same file again, in sequence and at the same moment, adds nothing.
    let again = purchases
        .enter_invoice(company, &ana, draft(1, 4321), entry())
        .await
        .unwrap();
    assert!(again.already_entered);
    assert_eq!(again.order.id(), outcome.order.id());
    assert_eq!(again.receipt.receipt.id(), outcome.receipt.receipt.id());
    let (a, b) = tokio::join!(
        purchases.enter_invoice(company, &ana, draft(1, 4322), entry()),
        purchases.enter_invoice(company, &ana, draft(1, 4322), entry())
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.order.id(), b.order.id());
    assert_eq!(u8::from(a.already_entered) + u8::from(b.already_entered), 1);
    assert_eq!(
        entries
            .list(company, &FinancialEntryQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        held(stock.balance_of(company, seed.product).await.unwrap()),
        ("20".into(), "205".into())
    );
    assert_eq!(
        purchases
            .list(company, &PurchaseOrderQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .len(),
        2
    );

    // What cannot be entered leaves nothing: an issuer nobody registered, an item with no product, an invoice
    // whose totals do not add up.
    let stranger = parse_nfe(sample("11444777000161", 1, 4321).as_bytes()).unwrap();
    assert_eq!(
        purchases
            .enter_invoice(company, &ana, stranger, entry())
            .await
            .unwrap_err(),
        ApplicationError::SupplierNotRegistered
    );
    let unknown_code = parse_nfe(
        sample(cnpj, 1, 4323)
            .replace("CAFE-500", "DESCONHECIDO")
            .as_bytes(),
    )
    .unwrap();
    assert_eq!(
        purchases
            .enter_invoice(company, &ana, unknown_code.clone(), entry())
            .await
            .unwrap_err(),
        ApplicationError::InvoiceItemNeedsAProduct(1)
    );
    // ... until the person says which product it is.
    let mapped = purchases
        .enter_invoice(
            company,
            &ana,
            unknown_code,
            InvoiceEntry {
                products: vec![Some(seed.product), None],
                ..entry()
            },
        )
        .await
        .unwrap();
    assert!(!mapped.already_entered);
    let broken = parse_nfe(
        sample(cnpj, 1, 4324)
            .replace("<vNF>111.50</vNF>", "<vNF>111.51</vNF>")
            .as_bytes(),
    )
    .unwrap();
    assert!(
        purchases
            .enter_invoice(company, &ana, broken, entry())
            .await
            .is_err()
    );
    assert_eq!(
        purchases
            .list(company, &PurchaseOrderQuery::default(), None, None)
            .await
            .unwrap()
            .items
            .len(),
        3
    );
}

/// The position, valuation and movements of the stock on a day and over a span ([ADR 0058]).
#[allow(clippy::too_many_lines)]
pub async fn reports_the_stock_valuation_and_its_movements<M, P, St>(
    stock: &StockService<M, P, St>,
    company: CompanyId,
    other_company: CompanyId,
    seed: &SalesSeed,
) where
    M: StockMovementRepository,
    P: ProductRepository,
    St: StockSettingsRepository,
{
    use conta_verde_domain::StockMovementKind::{Adjustment, In, Out};
    let ana = actor();
    let day = |text: &str| CalendarDate::parse(text).unwrap();
    let entered = |product: ProductId, quantity: &str, unit_cost: &str, when: &str| {
        let mut draft = stock_draft(product, In, quantity, when);
        draft.unit_cost = Some(unit_cost.into());
        draft
    };
    for draft in [
        entered(seed.product, "10", "5", "2026-11-01"),
        entered(seed.product, "10", "7", "2026-11-06"),
        stock_draft(seed.product, Out, "5", "2026-11-06"),
        entered(seed.other_product, "4", "3", "2026-11-10"),
        stock_draft(seed.product, Adjustment, "-1", "2026-11-12"),
    ] {
        stock.record(company, &ana, draft).await.unwrap();
    }
    let rows = |valuation: &conta_verde_application::StockValuation| -> Vec<(String, String, String, String)> {
        valuation
            .rows
            .iter()
            .map(|row| {
                (
                    row.product.name().to_owned(),
                    row.balance.quantity().to_string(),
                    row.balance.value().to_string(),
                    row.share.to_string(),
                )
            })
            .collect()
    };

    // What the stock was worth on a day, by product (biggest value first) and in all.
    let early = stock.valuation(company, day("2026-11-06")).await.unwrap();
    assert_eq!(
        rows(&early),
        [("Café".into(), "15".into(), "90".into(), "100".into())]
    );
    assert_eq!(early.total.to_string(), "90");
    let later = stock.valuation(company, day("2026-11-10")).await.unwrap();
    assert_eq!(
        rows(&later),
        [
            ("Café".into(), "15".into(), "90".into(), "88.24".into()),
            ("Pão".into(), "4".into(), "12".into(), "11.76".into())
        ]
    );
    assert_eq!(later.total.to_string(), "102");
    let before = stock.valuation(company, day("2026-10-01")).await.unwrap();
    assert!(before.rows.is_empty());
    assert_eq!(before.total.to_string(), "0");
    assert!(
        stock
            .valuation(other_company, day("2026-11-10"))
            .await
            .unwrap()
            .rows
            .is_empty()
    );

    // What moved over a span, with what each product had before and after: the exits as positive numbers.
    let summary = stock
        .movement_summary(company, day("2026-11-05"), day("2026-11-12"))
        .await
        .unwrap();
    let figures = |row: &conta_verde_application::MovementSummary| {
        [
            row.opening.quantity().to_string(),
            row.opening.value().to_string(),
            row.entered_quantity.to_string(),
            row.entered_value.to_string(),
            row.exited_quantity.to_string(),
            row.exited_value.to_string(),
            row.adjusted_quantity.to_string(),
            row.adjusted_value.to_string(),
            row.closing.quantity().to_string(),
            row.closing.value().to_string(),
        ]
    };
    let names: Vec<&str> = summary.iter().map(|row| row.product.name()).collect();
    assert_eq!(names, ["Café", "Pão"]);
    assert_eq!(
        figures(&summary[0]),
        ["10", "50", "10", "70", "5", "30", "-1", "-6", "14", "84"]
    );
    assert_eq!(
        figures(&summary[1]),
        ["0", "0", "4", "12", "0", "0", "0", "0", "4", "12"]
    );
    // A span without movements lists nothing, and a span that is not one is refused.
    assert!(
        stock
            .movement_summary(company, day("2026-12-01"), day("2026-12-31"))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        stock
            .movement_summary(other_company, day("2026-11-05"), day("2026-11-12"))
            .await
            .unwrap()
            .is_empty()
    );
    for (from, to) in [("2026-11-12", "2026-11-05"), ("2025-01-01", "2026-11-12")] {
        assert!(matches!(
            stock
                .movement_summary(company, day(from), day(to))
                .await
                .unwrap_err(),
            ApplicationError::InvalidSpan(_)
        ));
    }
}

/// How the queue of jobs hands out work ([ADR 0059]): the order, what is not yet due, the lease that lets a job be taken
/// again, the wait before a retry, the end of a job given up on, and two workers racing for it.
pub async fn runs_the_job_queue<Q: conta_verde_application::JobQueue>(queue: &Q) {
    use conta_verde_domain::{JobKind, NewJob};

    let job = |tag: &str, run_at: i64| {
        NewJob::new(
            JobKind::SendEmail,
            vec![
                format!("{tag}@example.com"),
                tag.to_owned(),
                "body".to_owned(),
            ],
            run_at,
        )
        .unwrap()
    };
    // A shared database may hold what an earlier run left behind.
    while let Some(left) = queue.claim_next(i64::MAX / 2, 1).await.unwrap() {
        queue.complete(left.id).await.unwrap();
    }

    // Due jobs come oldest first, and one that is not due yet waits.
    let a = queue.enqueue(&job("a", 100)).await.unwrap();
    let b = queue.enqueue(&job("b", 50)).await.unwrap();
    let c = queue.enqueue(&job("c", 1000)).await.unwrap();
    assert!(queue.claim_next(10, 300).await.unwrap().is_none());
    let first = queue.claim_next(100, 300).await.unwrap().unwrap();
    assert_eq!(
        (first.id, first.attempts, first.args[1].as_str()),
        (b, 1, "b")
    );
    let second = queue.claim_next(100, 300).await.unwrap().unwrap();
    assert_eq!((second.id, second.attempts), (a, 1));
    assert!(queue.claim_next(100, 300).await.unwrap().is_none());

    // A job held by a worker is not given to another until its lease ends, and then it is counted as another try.
    assert!(queue.claim_next(399, 300).await.unwrap().is_none());
    let again = queue.claim_next(400, 300).await.unwrap().unwrap();
    assert_eq!((again.id, again.attempts), (b, 2));
    queue.complete(b).await.unwrap();
    let again = queue.claim_next(400, 300).await.unwrap().unwrap();
    assert_eq!((again.id, again.attempts), (a, 2));
    queue.complete(a).await.unwrap();

    // A failed try waits for its time; a job given up on is never taken again.
    let tried = queue.claim_next(1000, 300).await.unwrap().unwrap();
    assert_eq!((tried.id, tried.attempts), (c, 1));
    queue.fail(c, "boom", Some(2000)).await.unwrap();
    assert!(queue.claim_next(1999, 300).await.unwrap().is_none());
    let tried = queue.claim_next(2000, 300).await.unwrap().unwrap();
    assert_eq!((tried.id, tried.attempts), (c, 2));
    queue.fail(c, "boom", None).await.unwrap();
    assert!(queue.claim_next(i64::MAX / 2, 300).await.unwrap().is_none());

    // Two workers asking at once get two different jobs.
    let d = queue.enqueue(&job("d", 0)).await.unwrap();
    let e = queue.enqueue(&job("e", 0)).await.unwrap();
    let (x, y) = tokio::join!(queue.claim_next(5, 60), queue.claim_next(5, 60));
    let (x, y) = (x.unwrap().unwrap(), y.unwrap().unwrap());
    assert_ne!(x.id, y.id);
    assert!([d, e].contains(&x.id) && [d, e].contains(&y.id));
    queue.complete(d).await.unwrap();
    queue.complete(e).await.unwrap();
}

fn import_row(reference: &str, category: &str, amount: &str) -> EntryImportDraft {
    EntryImportDraft {
        reference: reference.into(),
        kind: "payable".into(),
        description: format!("Imported {reference}"),
        party_document: None,
        party_name: None,
        category: category.into(),
        cost_center: None,
        competence_date: "2026-03-10".into(),
        due_date: "2026-03-15".into(),
        amount: amount.into(),
        settlement: None,
    }
}

fn paid_on(day: &str, account: &str) -> Option<SettlementImport> {
    Some(SettlementImport {
        settled_on: day.into(),
        applied: None,
        interest: None,
        fine: None,
        discount: None,
        bank_account: account.into(),
    })
}

async fn imported_entry<R, F>(
    service: &FinancialEntryService<R, F>,
    company: CompanyId,
    reference: &str,
) -> Option<FinancialEntry>
where
    R: FinancialEntryRepository,
    F: EntryReferences,
{
    let origin = EntryOrigin::Import(ImportId::new(company, reference).unwrap());
    service.made_by(company, origin).await.unwrap()
}

/// A file of entries is checked as a whole before anything is written, finds what it names (a category
/// by its path, a party by name, a cost center, a bank account) among the active records of the company,
/// writes each row with its audit entry and its settlement, and sending it again adds nothing and finishes
/// a settlement a failure left unmade (ADR 0064).
pub async fn imports_entries_from_a_file<R, F, C>(
    service: &FinancialEntryService<R, F>,
    categories: &C,
    company: CompanyId,
    other_company: CompanyId,
    seed: &EntrySeed,
) where
    R: FinancialEntryRepository,
    F: EntryReferences + EntryLookups,
    C: CategoryRepository,
{
    let ana = actor();
    let parent = categories
        .find(company, seed.category)
        .await
        .unwrap()
        .unwrap();
    let child = child_category(&parent, "Mensalidades");
    save_category(categories, &child).await;

    let mut paid = import_row("a-1", "Vendas > Mensalidades", "100.50");
    paid.settlement = paid_on("2026-03-16", "Conta principal");
    paid.settlement.as_mut().unwrap().interest = Some("1.5".into());
    let mut open = import_row("a-2", "vendas", "40");
    open.kind = "receivable".into();
    open.party_name = Some("Mercado Boa Compra".into());
    open.cost_center = Some("Matriz".into());
    let file = || vec![(2, paid.clone()), (3, open.clone())];

    // A dry run says what would happen and writes nothing.
    let dry = service
        .import_entries(company, &ana, file(), true)
        .await
        .unwrap();
    assert_eq!((dry.valid, dry.created), (2, 0));
    assert!(dry.refused.is_empty() && dry.skipped.is_empty());
    assert!(imported_entry(service, company, "a-1").await.is_none());

    let report = service
        .import_entries(company, &ana, file(), false)
        .await
        .unwrap();
    assert_eq!((report.valid, report.created), (2, 2));
    let first = imported_entry(service, company, "a-1").await.unwrap();
    assert_eq!(first.category_id(), child.id());
    assert_eq!(first.total(), "100.5".parse::<Decimal>().unwrap());
    let settlements = service.settlements(company, first.id()).await.unwrap();
    assert_eq!(settlements.len(), 1);
    assert_eq!(settlements[0].bank_account_id(), seed.bank_account);
    assert_eq!(settlements[0].cash(), "102".parse::<Decimal>().unwrap());
    let second = imported_entry(service, company, "a-2").await.unwrap();
    assert_eq!(second.party_id(), Some(seed.party));
    assert_eq!(second.cost_center_id(), Some(seed.cost_center));
    assert!(
        service
            .settlements(company, second.id())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(service.history(company, first.id()).await.unwrap().len(), 1);

    // Sending it again adds nothing, in a dry run too.
    for dry_run in [true, false] {
        let again = service
            .import_entries(company, &ana, file(), dry_run)
            .await
            .unwrap();
        assert_eq!(again.created, 0);
        assert_eq!(again.skipped.len(), 2);
    }
    assert_eq!(
        service
            .settlements(company, first.id())
            .await
            .unwrap()
            .len(),
        1
    );

    // An entry whose settlement was never made gets it when the file comes again.
    let mut unfinished = import_row("a-3", "Vendas", "25");
    service
        .import_entries(company, &ana, vec![(2, unfinished.clone())], false)
        .await
        .unwrap();
    unfinished.settlement = paid_on("2026-03-20", "Reserva");
    let finish = service
        .import_entries(company, &ana, vec![(2, unfinished)], false)
        .await
        .unwrap();
    assert_eq!((finish.created, finish.skipped.len()), (0, 1));
    let third = imported_entry(service, company, "a-3").await.unwrap();
    assert_eq!(
        service
            .settlements(company, third.id())
            .await
            .unwrap()
            .len(),
        1
    );

    // A name alone is enough while it means one category; once two trees have it, the path is asked for.
    service
        .import_entries(
            company,
            &ana,
            vec![(2, import_row("a-4", "mensalidades", "5"))],
            false,
        )
        .await
        .unwrap();
    let bare = imported_entry(service, company, "a-4").await.unwrap();
    assert_eq!(bare.category_id(), child.id());
    let other_parent = categories
        .find(company, seed.other_category)
        .await
        .unwrap()
        .unwrap();
    save_category(categories, &child_category(&other_parent, "Mensalidades")).await;
    let ambiguous = service
        .import_entries(
            company,
            &ana,
            vec![
                (2, import_row("a-5", "Mensalidades", "5")),
                (3, import_row("a-6", "Serviços > Mensalidades", "5")),
            ],
            false,
        )
        .await
        .unwrap();
    assert_eq!(ambiguous.refused.len(), 1);
    assert_eq!(ambiguous.refused[0].row, 2);
    assert!(ambiguous.refused[0].message.contains("Root > Child"));
    assert!(imported_entry(service, company, "a-6").await.is_none());

    // One refused row refuses the file: every problem is named by its row and nothing is written.
    let mut too_much = import_row("b-3", "Vendas", "10");
    too_much.settlement = paid_on("2026-03-16", "Conta principal");
    too_much.settlement.as_mut().unwrap().applied = Some("11".into());
    let mut no_account = import_row("b-4", "Vendas", "10");
    no_account.settlement = paid_on("2026-03-16", "Caixa antigo");
    let mut bad_kind = import_row("b-5", "Vendas", "10");
    bad_kind.kind = "income".into();
    let mut closed_center = import_row("b-6", "Vendas", "10");
    closed_center.cost_center = Some("Fechado".into());
    let mut stranger = import_row("b-7", "Vendas", "10");
    stranger.party_name = Some("Nobody".into());
    let problems = vec![
        (2, import_row("b-1", "Antiga", "10")),
        (3, import_row("b-2", "Transferências", "10")),
        (4, too_much),
        (5, no_account),
        (6, bad_kind),
        (7, closed_center),
        (8, stranger),
        (9, import_row("b-8", "Vendas", "10.505")),
        (10, import_row("b-9", "Vendas", "10")),
        (11, import_row("b-9", "Vendas", "10")),
        (12, import_row("b-10", "Vendas", "10")),
    ];
    let refused = service
        .import_entries(company, &ana, problems, false)
        .await
        .unwrap();
    assert_eq!((refused.valid, refused.created), (2, 0));
    let rows: Vec<usize> = refused.refused.iter().map(|note| note.row).collect();
    assert_eq!(rows, vec![2, 3, 4, 5, 6, 7, 8, 9, 11]);
    assert!(imported_entry(service, company, "b-10").await.is_none());

    // What belongs to another company is not found there.
    let elsewhere = service
        .import_entries(
            other_company,
            &ana,
            vec![(2, import_row("a-1", "Vendas", "10"))],
            false,
        )
        .await
        .unwrap();
    assert_eq!(elsewhere.refused.len(), 1);
    assert!(
        imported_entry(service, other_company, "a-1")
            .await
            .is_none()
    );
}

/// The log of e-mailed documents ([ADR 0065]): the same document is not sent again to the same address within the
/// cooldown, a company has a cap over a day, another company's sends do not count, a send that went nowhere is taken back,
/// and what was sent is listed newest first.
pub async fn limits_and_logs_the_documents_that_are_mailed<
    L: conta_verde_application::DocumentMailRepository,
>(
    log: &L,
    company: CompanyId,
    other_company: CompanyId,
    user: UserId,
) {
    use conta_verde_application::Reservation::{CapReached, Reserved, TooSoon};
    use conta_verde_domain::{DocumentMail, DocumentMailId, DocumentMailKind};

    // A cooldown of a second, a day of ten seconds and a cap of three keep the numbers small.
    const COOLDOWN: i64 = 1_000;
    const DAY: i64 = 10_000;
    const CAP: u32 = 3;
    let (order, receipt) = (Uuid::new_v4(), Uuid::new_v4());
    let mail = |company, kind, document, to: &str, at| DocumentMail {
        id: DocumentMailId::generate(),
        company_id: company,
        kind,
        document_id: document,
        to: conta_verde_domain::Email::parse(to).unwrap(),
        sent_by: user,
        sent_at_millis: at,
    };
    let reserve =
        |m: DocumentMail| async move { log.reserve(&m, COOLDOWN, DAY, CAP).await.unwrap() };
    let po = DocumentMailKind::PurchaseOrder;

    // The same document to the same address waits out the cooldown, to the second it is over.
    assert_eq!(
        reserve(mail(company, po, order, "a@example.com", 1_000)).await,
        Reserved
    );
    assert_eq!(
        reserve(mail(company, po, order, "a@example.com", 1_500)).await,
        TooSoon
    );
    assert_eq!(
        reserve(mail(company, po, order, "a@example.com", 2_000)).await,
        Reserved
    );
    // Another address, another document or another kind is not held back by it.
    assert_eq!(
        reserve(mail(company, po, order, "b@example.com", 2_100)).await,
        Reserved
    );

    // Three went out over the day: the fourth is refused, whatever it is.
    assert_eq!(
        reserve(mail(
            company,
            DocumentMailKind::PaymentReceipt,
            receipt,
            "c@example.com",
            2_500
        ))
        .await,
        CapReached
    );
    // Another company has its own count.
    assert_eq!(
        reserve(mail(other_company, po, order, "a@example.com", 2_500)).await,
        Reserved
    );

    // What a refusal stored nothing of: the log holds the three, newest first, and none of another company's.
    let sent = log.of_document(company, po, order, 10).await.unwrap();
    assert_eq!(
        sent.iter()
            .map(|m| (m.sent_at_millis, m.to.as_str()))
            .collect::<Vec<_>>(),
        [
            (2_100, "b@example.com"),
            (2_000, "a@example.com"),
            (1_000, "a@example.com")
        ]
    );
    assert_eq!(sent[0].sent_by, user);
    assert_eq!(
        log.of_document(company, po, order, 2).await.unwrap().len(),
        2
    );
    assert!(
        log.of_document(company, DocumentMailKind::PaymentReceipt, order, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        log.of_document(other_company, po, order, 10)
            .await
            .unwrap()
            .len(),
        1
    );

    // Once the day has passed the old sends no longer count.
    assert_eq!(
        reserve(mail(company, po, order, "a@example.com", 12_001)).await,
        Reserved
    );

    // A send that went nowhere is taken back, and does not hold the next one up.
    let gone = mail(company, po, receipt, "d@example.com", 20_000);
    assert_eq!(reserve(gone.clone()).await, Reserved);
    log.release(company, gone.id).await.unwrap();
    assert_eq!(
        reserve(mail(company, po, receipt, "d@example.com", 20_000)).await,
        Reserved
    );
    // Taking back what another company holds takes back nothing.
    log.release(other_company, gone.id).await.unwrap();
}
