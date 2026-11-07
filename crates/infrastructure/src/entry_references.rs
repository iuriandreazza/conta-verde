use std::collections::HashMap;

use conta_verde_application::{
    BankAccountQuery, BankAccountRepository, CategoryRepository, CostCenterQuery,
    CostCenterRepository, EntryLookups, EntryReferences, ImportNames, NameCursor, PartyQuery,
    PartyRepository, PaymentMethodRepository, RepositoryError,
};
use conta_verde_domain::{
    BankAccountId, CategoryId, CompanyId, CostCenterId, DreGroup, PartyId, PaymentMethodId,
};

/// How many rows of a register are read at a time when a whole register is needed.
const PAGE: usize = 200;

/// Answers what an entry and its settlements may point at by asking the registers themselves, so one
/// implementation serves every adapter: Postgres, and the in-memory ones that tests share between the
/// services. A record of another company is not found, so it answers like one that does not exist.
pub struct RegisterReferences<C, K, P, B, M> {
    categories: C,
    cost_centers: K,
    parties: P,
    bank_accounts: B,
    payment_methods: M,
}

impl<C, K, P, B, M> RegisterReferences<C, K, P, B, M> {
    pub fn new(
        categories: C,
        cost_centers: K,
        parties: P,
        bank_accounts: B,
        payment_methods: M,
    ) -> Self {
        Self {
            categories,
            cost_centers,
            parties,
            bank_accounts,
            payment_methods,
        }
    }
}

impl<C, K, P, B, M> EntryReferences for RegisterReferences<C, K, P, B, M>
where
    C: CategoryRepository,
    K: CostCenterRepository,
    P: PartyRepository,
    B: BankAccountRepository,
    M: PaymentMethodRepository,
{
    async fn category_is_usable(
        &self,
        company_id: CompanyId,
        id: CategoryId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .categories
            .find(company_id, id)
            .await?
            .is_some_and(|category| {
                category.is_active() && category.dre_group() != DreGroup::Transfers
            }))
    }

    async fn cost_center_is_usable(
        &self,
        company_id: CompanyId,
        id: CostCenterId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .cost_centers
            .find(company_id, id)
            .await?
            .is_some_and(|cost_center| cost_center.is_active()))
    }

    async fn party_exists(
        &self,
        company_id: CompanyId,
        id: PartyId,
    ) -> Result<bool, RepositoryError> {
        Ok(self.parties.find(company_id, id).await?.is_some())
    }

    async fn bank_account_is_usable(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .bank_accounts
            .find(company_id, id)
            .await?
            .is_some_and(|account| account.is_active()))
    }

    async fn payment_method_is_usable(
        &self,
        company_id: CompanyId,
        id: PaymentMethodId,
    ) -> Result<bool, RepositoryError> {
        Ok(self
            .payment_methods
            .find(company_id, id)
            .await?
            .is_some_and(|method| method.is_active()))
    }
}

impl<C, K, P, B, M> EntryLookups for RegisterReferences<C, K, P, B, M>
where
    C: CategoryRepository,
    K: CostCenterRepository,
    P: PartyRepository,
    B: BankAccountRepository,
    M: PaymentMethodRepository,
{
    async fn import_names(&self, company_id: CompanyId) -> Result<ImportNames, RepositoryError> {
        let mut names = ImportNames::default();

        let categories = self
            .categories
            .list_by_company(company_id, Some(true))
            .await?;
        let parent_names: HashMap<CategoryId, String> = categories
            .iter()
            .map(|category| (category.id(), category.name().to_owned()))
            .collect();
        for category in &categories {
            let path = match category.parent_id() {
                None => category.name().to_owned(),
                // A child of an inactive parent cannot be active, so the parent is always here.
                Some(parent) => format!("{} > {}", parent_names[&parent], category.name()),
            };
            names.add_category(&path, category.id(), category.dre_group());
        }

        let mut after = None;
        loop {
            let page = self
                .cost_centers
                .list_by_company(
                    company_id,
                    &CostCenterQuery::default().with_active(true),
                    after.as_ref(),
                    PAGE,
                )
                .await?;
            for cost_center in &page {
                names.add_cost_center(cost_center.name(), cost_center.id());
            }
            match page.last() {
                Some(last) if page.len() == PAGE => {
                    after = Some(NameCursor {
                        name: last.name().to_owned(),
                        id: last.id().as_uuid(),
                    });
                }
                _ => break,
            }
        }

        let mut after = None;
        loop {
            let page = self
                .bank_accounts
                .list_by_company(
                    company_id,
                    &BankAccountQuery::default().with_active(true),
                    after.as_ref(),
                    PAGE,
                )
                .await?;
            for account in &page {
                names.add_bank_account(account.name(), account.id());
            }
            match page.last() {
                Some(last) if page.len() == PAGE => {
                    after = Some(NameCursor {
                        name: last.name().to_owned(),
                        id: last.id().as_uuid(),
                    });
                }
                _ => break,
            }
        }

        let mut after = None;
        loop {
            let page = self
                .parties
                .list_by_company(company_id, &PartyQuery::default(), after.as_ref(), PAGE)
                .await?;
            for party in &page {
                names.add_party(
                    party.name(),
                    party.document().map(|document| document.as_str()),
                    party.id(),
                    party.is_customer(),
                    party.is_supplier(),
                );
            }
            match page.last() {
                Some(last) if page.len() == PAGE => {
                    after = Some(NameCursor {
                        name: last.name().to_owned(),
                        id: last.id().as_uuid(),
                    });
                }
                _ => break,
            }
        }
        Ok(names)
    }
}
