use crate::name::clean_name;
use crate::{Amount, CompanyId, DomainError, PriceListId, ProductId};

/// How many products a price list can name: a bound that keeps one list a page a person can read.
pub const MAX_PRICE_LIST_ITEMS: usize = 1000;

/// One price as it is asked for: unvalidated, the price as decimal text.
#[derive(Debug, Clone)]
pub struct PriceListItemDraft {
    pub product_id: ProductId,
    pub price: String,
}

/// Unvalidated input for registering or changing a price list.
#[derive(Debug, Clone)]
pub struct PriceListDraft {
    pub name: String,
    pub items: Vec<PriceListItemDraft>,
    pub active: bool,
}

/// What one product sells for in a list: a unit price with at most six decimals, like the price of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceListItem {
    product_id: ProductId,
    price: Amount,
}

impl PriceListItem {
    pub fn product_id(&self) -> ProductId {
        self.product_id
    }

    pub fn price(&self) -> Amount {
        self.price
    }
}

/// A set of unit prices that replaces the product's own sale price for some customers or some kind of sale
/// ([ADR 0042]): wholesale, retail, a partner. A list never holds a document's price: choosing one fills the
/// price of a line, and the line keeps what it was filled with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceList {
    id: PriceListId,
    company_id: CompanyId,
    name: String,
    items: Vec<PriceListItem>,
    active: bool,
}

impl PriceList {
    pub fn new(
        id: PriceListId,
        company_id: CompanyId,
        draft: PriceListDraft,
    ) -> Result<Self, DomainError> {
        let name = clean_name(&draft.name)?;
        let items = validated_items(&draft.items)?;
        Ok(Self {
            id,
            company_id,
            name,
            items,
            active: draft.active,
        })
    }

    /// The same price list with every editable field replaced by the draft's.
    pub fn replaced_with(&self, draft: PriceListDraft) -> Result<Self, DomainError> {
        Self::new(self.id, self.company_id, draft)
    }

    pub fn id(&self) -> PriceListId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn items(&self) -> &[PriceListItem] {
        &self.items
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The products and prices in one canonical text, `product:price` one per row, which is what the audit trail
    /// records: the same prices always read the same, however they were written.
    pub fn items_text(&self) -> String {
        self.items
            .iter()
            .map(|item| format!("{}:{}", item.product_id.as_uuid(), item.price))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The rules that span the rows of a list, which no single-row constraint can state: at least one price, at most
/// [`MAX_PRICE_LIST_ITEMS`], and a product at most once.
fn validated_items(drafts: &[PriceListItemDraft]) -> Result<Vec<PriceListItem>, DomainError> {
    if drafts.is_empty() {
        return Err(DomainError::NoPriceListItems);
    }
    if drafts.len() > MAX_PRICE_LIST_ITEMS {
        return Err(DomainError::TooManyPriceListItems);
    }
    let mut items: Vec<PriceListItem> = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let price = Amount::parse(draft.price.trim()).ok_or(DomainError::InvalidAmount("price"))?;
        if items.iter().any(|item| item.product_id == draft.product_id) {
            return Err(DomainError::DuplicatePriceListProduct);
        }
        items.push(PriceListItem {
            product_id: draft.product_id,
            price,
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(name: &str, prices: &[(ProductId, &str)]) -> PriceListDraft {
        PriceListDraft {
            name: name.into(),
            items: prices
                .iter()
                .map(|(product_id, price)| PriceListItemDraft {
                    product_id: *product_id,
                    price: (*price).into(),
                })
                .collect(),
            active: true,
        }
    }

    fn build(prices: &[(ProductId, &str)]) -> Result<PriceList, DomainError> {
        PriceList::new(
            PriceListId::generate(),
            CompanyId::generate(),
            draft("  Atacado ", prices),
        )
    }

    #[test]
    fn keeps_the_prices_in_one_canonical_text() {
        let (coffee, bread) = (ProductId::generate(), ProductId::generate());
        let list = build(&[(coffee, "10.50"), (bread, " 3 ")]).unwrap();
        assert_eq!(list.name(), "Atacado");
        assert_eq!(list.items().len(), 2);
        assert_eq!(list.items()[0].price().to_string(), "10.5");
        assert_eq!(
            list.items_text(),
            format!("{}:10.5\n{}:3", coffee.as_uuid(), bread.as_uuid())
        );
        // A price may be free and may have six decimals.
        assert!(build(&[(coffee, "0")]).is_ok());
        assert!(build(&[(coffee, "0.000001")]).is_ok());
    }

    #[test]
    fn a_list_needs_prices_that_are_numbers_for_products_named_once() {
        let coffee = ProductId::generate();
        assert_eq!(build(&[]).unwrap_err(), DomainError::NoPriceListItems);
        for bad in ["", "-1", "abc", "1,5", "0.0000001", "1e3"] {
            assert_eq!(
                build(&[(coffee, bad)]).unwrap_err(),
                DomainError::InvalidAmount("price"),
                "{bad:?}"
            );
        }
        assert_eq!(
            build(&[(coffee, "1"), (coffee, "2")]).unwrap_err(),
            DomainError::DuplicatePriceListProduct
        );
        let many: Vec<_> = (0..=MAX_PRICE_LIST_ITEMS)
            .map(|_| (ProductId::generate(), "1"))
            .collect();
        assert_eq!(
            build(&many).unwrap_err(),
            DomainError::TooManyPriceListItems
        );
        assert!(build(&many[..MAX_PRICE_LIST_ITEMS]).is_ok());
    }

    #[test]
    fn the_name_is_required() {
        let coffee = ProductId::generate();
        let draft = draft("   ", &[(coffee, "1")]);
        assert_eq!(
            PriceList::new(PriceListId::generate(), CompanyId::generate(), draft).unwrap_err(),
            DomainError::EmptyName
        );
    }
}
