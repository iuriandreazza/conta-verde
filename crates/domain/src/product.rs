use crate::name::clean_name;
use crate::{Amount, CompanyId, DomainError, ProductId};

const MAX_SKU_LEN: usize = 60;
// The width of the unit of measure in an NF-e item (`uCom`).
const MAX_UNIT_LEN: usize = 6;
const NCM_LEN: usize = 8;

/// A good that is bought, stocked and sold, or a service that is only sold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductKind {
    Product,
    Service,
}

impl ProductKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Service => "service",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "product" => Some(Self::Product),
            "service" => Some(Self::Service),
            _ => None,
        }
    }
}

/// Unvalidated input for registering or changing a product or service. Prices and the minimum stock
/// are decimal text, never numbers, so no precision is lost on the way in ([`Amount`]).
#[derive(Debug, Clone)]
pub struct ProductDraft {
    pub kind: ProductKind,
    pub name: String,
    pub sku: Option<String>,
    pub unit: String,
    pub ncm: Option<String>,
    pub sale_price: Option<String>,
    pub cost_price: Option<String>,
    pub minimum_stock: Option<String>,
    pub active: bool,
}

/// A product or service owned by a single company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    id: ProductId,
    company_id: CompanyId,
    kind: ProductKind,
    name: String,
    sku: Option<String>,
    unit: String,
    ncm: Option<String>,
    sale_price: Option<Amount>,
    cost_price: Option<Amount>,
    minimum_stock: Option<Amount>,
    active: bool,
}

impl Product {
    pub fn new(
        id: ProductId,
        company_id: CompanyId,
        draft: ProductDraft,
    ) -> Result<Self, DomainError> {
        let name = clean_name(&draft.name)?;
        let sku = draft
            .sku
            .as_deref()
            .and_then(non_blank)
            .map(clean_sku)
            .transpose()?;
        let unit = clean_unit(&draft.unit)?;
        let ncm = draft
            .ncm
            .as_deref()
            .and_then(non_blank)
            .map(clean_ncm)
            .transpose()?;
        let amount = |field: &'static str, raw: &Option<String>| {
            raw.as_deref()
                .and_then(non_blank)
                .map(|text| Amount::parse(text).ok_or(DomainError::InvalidAmount(field)))
                .transpose()
        };
        let sale_price = amount("sale price", &draft.sale_price)?;
        let cost_price = amount("cost price", &draft.cost_price)?;
        let minimum_stock = amount("minimum stock", &draft.minimum_stock)?;

        // A service has nothing to stock and no tariff classification of goods; refusing is clearer
        // than storing a value that would never be used.
        if draft.kind == ProductKind::Service {
            if ncm.is_some() {
                return Err(DomainError::ServiceCannotHaveNcm);
            }
            if minimum_stock.is_some() {
                return Err(DomainError::ServiceCannotHaveMinimumStock);
            }
        }

        Ok(Self {
            id,
            company_id,
            kind: draft.kind,
            name,
            sku,
            unit,
            ncm,
            sale_price,
            cost_price,
            minimum_stock,
            active: draft.active,
        })
    }

    /// The same product with every editable field replaced by the draft's, validated as for a new
    /// one. Whether it is a product or a service is fixed when it is registered: stock and fiscal
    /// data hang from it.
    pub fn replaced_with(&self, draft: ProductDraft) -> Result<Self, DomainError> {
        if draft.kind != self.kind {
            return Err(DomainError::KindCannotChange);
        }
        Self::new(self.id, self.company_id, draft)
    }

    pub fn id(&self) -> ProductId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn kind(&self) -> ProductKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sku(&self) -> Option<&str> {
        self.sku.as_deref()
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub fn ncm(&self) -> Option<&str> {
        self.ncm.as_deref()
    }

    pub fn sale_price(&self) -> Option<Amount> {
        self.sale_price
    }

    pub fn cost_price(&self) -> Option<Amount> {
        self.cost_price
    }

    pub fn minimum_stock(&self) -> Option<Amount> {
        self.minimum_stock
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

fn non_blank(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// Upper case, so `abc-1` and `ABC-1` are one SKU; letters, digits and `. _ / -` only.
fn clean_sku(raw: &str) -> Result<String, DomainError> {
    let sku = raw.to_ascii_uppercase();
    let allowed = |c: char| c.is_ascii_uppercase() || c.is_ascii_digit() || ".-_/".contains(c);
    if sku.len() > MAX_SKU_LEN || !sku.chars().all(allowed) {
        return Err(DomainError::InvalidSku);
    }
    Ok(sku)
}

fn clean_unit(raw: &str) -> Result<String, DomainError> {
    let unit = raw.trim().to_ascii_uppercase();
    let valid = !unit.is_empty()
        && unit.len() <= MAX_UNIT_LEN
        && unit
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if valid {
        Ok(unit)
    } else {
        Err(DomainError::InvalidUnit)
    }
}

/// Eight digits, written with or without the usual dots (`2106.90.10`). Only the format is checked:
/// whether the code exists is a table that changes and belongs to the fiscal phase.
fn clean_ncm(raw: &str) -> Result<String, DomainError> {
    let ncm: String = raw
        .chars()
        .filter(|c| *c != '.' && !c.is_whitespace())
        .collect();
    if ncm.len() == NCM_LEN && ncm.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(ncm)
    } else {
        Err(DomainError::InvalidNcm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> ProductDraft {
        ProductDraft {
            kind: ProductKind::Product,
            name: "  Pão francês ".into(),
            sku: Some(" pao-001 ".into()),
            unit: " kg ".into(),
            ncm: Some("1905.90.90".into()),
            sale_price: Some("12.50".into()),
            cost_price: Some("7".into()),
            minimum_stock: Some("10.000".into()),
            active: true,
        }
    }

    fn build(draft: ProductDraft) -> Result<Product, DomainError> {
        Product::new(ProductId::generate(), CompanyId::generate(), draft)
    }

    #[test]
    fn normalizes_what_it_keeps() {
        let product = build(draft()).unwrap();
        assert_eq!(product.name(), "Pão francês");
        assert_eq!(product.sku(), Some("PAO-001"));
        assert_eq!(product.unit(), "KG");
        assert_eq!(product.ncm(), Some("19059090"));
        assert_eq!(product.sale_price().unwrap().to_string(), "12.5");
        assert_eq!(product.minimum_stock().unwrap().to_string(), "10");
    }

    #[test]
    fn blank_optional_fields_are_not_set() {
        let product = build(ProductDraft {
            sku: Some("  ".into()),
            ncm: Some("".into()),
            sale_price: Some(" ".into()),
            cost_price: None,
            minimum_stock: Some("".into()),
            ..draft()
        })
        .unwrap();
        assert_eq!(product.sku(), None);
        assert_eq!(product.ncm(), None);
        assert_eq!(product.sale_price(), None);
        assert_eq!(product.minimum_stock(), None);
    }

    #[test]
    fn rejects_a_blank_name_a_nul_and_an_overlong_one() {
        let named = |name: &str| {
            build(ProductDraft {
                name: name.into(),
                ..draft()
            })
        };
        assert_eq!(named("  ").unwrap_err(), DomainError::EmptyName);
        assert_eq!(named("Pão\0").unwrap_err(), DomainError::InvalidName);
        // A tab or a line break is odd but storable, and parties with one must stay readable.
        assert!(named("Pão\tfrancês").is_ok());
        assert!(named(&"ã".repeat(200)).is_ok());
        assert_eq!(
            named(&"ã".repeat(201)).unwrap_err(),
            DomainError::InvalidName
        );
    }

    #[test]
    fn rejects_skus_and_units_that_would_not_survive_a_document() {
        let with_sku = |sku: &str| {
            build(ProductDraft {
                sku: Some(sku.into()),
                ..draft()
            })
        };
        assert_eq!(with_sku("a b").unwrap_err(), DomainError::InvalidSku);
        assert_eq!(with_sku("ç").unwrap_err(), DomainError::InvalidSku);
        assert_eq!(
            with_sku(&"A".repeat(61)).unwrap_err(),
            DomainError::InvalidSku
        );
        let with_unit = |unit: &str| {
            build(ProductDraft {
                unit: unit.into(),
                ..draft()
            })
        };
        assert_eq!(with_unit("").unwrap_err(), DomainError::InvalidUnit);
        assert_eq!(
            with_unit("quilogramas").unwrap_err(),
            DomainError::InvalidUnit
        );
        assert_eq!(with_unit("k.g").unwrap_err(), DomainError::InvalidUnit);
    }

    #[test]
    fn checks_only_the_shape_of_an_ncm() {
        let with_ncm = |ncm: &str| {
            build(ProductDraft {
                ncm: Some(ncm.into()),
                ..draft()
            })
        };
        assert!(with_ncm("19059090").is_ok());
        assert_eq!(with_ncm("1905909").unwrap_err(), DomainError::InvalidNcm);
        assert_eq!(with_ncm("1905909A").unwrap_err(), DomainError::InvalidNcm);
        assert_eq!(with_ncm("190590900").unwrap_err(), DomainError::InvalidNcm);
    }

    #[test]
    fn names_the_amount_that_is_wrong() {
        let bad = ProductDraft {
            sale_price: Some("10,50".into()),
            ..draft()
        };
        assert_eq!(
            build(bad).unwrap_err(),
            DomainError::InvalidAmount("sale price")
        );
        let bad = ProductDraft {
            cost_price: Some("-1".into()),
            ..draft()
        };
        assert_eq!(
            build(bad).unwrap_err(),
            DomainError::InvalidAmount("cost price")
        );
        let bad = ProductDraft {
            minimum_stock: Some("1e2".into()),
            ..draft()
        };
        assert_eq!(
            build(bad).unwrap_err(),
            DomainError::InvalidAmount("minimum stock")
        );
    }

    #[test]
    fn a_service_has_no_ncm_and_no_minimum_stock() {
        let service = |ncm: Option<&str>, minimum_stock: Option<&str>| {
            build(ProductDraft {
                kind: ProductKind::Service,
                ncm: ncm.map(Into::into),
                minimum_stock: minimum_stock.map(Into::into),
                ..draft()
            })
        };
        assert_eq!(
            service(Some("19059090"), None).unwrap_err(),
            DomainError::ServiceCannotHaveNcm
        );
        assert_eq!(
            service(None, Some("1")).unwrap_err(),
            DomainError::ServiceCannotHaveMinimumStock
        );
        assert!(service(None, None).is_ok());
    }

    #[test]
    fn replacing_keeps_the_identity_and_refuses_a_change_of_kind() {
        let product = build(draft()).unwrap();
        let replaced = product
            .replaced_with(ProductDraft {
                name: "Pão de sal".into(),
                active: false,
                ..draft()
            })
            .unwrap();
        assert_eq!(replaced.id(), product.id());
        assert_eq!(replaced.company_id(), product.company_id());
        assert_eq!(replaced.name(), "Pão de sal");
        assert!(!replaced.is_active());

        let as_service = ProductDraft {
            kind: ProductKind::Service,
            ncm: None,
            minimum_stock: None,
            ..draft()
        };
        assert_eq!(
            product.replaced_with(as_service).unwrap_err(),
            DomainError::KindCannotChange
        );
    }
}
