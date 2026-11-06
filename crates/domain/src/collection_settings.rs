use crate::{BankAccountId, CategoryId, CompanyId};

/// How a company collects ([ADR 0046]): the bank account the money of a paid charge lands in, and the
/// category the provider's fee is entered under (without one the fee is only recorded on the charge).
/// There is one per company, replaced as a whole. Whether the account and the category are usable is the
/// use case's to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionSettings {
    company_id: CompanyId,
    bank_account_id: BankAccountId,
    fee_category_id: Option<CategoryId>,
}

impl CollectionSettings {
    pub fn new(
        company_id: CompanyId,
        bank_account_id: BankAccountId,
        fee_category_id: Option<CategoryId>,
    ) -> Self {
        Self {
            company_id,
            bank_account_id,
            fee_category_id,
        }
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn bank_account_id(&self) -> BankAccountId {
        self.bank_account_id
    }

    pub fn fee_category_id(&self) -> Option<CategoryId> {
        self.fee_category_id
    }
}
