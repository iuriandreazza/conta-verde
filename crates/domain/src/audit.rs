use uuid::Uuid;

use crate::line::lines_text;
use crate::{
    AccountOpening, Attachment, BankAccount, Category, CompanyId, CostCenter, DocumentCheck,
    FinancialEntry, LineMatch, Party, PaymentMethod, PaymentTerm, Product, Quote, Recurrence, Sale,
    SaleReturn, Settlement, StatementLine, Tag, Transfer, UserId,
};

/// Who made a change. The name is a snapshot: history must keep reading the same after the user is
/// renamed or removed. API keys will join as another variant ([ADR 0007]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User { id: UserId, name: String },
}

/// The kind of record that changed. One variant per audited table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEntity {
    Party,
    Product,
    CostCenter,
    Category,
    BankAccount,
    PaymentMethod,
    PaymentTerm,
    FinancialEntry,
    Settlement,
    Transfer,
    /// One per bank account: the audit id is the account's.
    AccountOpening,
    Recurrence,
    StatementLine,
    Attachment,
    Quote,
    Sale,
    SaleReturn,
}

impl AuditEntity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Party => "party",
            Self::Product => "product",
            Self::CostCenter => "cost_center",
            Self::Category => "category",
            Self::BankAccount => "bank_account",
            Self::PaymentMethod => "payment_method",
            Self::PaymentTerm => "payment_term",
            Self::FinancialEntry => "financial_entry",
            Self::Recurrence => "recurrence",
            Self::StatementLine => "statement_line",
            Self::Attachment => "attachment",
            Self::Quote => "quote",
            Self::Sale => "sale",
            Self::SaleReturn => "sale_return",
            Self::Settlement => "settlement",
            Self::Transfer => "transfer",
            Self::AccountOpening => "account_opening",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditAction {
    Created,
    Updated,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "created" => Some(Self::Created),
            "updated" => Some(Self::Updated),
            _ => None,
        }
    }
}

/// One field that changed. Values are text so the trail reads the same whatever the field's type;
/// `None` means "not set".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub field: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// What the store is asked to record together with a write. The store adds the sequence and time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub company_id: CompanyId,
    pub actor: Actor,
    pub entity: AuditEntity,
    pub entity_id: Uuid,
    pub action: AuditAction,
    pub changes: Vec<FieldChange>,
}

/// A record that has an audit trail: what kind it is, whose it is and which one.
pub trait Audited {
    fn audit_entity(&self) -> AuditEntity;
    fn audit_company(&self) -> CompanyId;
    fn audit_id(&self) -> Uuid;
}

impl Audited for Party {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Party
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for Product {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Product
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for Category {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Category
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for BankAccount {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::BankAccount
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for PaymentMethod {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::PaymentMethod
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for PaymentTerm {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::PaymentTerm
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for FinancialEntry {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::FinancialEntry
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for Recurrence {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Recurrence
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for StatementLine {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::StatementLine
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for Settlement {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Settlement
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for Quote {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Quote
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for Sale {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Sale
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for SaleReturn {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::SaleReturn
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for Attachment {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Attachment
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for Transfer {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::Transfer
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl Audited for AccountOpening {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::AccountOpening
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.account_id().as_uuid()
    }
}

impl Audited for CostCenter {
    fn audit_entity(&self) -> AuditEntity {
        AuditEntity::CostCenter
    }

    fn audit_company(&self) -> CompanyId {
        self.company_id()
    }

    fn audit_id(&self) -> Uuid {
        self.id().as_uuid()
    }
}

impl AuditEntry {
    /// Whether this entry is about that record, in that record's company. Stores check it before
    /// writing, so a caller's mistake cannot put a row into another company's trail.
    pub fn describes(&self, record: &impl Audited) -> bool {
        self.entity == record.audit_entity()
            && self.company_id == record.audit_company()
            && self.entity_id == record.audit_id()
    }
}

/// An entry as read back. `sequence` orders records of the whole trail, also within the same
/// millisecond; `at_millis` is milliseconds since the Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub sequence: i64,
    pub at_millis: i64,
    pub entry: AuditEntry,
}

fn set(field: &str, value: impl Into<String>) -> FieldChange {
    FieldChange {
        field: field.to_owned(),
        from: None,
        to: Some(value.into()),
    }
}

/// The fields of a record as text, in the order the trail lists them; `None` is "not set".
type Fields = Vec<(&'static str, Option<String>)>;

/// Every field that is set: what a creation records.
fn creation_from(fields: Fields) -> Vec<FieldChange> {
    fields
        .into_iter()
        .filter_map(|(field, value)| value.map(|value| set(field, value)))
        .collect()
}

/// The fields that differ between two versions of a record; empty when nothing changed, in which
/// case nothing is written and nothing is recorded.
fn changes_between(before: Fields, after: Fields) -> Vec<FieldChange> {
    before
        .into_iter()
        .zip(after)
        .filter(|((_, from), (_, to))| from != to)
        .map(|((field, from), (_, to))| FieldChange {
            field: field.to_owned(),
            from,
            to,
        })
        .collect()
}

fn party_fields(party: &Party) -> Fields {
    let address = party.address();
    let text = |value: Option<&str>| value.map(str::to_owned);
    vec![
        ("name", Some(party.name().to_owned())),
        ("document", text(party.document().map(|d| d.as_str()))),
        ("email", text(party.email().map(|e| e.as_str()))),
        ("is_customer", Some(party.is_customer().to_string())),
        ("is_supplier", Some(party.is_supplier().to_string())),
        ("is_carrier", Some(party.is_carrier().to_string())),
        ("is_seller", Some(party.is_seller().to_string())),
        ("postal_code", text(address.map(|a| a.postal_code()))),
        ("street", text(address.map(|a| a.street()))),
        ("street_number", text(address.and_then(|a| a.number()))),
        ("complement", text(address.and_then(|a| a.complement()))),
        ("district", text(address.and_then(|a| a.district()))),
        ("city", text(address.map(|a| a.city()))),
        ("state", text(address.map(|a| a.state()))),
        ("tags", tags_text(party)),
    ]
}

/// The tags on one line, which is unambiguous because a tag cannot hold a comma.
fn tags_text(party: &Party) -> Option<String> {
    let tags: Vec<&str> = party.tags().iter().map(Tag::as_str).collect();
    (!tags.is_empty()).then(|| tags.join(", "))
}

/// The contacts as text for a person to read, one after the other. Whether two lists of contacts
/// are the same is decided on the contacts themselves, never on this text: a name can hold the
/// separator, so two different lists could read alike.
fn contacts_text(party: &Party) -> Option<String> {
    let lines: Vec<String> = party
        .contacts()
        .iter()
        .map(|contact| {
            let mut line = contact.name().to_owned();
            if let Some(job_title) = contact.job_title() {
                line.push_str(&format!(" ({job_title})"));
            }
            if let Some(email) = contact.email() {
                line.push_str(&format!(" <{}>", email.as_str()));
            }
            if let Some(phone) = contact.phone() {
                line.push_str(&format!(" {phone}"));
            }
            line
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("; "))
}

/// Every field a new party has set, and what the registry said about its CNPJ.
pub fn creation_changes(party: &Party) -> Vec<FieldChange> {
    let mut changes = creation_from(party_fields(party));
    changes.extend(contacts_text(party).map(|contacts| set("contacts", contacts)));
    changes.extend(document_check_changes(
        &DocumentCheck::NotApplicable,
        party.document_check(),
    ));
    changes
}

/// What differs between two versions of a party: its fields, its contacts, and the registry's
/// answer when the document changed. Empty when nothing did.
pub fn party_changes(before: &Party, after: &Party) -> Vec<FieldChange> {
    let mut changes = changes_between(party_fields(before), party_fields(after));
    if before.contacts() != after.contacts() {
        changes.push(FieldChange {
            field: "contacts".to_owned(),
            from: contacts_text(before),
            to: contacts_text(after),
        });
    }
    changes.extend(document_check_changes(
        before.document_check(),
        after.document_check(),
    ));
    changes
}

fn product_fields(product: &Product) -> Fields {
    let amount = |amount: Option<crate::Amount>| amount.map(|value| value.to_string());
    vec![
        ("kind", Some(product.kind().as_str().to_owned())),
        ("name", Some(product.name().to_owned())),
        ("sku", product.sku().map(str::to_owned)),
        ("unit", Some(product.unit().to_owned())),
        ("ncm", product.ncm().map(str::to_owned)),
        ("sale_price", amount(product.sale_price())),
        ("cost_price", amount(product.cost_price())),
        ("minimum_stock", amount(product.minimum_stock())),
        ("active", Some(product.is_active().to_string())),
    ]
}

/// Every field a new product has set.
pub fn product_creation_changes(product: &Product) -> Vec<FieldChange> {
    creation_from(product_fields(product))
}

pub fn product_changes(before: &Product, after: &Product) -> Vec<FieldChange> {
    changes_between(product_fields(before), product_fields(after))
}

fn category_fields(category: &Category) -> Fields {
    vec![
        ("name", Some(category.name().to_owned())),
        (
            "parent",
            category
                .parent_id()
                .map(|parent| parent.as_uuid().to_string()),
        ),
        ("dre_group", Some(category.dre_group().as_str().to_owned())),
        (
            "cash_flow_group",
            Some(category.cash_flow_group().as_str().to_owned()),
        ),
        ("active", Some(category.is_active().to_string())),
    ]
}

/// Every field a new category has set.
pub fn category_creation_changes(category: &Category) -> Vec<FieldChange> {
    creation_from(category_fields(category))
}

pub fn category_changes(before: &Category, after: &Category) -> Vec<FieldChange> {
    changes_between(category_fields(before), category_fields(after))
}

fn bank_account_fields(account: &BankAccount) -> Fields {
    vec![
        ("kind", Some(account.kind().as_str().to_owned())),
        ("name", Some(account.name().to_owned())),
        ("bank_code", account.bank_code().map(str::to_owned)),
        ("branch", account.branch().map(str::to_owned)),
        ("number", account.number().map(str::to_owned)),
        ("active", Some(account.is_active().to_string())),
    ]
}

/// Every field a new bank account has set.
pub fn bank_account_creation_changes(account: &BankAccount) -> Vec<FieldChange> {
    creation_from(bank_account_fields(account))
}

pub fn bank_account_changes(before: &BankAccount, after: &BankAccount) -> Vec<FieldChange> {
    changes_between(bank_account_fields(before), bank_account_fields(after))
}

fn payment_method_fields(method: &PaymentMethod) -> Fields {
    vec![
        ("kind", Some(method.kind().as_str().to_owned())),
        ("name", Some(method.name().to_owned())),
        ("active", Some(method.is_active().to_string())),
    ]
}

/// Every field a new payment method has set.
pub fn payment_method_creation_changes(method: &PaymentMethod) -> Vec<FieldChange> {
    creation_from(payment_method_fields(method))
}

pub fn payment_method_changes(before: &PaymentMethod, after: &PaymentMethod) -> Vec<FieldChange> {
    changes_between(payment_method_fields(before), payment_method_fields(after))
}

fn entry_fields(entry: &FinancialEntry) -> Fields {
    vec![
        ("kind", Some(entry.kind().as_str().to_owned())),
        ("description", Some(entry.description().to_owned())),
        (
            "party_id",
            entry.party_id().map(|id| id.as_uuid().to_string()),
        ),
        (
            "category_id",
            Some(entry.category_id().as_uuid().to_string()),
        ),
        (
            "cost_center_id",
            entry.cost_center_id().map(|id| id.as_uuid().to_string()),
        ),
        ("competence_date", Some(entry.competence_date().to_string())),
        ("installments", Some(entry.installments_text())),
        // Only a cancelled entry says so: the trail of the others reads as it always did.
        ("voided", entry.is_voided().then(|| "true".to_owned())),
    ]
}

/// Every field a new entry has set.
pub fn entry_creation_changes(entry: &FinancialEntry) -> Vec<FieldChange> {
    creation_from(entry_fields(entry))
}

pub fn entry_changes(before: &FinancialEntry, after: &FinancialEntry) -> Vec<FieldChange> {
    changes_between(entry_fields(before), entry_fields(after))
}

fn recurrence_fields(recurrence: &Recurrence) -> Fields {
    vec![
        ("kind", Some(recurrence.kind().as_str().to_owned())),
        ("description", Some(recurrence.description().to_owned())),
        (
            "party_id",
            recurrence.party_id().map(|id| id.as_uuid().to_string()),
        ),
        (
            "category_id",
            Some(recurrence.category_id().as_uuid().to_string()),
        ),
        (
            "cost_center_id",
            recurrence
                .cost_center_id()
                .map(|id| id.as_uuid().to_string()),
        ),
        ("amount", Some(recurrence.amount().to_string())),
        (
            "frequency",
            Some(recurrence.frequency().as_str().to_owned()),
        ),
        ("starts_on", Some(recurrence.starts_on().to_string())),
        ("ends_on", recurrence.ends_on().map(|day| day.to_string())),
        ("active", Some(recurrence.is_active().to_string())),
    ]
}

/// Every field a new recurrence has set.
pub fn recurrence_creation_changes(recurrence: &Recurrence) -> Vec<FieldChange> {
    creation_from(recurrence_fields(recurrence))
}

pub fn recurrence_changes(before: &Recurrence, after: &Recurrence) -> Vec<FieldChange> {
    changes_between(recurrence_fields(before), recurrence_fields(after))
}

fn quote_fields(quote: &Quote) -> Fields {
    vec![
        (
            "customer_id",
            Some(quote.customer_id().as_uuid().to_string()),
        ),
        ("issued_on", Some(quote.issued_on().to_string())),
        ("valid_until", Some(quote.valid_until().to_string())),
        (
            "payment_term_id",
            quote.payment_term_id().map(|id| id.as_uuid().to_string()),
        ),
        ("notes", quote.notes().map(str::to_owned)),
        ("lines", Some(lines_text(quote.lines()))),
        ("status", Some(quote.status().as_str().to_owned())),
    ]
}

/// Every field a new quote has set.
pub fn quote_creation_changes(quote: &Quote) -> Vec<FieldChange> {
    creation_from(quote_fields(quote))
}

pub fn quote_changes(before: &Quote, after: &Quote) -> Vec<FieldChange> {
    changes_between(quote_fields(before), quote_fields(after))
}

fn sale_fields(sale: &Sale) -> Fields {
    vec![
        (
            "customer_id",
            Some(sale.customer_id().as_uuid().to_string()),
        ),
        (
            "seller_id",
            sale.seller_id().map(|id| id.as_uuid().to_string()),
        ),
        ("sold_on", Some(sale.sold_on().to_string())),
        (
            "payment_term_id",
            sale.payment_term_id().map(|id| id.as_uuid().to_string()),
        ),
        (
            "category_id",
            Some(sale.category_id().as_uuid().to_string()),
        ),
        (
            "cost_center_id",
            sale.cost_center_id().map(|id| id.as_uuid().to_string()),
        ),
        ("notes", sale.notes().map(str::to_owned)),
        ("lines", Some(lines_text(sale.lines()))),
        ("status", Some(sale.status().as_str().to_owned())),
        (
            "quote_id",
            sale.quote_id().map(|id| id.as_uuid().to_string()),
        ),
    ]
}

/// Every field a new sale has set.
pub fn sale_creation_changes(sale: &Sale) -> Vec<FieldChange> {
    creation_from(sale_fields(sale))
}

pub fn sale_changes(before: &Sale, after: &Sale) -> Vec<FieldChange> {
    changes_between(sale_fields(before), sale_fields(after))
}

/// Every field a new return has set.
pub fn sale_return_creation_changes(sale_return: &SaleReturn) -> Vec<FieldChange> {
    creation_from(vec![
        ("sale_id", Some(sale_return.sale_id().as_uuid().to_string())),
        ("returned_on", Some(sale_return.returned_on().to_string())),
        ("reason", sale_return.reason().map(str::to_owned)),
        (
            "items",
            Some(
                sale_return
                    .items()
                    .iter()
                    .map(|(line, quantity)| format!("{line}:{quantity}"))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ),
        ("refund", Some(sale_return.refund().to_string())),
    ])
}

fn statement_line_fields(line: &StatementLine) -> Fields {
    let (settlement, transfer) = match line.matched() {
        Some(LineMatch::Settlement(id)) => (Some(id.as_uuid().to_string()), None),
        Some(LineMatch::Transfer(id)) => (None, Some(id.as_uuid().to_string())),
        None => (None, None),
    };
    vec![
        ("account_id", Some(line.account_id().as_uuid().to_string())),
        ("posted_on", Some(line.posted_on().to_string())),
        ("amount", Some(line.amount().to_string())),
        ("description", Some(line.description().to_owned())),
        ("bank_ref", line.bank_ref().map(str::to_owned)),
        ("status", Some(line.status().as_str().to_owned())),
        ("matched_settlement_id", settlement),
        ("matched_transfer_id", transfer),
    ]
}

/// Every field a new statement line has set.
pub fn statement_line_creation_changes(line: &StatementLine) -> Vec<FieldChange> {
    creation_from(statement_line_fields(line))
}

/// What a move of the status changes: the status and what it is matched to.
pub fn statement_line_changes(before: &StatementLine, after: &StatementLine) -> Vec<FieldChange> {
    changes_between(statement_line_fields(before), statement_line_fields(after))
}

fn settlement_fields(settlement: &Settlement) -> Fields {
    vec![
        (
            "entry_id",
            Some(settlement.entry_id().as_uuid().to_string()),
        ),
        ("installment", Some(settlement.installment().to_string())),
        ("settled_on", Some(settlement.settled_on().to_string())),
        ("applied", Some(settlement.applied().to_string())),
        ("interest", Some(settlement.interest().to_string())),
        ("fine", Some(settlement.fine().to_string())),
        ("discount", Some(settlement.discount().to_string())),
        (
            "bank_account_id",
            Some(settlement.bank_account_id().as_uuid().to_string()),
        ),
        (
            "payment_method_id",
            settlement
                .payment_method_id()
                .map(|id| id.as_uuid().to_string()),
        ),
        ("reversed", Some(settlement.is_reversed().to_string())),
    ]
}

/// Every field a new settlement has set.
pub fn settlement_creation_changes(settlement: &Settlement) -> Vec<FieldChange> {
    creation_from(settlement_fields(settlement))
}

/// What a reversal changes: only the `reversed` flag.
pub fn settlement_changes(before: &Settlement, after: &Settlement) -> Vec<FieldChange> {
    changes_between(settlement_fields(before), settlement_fields(after))
}

fn attachment_fields(attachment: &Attachment) -> Fields {
    vec![
        (
            "entry_id",
            Some(attachment.entry_id().as_uuid().to_string()),
        ),
        ("file_name", Some(attachment.file_name().to_owned())),
        ("content_type", Some(attachment.content_type().to_owned())),
        ("size", Some(attachment.size().to_string())),
        ("removed", Some(attachment.is_removed().to_string())),
    ]
}

/// Every field a new attachment has set.
pub fn attachment_creation_changes(attachment: &Attachment) -> Vec<FieldChange> {
    creation_from(attachment_fields(attachment))
}

/// What a removal changes: only the `removed` flag.
pub fn attachment_changes(before: &Attachment, after: &Attachment) -> Vec<FieldChange> {
    changes_between(attachment_fields(before), attachment_fields(after))
}

fn transfer_fields(transfer: &Transfer) -> Fields {
    vec![
        ("from", Some(transfer.from().as_uuid().to_string())),
        ("to", Some(transfer.to().as_uuid().to_string())),
        ("amount", Some(transfer.amount().to_string())),
        ("occurred_on", Some(transfer.occurred_on().to_string())),
        ("note", transfer.note().map(str::to_owned)),
        ("reversed", Some(transfer.is_reversed().to_string())),
    ]
}

/// Every field a new transfer has set.
pub fn transfer_creation_changes(transfer: &Transfer) -> Vec<FieldChange> {
    creation_from(transfer_fields(transfer))
}

/// What a reversal changes: only the `reversed` flag.
pub fn transfer_changes(before: &Transfer, after: &Transfer) -> Vec<FieldChange> {
    changes_between(transfer_fields(before), transfer_fields(after))
}

fn opening_fields(opening: &AccountOpening) -> Fields {
    vec![
        ("balance", Some(opening.balance().to_string())),
        ("on", Some(opening.on().to_string())),
    ]
}

/// Every field a first opening balance has set.
pub fn opening_creation_changes(opening: &AccountOpening) -> Vec<FieldChange> {
    creation_from(opening_fields(opening))
}

pub fn opening_changes(before: &AccountOpening, after: &AccountOpening) -> Vec<FieldChange> {
    changes_between(opening_fields(before), opening_fields(after))
}

fn payment_term_fields(term: &PaymentTerm) -> Fields {
    vec![
        ("name", Some(term.name().to_owned())),
        ("installments", Some(term.installments_text())),
        ("active", Some(term.is_active().to_string())),
    ]
}

/// Every field a new payment term has set.
pub fn payment_term_creation_changes(term: &PaymentTerm) -> Vec<FieldChange> {
    creation_from(payment_term_fields(term))
}

pub fn payment_term_changes(before: &PaymentTerm, after: &PaymentTerm) -> Vec<FieldChange> {
    changes_between(payment_term_fields(before), payment_term_fields(after))
}

fn cost_center_fields(cost_center: &CostCenter) -> Fields {
    vec![
        ("name", Some(cost_center.name().to_owned())),
        ("active", Some(cost_center.is_active().to_string())),
    ]
}

/// Every field a new cost center has set.
pub fn cost_center_creation_changes(cost_center: &CostCenter) -> Vec<FieldChange> {
    creation_from(cost_center_fields(cost_center))
}

pub fn cost_center_changes(before: &CostCenter, after: &CostCenter) -> Vec<FieldChange> {
    changes_between(cost_center_fields(before), cost_center_fields(after))
}

/// The fields of the registry check that differ between two answers; empty when nothing changed.
pub fn document_check_changes(before: &DocumentCheck, after: &DocumentCheck) -> Vec<FieldChange> {
    let fields = |check: &DocumentCheck| -> [(&'static str, Option<String>); 3] {
        let (kind, legal_name, situation) = match check {
            DocumentCheck::NotApplicable => (None, None, None),
            DocumentCheck::NotFound => (Some("not_found"), None, None),
            DocumentCheck::Unverified => (Some("unverified"), None, None),
            DocumentCheck::Verified {
                legal_name,
                situation,
            } => (
                Some("verified"),
                Some(legal_name.clone()),
                Some(situation.clone()),
            ),
        };
        [
            ("document_check", kind.map(str::to_owned)),
            ("registry_legal_name", legal_name),
            ("registry_situation", situation),
        ]
    };
    fields(before)
        .into_iter()
        .zip(fields(after))
        .filter(|((_, from), (_, to))| from != to)
        .map(|((field, from), (_, to))| FieldChange {
            field: field.to_owned(),
            from,
            to,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AddressDraft, CompanyId, ContactDraft, PartyDraft, PartyId, ProductDraft, ProductId,
        ProductKind,
    };

    fn party(document: Option<&str>) -> Party {
        Party::new(
            PartyId::generate(),
            CompanyId::generate(),
            PartyDraft {
                name: "Padaria Central".into(),
                document: document.map(Into::into),
                email: Some("contato@padaria.example".into()),
                is_customer: true,
                ..PartyDraft::default()
            },
        )
        .unwrap()
    }

    fn fields(changes: &[FieldChange]) -> Vec<&str> {
        changes.iter().map(|change| change.field.as_str()).collect()
    }

    #[test]
    fn a_new_party_records_only_the_fields_it_has() {
        let bare = party(None);
        let changes = creation_changes(&bare);
        assert_eq!(
            fields(&changes),
            [
                "name",
                "email",
                "is_customer",
                "is_supplier",
                "is_carrier",
                "is_seller"
            ]
        );
        assert!(changes.iter().all(|change| change.from.is_none()));

        let with_document = party(Some("529.982.247-25"));
        assert!(fields(&creation_changes(&with_document)).contains(&"document"));
    }

    #[test]
    fn a_new_party_records_what_the_registry_said() {
        let checked =
            party(Some("11.222.333/0001-81")).with_document_check(DocumentCheck::Verified {
                legal_name: "PADARIA CENTRAL LTDA".into(),
                situation: "ATIVA".into(),
            });
        let changes = creation_changes(&checked);
        assert!(changes.contains(&set("document_check", "verified")));
        assert!(changes.contains(&set("registry_situation", "ATIVA")));
    }

    #[test]
    fn a_party_edit_lists_only_what_changed_and_an_equal_one_nothing() {
        let before = party(Some("529.982.247-25"));
        assert!(party_changes(&before, &before.clone()).is_empty());

        let after = before
            .replaced_with(PartyDraft {
                name: "Padaria do Centro".into(),
                document: Some("529.982.247-25".into()),
                email: None,
                is_customer: true,
                is_seller: true,
                address: Some(AddressDraft {
                    postal_code: "01310-100".into(),
                    street: "Avenida Paulista".into(),
                    city: "São Paulo".into(),
                    state: "SP".into(),
                    ..AddressDraft::default()
                }),
                ..PartyDraft::default()
            })
            .unwrap();
        let changes = party_changes(&before, &after);
        assert_eq!(
            fields(&changes),
            [
                "name",
                "email",
                "is_seller",
                "postal_code",
                "street",
                "city",
                "state"
            ]
        );
        assert_eq!(changes[1].from.as_deref(), Some("contato@padaria.example"));
        assert_eq!(changes[1].to, None);
        assert_eq!(changes[2].to.as_deref(), Some("true"));
    }

    fn contact(name: &str) -> ContactDraft {
        ContactDraft {
            name: name.into(),
            ..ContactDraft::default()
        }
    }

    #[test]
    fn tags_and_contacts_are_recorded_as_a_line_each_and_only_when_they_change() {
        let before = party(None);
        let after = before
            .replaced_with(PartyDraft {
                name: "Padaria Central".into(),
                email: Some("contato@padaria.example".into()),
                is_customer: true,
                tags: vec!["VIP".into(), "atacado".into()],
                contacts: vec![
                    ContactDraft {
                        job_title: Some("Compras".into()),
                        email: Some("ana@padaria.example".into()),
                        phone: Some("(81) 99999-0000".into()),
                        ..contact("Ana")
                    },
                    contact("Bruno"),
                ],
                ..PartyDraft::default()
            })
            .unwrap();
        let changes = party_changes(&before, &after);
        assert_eq!(fields(&changes), ["tags", "contacts"]);
        assert_eq!(changes[0].from, None);
        assert_eq!(changes[0].to.as_deref(), Some("atacado, vip"));
        assert_eq!(
            changes[1].to.as_deref(),
            Some("Ana (Compras) <ana@padaria.example> (81) 99999-0000; Bruno")
        );
        assert!(
            creation_changes(&after)
                .iter()
                .any(|change| change.field == "contacts")
        );

        // The same tags typed another way, and the same contacts, change nothing.
        let same = after
            .replaced_with(PartyDraft {
                tags: vec!["vip".into(), " ATACADO".into()],
                contacts: vec![
                    ContactDraft {
                        job_title: Some("Compras".into()),
                        email: Some("ANA@padaria.example".into()),
                        phone: Some("(81) 99999-0000".into()),
                        ..contact("Ana")
                    },
                    contact("Bruno"),
                ],
                ..PartyDraft {
                    name: "Padaria Central".into(),
                    email: Some("contato@padaria.example".into()),
                    is_customer: true,
                    ..PartyDraft::default()
                }
            })
            .unwrap();
        assert!(party_changes(&after, &same).is_empty());

        // The order of the contacts is part of what is stored.
        let reordered = after
            .replaced_with(PartyDraft {
                contacts: vec![contact("Bruno"), contact("Ana")],
                tags: vec!["atacado".into(), "vip".into()],
                name: "Padaria Central".into(),
                email: Some("contato@padaria.example".into()),
                is_customer: true,
                ..PartyDraft::default()
            })
            .unwrap();
        assert_eq!(fields(&party_changes(&after, &reordered)), ["contacts"]);
    }

    #[test]
    fn two_lists_of_contacts_that_read_alike_are_still_a_change() {
        // "Ana; Bruno" as one name reads like the two contacts "Ana" and "Bruno".
        let draft = |contacts: Vec<ContactDraft>| PartyDraft {
            name: "Padaria Central".into(),
            is_customer: true,
            contacts,
            ..PartyDraft::default()
        };
        let before = party(None)
            .replaced_with(draft(vec![contact("Ana"), contact("Bruno")]))
            .unwrap();
        let after = before
            .replaced_with(draft(vec![contact("Ana; Bruno")]))
            .unwrap();
        let changes = party_changes(&before, &after);
        assert_eq!(fields(&changes), ["contacts"]);
        assert_eq!(changes[0].from, changes[0].to);
    }

    #[test]
    fn a_recheck_lists_only_what_changed() {
        let verified = DocumentCheck::Verified {
            legal_name: "PADARIA CENTRAL LTDA".into(),
            situation: "ATIVA".into(),
        };
        let closed = DocumentCheck::Verified {
            legal_name: "PADARIA CENTRAL LTDA".into(),
            situation: "BAIXADA".into(),
        };

        let changes = document_check_changes(&verified, &closed);
        assert_eq!(
            changes,
            [FieldChange {
                field: "registry_situation".into(),
                from: Some("ATIVA".into()),
                to: Some("BAIXADA".into()),
            }]
        );

        let from_unverified = document_check_changes(&DocumentCheck::Unverified, &verified);
        assert_eq!(
            fields(&from_unverified),
            [
                "document_check",
                "registry_legal_name",
                "registry_situation"
            ]
        );
        assert_eq!(from_unverified[0].from.as_deref(), Some("unverified"));
        assert_eq!(from_unverified[0].to.as_deref(), Some("verified"));
    }

    #[test]
    fn the_same_answer_changes_nothing() {
        assert!(
            document_check_changes(&DocumentCheck::Unverified, &DocumentCheck::Unverified)
                .is_empty()
        );
        assert!(
            document_check_changes(&DocumentCheck::NotFound, &DocumentCheck::NotFound).is_empty()
        );
    }

    fn product(sale_price: &str, active: bool) -> Product {
        Product::new(
            ProductId::generate(),
            CompanyId::generate(),
            ProductDraft {
                kind: ProductKind::Product,
                name: "Pão francês".into(),
                sku: None,
                unit: "KG".into(),
                ncm: None,
                sale_price: Some(sale_price.into()),
                cost_price: None,
                minimum_stock: None,
                active,
            },
        )
        .unwrap()
    }

    #[test]
    fn a_new_product_records_only_the_fields_it_has() {
        let changes = product_creation_changes(&product("12.50", true));
        assert_eq!(
            fields(&changes),
            ["kind", "name", "unit", "sale_price", "active"]
        );
        assert_eq!(changes[3].to.as_deref(), Some("12.5"));
        assert!(changes.iter().all(|change| change.from.is_none()));
    }

    #[test]
    fn a_product_edit_lists_only_what_changed_and_an_equal_one_nothing() {
        let before = product("12.50", true);
        let after = Product::new(
            before.id(),
            before.company_id(),
            ProductDraft {
                kind: ProductKind::Product,
                name: "Pão francês".into(),
                sku: None,
                unit: "KG".into(),
                ncm: None,
                sale_price: Some("15".into()),
                cost_price: None,
                minimum_stock: None,
                active: false,
            },
        )
        .unwrap();
        let changes = product_changes(&before, &after);
        assert_eq!(fields(&changes), ["sale_price", "active"]);
        assert_eq!(changes[0].from.as_deref(), Some("12.5"));
        assert_eq!(changes[0].to.as_deref(), Some("15"));

        // An unchanged product has nothing to record.
        assert!(product_changes(&before, &before.clone()).is_empty());
    }
}
