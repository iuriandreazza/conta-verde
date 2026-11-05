use crate::name::clean_name;
use crate::{CategoryId, CompanyId, DomainError};

/// How many levels a tree may have, the roots being the first.
pub const MAX_CATEGORY_LEVELS: u8 = 5;

/// Where a category lands in the managerial income statement (ADR 0012). The keys are stored, in a
/// CHECK and in the audit trail, so they never change; the labels live in the web app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreGroup {
    GrossRevenue,
    Deductions,
    Costs,
    OperatingExpenses,
    FinancialResult,
    Investments,
    /// Cash that moves without being a result: loans, owner capital.
    OutsideResult,
    /// Moves between the company's own accounts; excluded from every result.
    Transfers,
}

impl DreGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GrossRevenue => "gross_revenue",
            Self::Deductions => "deductions",
            Self::Costs => "costs",
            Self::OperatingExpenses => "operating_expenses",
            Self::FinancialResult => "financial_result",
            Self::Investments => "investments",
            Self::OutsideResult => "outside_result",
            Self::Transfers => "transfers",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::GrossRevenue,
            Self::Deductions,
            Self::Costs,
            Self::OperatingExpenses,
            Self::FinancialResult,
            Self::Investments,
            Self::OutsideResult,
            Self::Transfers,
        ]
        .into_iter()
        .find(|group| group.as_str() == raw)
    }
}

/// Where a category lands in the cash-flow statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashFlowGroup {
    Operating,
    Investing,
    Financing,
    /// Moves between the company's own accounts; goes with [`DreGroup::Transfers`].
    Transfer,
}

impl CashFlowGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Operating => "operating",
            Self::Investing => "investing",
            Self::Financing => "financing",
            Self::Transfer => "transfer",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::Operating,
            Self::Investing,
            Self::Financing,
            Self::Transfer,
        ]
        .into_iter()
        .find(|group| group.as_str() == raw)
    }
}

/// A node of a company's tree of categories. Only a root chooses the groups; a subcategory
/// inherits both, and they are kept on every row so that a report reads a column instead of
/// walking the tree. The parent and the groups are fixed once the category exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    id: CategoryId,
    company_id: CompanyId,
    parent_id: Option<CategoryId>,
    name: String,
    dre_group: DreGroup,
    cash_flow_group: CashFlowGroup,
    level: u8,
    active: bool,
}

/// The category that holds moves between the company's own accounts is the root whose DRE group is
/// `Transfers`: it is found by that group, never by its name, which can be edited.
fn is_transfer(dre_group: DreGroup, cash_flow_group: CashFlowGroup) -> Result<bool, DomainError> {
    match (
        dre_group == DreGroup::Transfers,
        cash_flow_group == CashFlowGroup::Transfer,
    ) {
        (true, true) => Ok(true),
        (false, false) => Ok(false),
        _ => Err(DomainError::TransferGroupsGoTogether),
    }
}

impl Category {
    /// A top-level category, which chooses the groups.
    pub fn new_root(
        id: CategoryId,
        company_id: CompanyId,
        name: &str,
        dre_group: DreGroup,
        cash_flow_group: CashFlowGroup,
    ) -> Result<Self, DomainError> {
        is_transfer(dre_group, cash_flow_group)?;
        Ok(Self {
            id,
            company_id,
            parent_id: None,
            name: clean_name(name)?,
            dre_group,
            cash_flow_group,
            level: 1,
            active: true,
        })
    }

    /// A subcategory of `parent`, inheriting its groups. Whether the parent is active is the
    /// use case's to check, since it is a matter of state, not of shape.
    pub fn new_child(id: CategoryId, parent: &Category, name: &str) -> Result<Self, DomainError> {
        if parent.is_transfer() {
            return Err(DomainError::TransferCategoryHasNoChildren);
        }
        if parent.level >= MAX_CATEGORY_LEVELS {
            return Err(DomainError::CategoryTooDeep);
        }
        Ok(Self {
            id,
            company_id: parent.company_id,
            parent_id: Some(parent.id),
            name: clean_name(name)?,
            dre_group: parent.dre_group,
            cash_flow_group: parent.cash_flow_group,
            level: parent.level + 1,
            active: true,
        })
    }

    /// A stored category, as it is read back. The same shape rules as above, so that a row the
    /// database should have refused is noticed instead of used.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: CategoryId,
        company_id: CompanyId,
        parent_id: Option<CategoryId>,
        name: &str,
        dre_group: DreGroup,
        cash_flow_group: CashFlowGroup,
        level: u8,
        active: bool,
    ) -> Result<Self, DomainError> {
        is_transfer(dre_group, cash_flow_group)?;
        if level == 0 || level > MAX_CATEGORY_LEVELS || (level == 1) != parent_id.is_none() {
            return Err(DomainError::CategoryTooDeep);
        }
        Ok(Self {
            id,
            company_id,
            parent_id,
            name: clean_name(name)?,
            dre_group,
            cash_flow_group,
            level,
            active,
        })
    }

    /// The same category with a new name and active flag; everything else is fixed. The transfer
    /// category can be renamed but never deactivated, since moves between accounts need it.
    pub fn changed(&self, name: &str, active: bool) -> Result<Self, DomainError> {
        if self.is_transfer() && !active {
            return Err(DomainError::TransferCategoryStaysActive);
        }
        Ok(Self {
            name: clean_name(name)?,
            active,
            ..self.clone()
        })
    }

    pub fn id(&self) -> CategoryId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn parent_id(&self) -> Option<CategoryId> {
        self.parent_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dre_group(&self) -> DreGroup {
        self.dre_group
    }

    pub fn cash_flow_group(&self) -> CashFlowGroup {
        self.cash_flow_group
    }

    /// 1 for a root.
    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Whether this is the company's category for moves between its own accounts.
    pub fn is_transfer(&self) -> bool {
        self.parent_id.is_none() && self.dre_group == DreGroup::Transfers
    }
}

/// The usual chart of accounts of a small business, as a tree: every parent comes before its
/// children, so it can be stored in this order. A company applies it on request, when it has none.
pub fn default_template(company_id: CompanyId) -> Vec<Category> {
    use CashFlowGroup::{Financing, Investing, Operating, Transfer};
    use DreGroup::{
        Costs, Deductions, FinancialResult, GrossRevenue, Investments, OperatingExpenses,
        OutsideResult, Transfers,
    };
    let tree: [(&str, DreGroup, CashFlowGroup, &[&str]); 9] = [
        (
            "Receitas de vendas",
            GrossRevenue,
            Operating,
            &["Venda de produtos", "Prestação de serviços"],
        ),
        ("Outras receitas", GrossRevenue, Operating, &[]),
        (
            "Deduções",
            Deductions,
            Operating,
            &["Impostos sobre vendas", "Devoluções e cancelamentos"],
        ),
        (
            "Custos",
            Costs,
            Operating,
            &[
                "Custo das mercadorias vendidas",
                "Custo dos serviços prestados",
            ],
        ),
        (
            "Despesas operacionais",
            OperatingExpenses,
            Operating,
            &[
                "Pessoal",
                "Aluguel e condomínio",
                "Água, luz e internet",
                "Marketing",
                "Administrativas",
            ],
        ),
        (
            "Resultado financeiro",
            FinancialResult,
            Financing,
            &["Receitas financeiras", "Juros e tarifas bancárias"],
        ),
        (
            "Investimentos",
            Investments,
            Investing,
            &["Equipamentos", "Reformas"],
        ),
        (
            "Empréstimos e capital",
            OutsideResult,
            Financing,
            &[
                "Empréstimos obtidos",
                "Aporte dos sócios",
                "Retirada dos sócios",
            ],
        ),
        ("Transferências entre contas", Transfers, Transfer, &[]),
    ];

    let mut categories = Vec::new();
    for (name, dre_group, cash_flow_group, children) in tree {
        let root = Category::new_root(
            CategoryId::generate(),
            company_id,
            name,
            dre_group,
            cash_flow_group,
        )
        .expect("the template is valid");
        for child in children {
            let child = Category::new_child(CategoryId::generate(), &root, child)
                .expect("the template is valid");
            categories.push(child);
        }
        categories.insert(categories.len() - children.len(), root);
    }
    categories
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str, dre: DreGroup, cash: CashFlowGroup) -> Result<Category, DomainError> {
        Category::new_root(
            CategoryId::generate(),
            CompanyId::generate(),
            name,
            dre,
            cash,
        )
    }

    fn operating_root() -> Category {
        root(
            "Despesas",
            DreGroup::OperatingExpenses,
            CashFlowGroup::Operating,
        )
        .unwrap()
    }

    #[test]
    fn a_child_inherits_the_groups_and_sits_one_level_down() {
        let parent = root("Receitas", DreGroup::GrossRevenue, CashFlowGroup::Operating).unwrap();
        let child = Category::new_child(CategoryId::generate(), &parent, " Vendas ").unwrap();
        assert_eq!(child.name(), "Vendas");
        assert_eq!(child.parent_id(), Some(parent.id()));
        assert_eq!(child.company_id(), parent.company_id());
        assert_eq!(child.dre_group(), DreGroup::GrossRevenue);
        assert_eq!(child.cash_flow_group(), CashFlowGroup::Operating);
        assert_eq!((parent.level(), child.level()), (1, 2));
        assert!(child.is_active());
    }

    #[test]
    fn the_tree_has_five_levels() {
        let mut node = operating_root();
        for _ in 1..MAX_CATEGORY_LEVELS {
            node = Category::new_child(CategoryId::generate(), &node, "Filho").unwrap();
        }
        assert_eq!(node.level(), MAX_CATEGORY_LEVELS);
        assert_eq!(
            Category::new_child(CategoryId::generate(), &node, "Neto").unwrap_err(),
            DomainError::CategoryTooDeep
        );
    }

    #[test]
    fn the_transfer_groups_go_together_and_the_category_stands_alone() {
        assert_eq!(
            root("X", DreGroup::Transfers, CashFlowGroup::Operating).unwrap_err(),
            DomainError::TransferGroupsGoTogether
        );
        assert_eq!(
            root("X", DreGroup::Costs, CashFlowGroup::Transfer).unwrap_err(),
            DomainError::TransferGroupsGoTogether
        );
        let transfers = root(
            "Transferências",
            DreGroup::Transfers,
            CashFlowGroup::Transfer,
        )
        .unwrap();
        assert!(transfers.is_transfer());
        assert_eq!(
            Category::new_child(CategoryId::generate(), &transfers, "Filho").unwrap_err(),
            DomainError::TransferCategoryHasNoChildren
        );
        // Renamed, it is still the transfer category; deactivated, it is not allowed.
        let renamed = transfers.changed("Entre contas", true).unwrap();
        assert!(renamed.is_transfer());
        assert_eq!(
            transfers.changed("Transferências", false).unwrap_err(),
            DomainError::TransferCategoryStaysActive
        );
    }

    #[test]
    fn changing_keeps_the_place_in_the_tree() {
        let parent = operating_root();
        let child = Category::new_child(CategoryId::generate(), &parent, "Aluguel").unwrap();
        let changed = child.changed("  Aluguel da loja ", false).unwrap();
        assert_eq!(changed.name(), "Aluguel da loja");
        assert!(!changed.is_active());
        assert_eq!(changed.id(), child.id());
        assert_eq!(changed.parent_id(), child.parent_id());
        assert_eq!(changed.dre_group(), child.dre_group());
        assert_eq!(changed.level(), child.level());
        assert_eq!(
            child.changed(" ", true).unwrap_err(),
            DomainError::EmptyName
        );
    }

    #[test]
    fn a_stored_row_that_breaks_the_shape_is_noticed() {
        let id = CategoryId::generate();
        let company = CompanyId::generate();
        let restore = |parent: Option<CategoryId>, level| {
            Category::restore(
                id,
                company,
                parent,
                "X",
                DreGroup::Costs,
                CashFlowGroup::Operating,
                level,
                true,
            )
        };
        assert!(restore(None, 1).is_ok());
        assert!(restore(Some(CategoryId::generate()), 2).is_ok());
        assert!(restore(None, 2).is_err());
        assert!(restore(Some(CategoryId::generate()), 1).is_err());
        assert!(restore(None, 0).is_err());
        assert!(restore(Some(CategoryId::generate()), 6).is_err());
    }

    #[test]
    fn the_group_keys_are_stable_and_round_trip() {
        for key in [
            "gross_revenue",
            "deductions",
            "costs",
            "operating_expenses",
            "financial_result",
            "investments",
            "outside_result",
            "transfers",
        ] {
            assert_eq!(DreGroup::parse(key).unwrap().as_str(), key);
        }
        for key in ["operating", "investing", "financing", "transfer"] {
            assert_eq!(CashFlowGroup::parse(key).unwrap().as_str(), key);
        }
        assert_eq!(DreGroup::parse("Costs"), None);
        assert_eq!(CashFlowGroup::parse(""), None);
    }

    #[test]
    fn the_template_is_a_valid_tree_with_one_transfer_category() {
        let company = CompanyId::generate();
        let template = default_template(company);
        assert!(
            template
                .iter()
                .all(|category| category.company_id() == company)
        );
        // Every parent comes before its children, so it can be stored in this order.
        for (index, category) in template.iter().enumerate() {
            if let Some(parent) = category.parent_id() {
                let position = template
                    .iter()
                    .position(|other| other.id() == parent)
                    .unwrap();
                assert!(position < index, "{}", category.name());
            }
        }
        assert_eq!(
            template
                .iter()
                .filter(|category| category.is_transfer())
                .count(),
            1
        );
        assert_eq!(
            template
                .iter()
                .filter(|category| category.parent_id().is_none())
                .count(),
            9
        );
        // Siblings are unique, ignoring case, which the database insists on.
        let mut keys: Vec<(Option<CategoryId>, String)> = template
            .iter()
            .map(|category| (category.parent_id(), category.name().to_lowercase()))
            .collect();
        let total = keys.len();
        keys.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)))
        });
        keys.dedup();
        assert_eq!(keys.len(), total);
        // A DRE group is never left to the parent of a different one.
        for category in &template {
            if let Some(parent) = category
                .parent_id()
                .and_then(|id| template.iter().find(|c| c.id() == id))
            {
                assert_eq!(category.dre_group(), parent.dre_group());
            }
        }
    }
}
