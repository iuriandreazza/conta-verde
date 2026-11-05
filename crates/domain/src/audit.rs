use uuid::Uuid;

use crate::{
    BankAccount, Category, CompanyId, CostCenter, DocumentCheck, Party, PaymentMethod, Product,
    UserId,
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

/// Every field a new party has set.
pub fn creation_changes(party: &Party) -> Vec<FieldChange> {
    let mut changes = vec![set("name", party.name())];
    if let Some(document) = party.document() {
        changes.push(set("document", document.as_str()));
    }
    if let Some(email) = party.email() {
        changes.push(set("email", email.as_str()));
    }
    changes.push(set("is_customer", party.is_customer().to_string()));
    changes.push(set("is_supplier", party.is_supplier().to_string()));
    changes.extend(document_check_changes(
        &DocumentCheck::NotApplicable,
        party.document_check(),
    ));
    changes
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
    use crate::{CompanyId, PartyDraft, PartyId, ProductDraft, ProductId, ProductKind};

    fn party(document: Option<&str>) -> Party {
        Party::new(
            PartyId::generate(),
            CompanyId::generate(),
            PartyDraft {
                name: "Padaria Central".into(),
                document: document.map(Into::into),
                email: Some("contato@padaria.example".into()),
                is_customer: true,
                is_supplier: false,
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
            ["name", "email", "is_customer", "is_supplier"]
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
