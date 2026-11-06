use crate::CompanyId;

/// How a company's stock behaves ([ADR 0055]): whether a sale that the stock cannot cover is refused. There is one
/// per company, replaced as a whole; a company that never said has the default, which allows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockSettings {
    company_id: CompanyId,
    block_sale_without_balance: bool,
}

impl StockSettings {
    pub fn new(company_id: CompanyId, block_sale_without_balance: bool) -> Self {
        Self {
            company_id,
            block_sale_without_balance,
        }
    }

    /// What a company has before it says anything: a sale may take the balance below zero.
    pub fn open(company_id: CompanyId) -> Self {
        Self::new(company_id, false)
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    /// Whether a sale of more than the balance is refused.
    pub fn blocks_sales_without_balance(&self) -> bool {
        self.block_sale_without_balance
    }
}
