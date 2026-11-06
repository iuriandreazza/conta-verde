use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CalendarDate, CompanyId, Contract, ContractCycle,
    ContractDraft, ContractId, ContractStatus, Sale, SaleId, contract_changes,
    contract_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, ContractQuery, ContractRepository, EntryReferences, FinancialEntryRepository,
    NameCursor, QuoteRepository, RepositoryError, SaleRepository, SaleService, SalesReferences,
};

/// The most cycles one pass bills for one contract: a contract that starts years back catches up over a few.
const MAX_PER_PASS: usize = 24;
/// How many contracts the job reads at a time.
const JOB_BATCH: usize = 200;

/// What a pass of the scheduled job did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BillingReport {
    /// Cycles billed, counting a cycle that was only finished.
    pub billed: usize,
    /// Contracts that could not be billed and will be tried again.
    pub failed: usize,
}

#[derive(Debug, Clone)]
pub struct ContractPage {
    pub items: Vec<Contract>,
    /// Present when more contracts exist after this page.
    pub next: Option<NameCursor>,
}

/// Contracts that bill themselves every cycle ([ADR 0040]): each cycle is a completed sale and its receivable,
/// made by [`SaleService`].
pub struct ContractService<C, S, Q, E, F, R> {
    repository: C,
    sales: SaleService<S, Q, E, F, R>,
}

impl<C, S, Q, E, F, R> ContractService<C, S, Q, E, F, R>
where
    C: ContractRepository,
    S: SaleRepository,
    Q: QuoteRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
    R: SalesReferences,
{
    pub fn new(repository: C, sales: SaleService<S, Q, E, F, R>) -> Self {
        Self { repository, sales }
    }

    /// Validates the draft and what it points at, stores the contract with its creation in the audit trail,
    /// and bills what has already fallen due.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: ContractDraft,
        today: CalendarDate,
    ) -> Result<Contract, ApplicationError> {
        let contract = Contract::new(ContractId::generate(), company_id, draft, actor.clone())?;
        self.check_references(&contract, None).await?;
        let audit = audit_entry(
            &contract,
            actor,
            AuditAction::Created,
            contract_creation_changes(&contract),
        );
        self.repository.save(&contract, &audit).await?;
        self.bill_one(contract, today).await
    }

    /// Replaces the editable fields of a contract that is not terminated. What was billed is never touched,
    /// and one brought back after a pause goes on from today.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ContractId,
        draft: ContractDraft,
        today: CalendarDate,
    ) -> Result<Contract, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let mut after = before.replaced_with(draft, actor.clone())?;
        if before.status() == ContractStatus::Paused
            && after.status() == ContractStatus::Active
            && let Some(yesterday) = today.pred()
        {
            after = after.resumed_from(yesterday);
        }
        let changes = contract_changes(&before, &after);
        if changes.is_empty() && after.billed_through() == before.billed_through() {
            return Ok(before);
        }
        self.check_references(&after, Some(&before)).await?;
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.write(&after, &audit).await?;
        self.bill_one(after, today).await
    }

    /// Raises (or lowers) the prices of the lines by a percentage, from the next cycle that is billed.
    pub async fn adjust(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ContractId,
        percent: &str,
    ) -> Result<Contract, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.adjusted_by(percent, actor.clone())?;
        let changes = contract_changes(&before, &after);
        if changes.is_empty() {
            return Ok(before);
        }
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.write(&after, &audit).await?;
        Ok(after)
    }

    /// Ends the contract on a day: nothing is billed after it, and it never changes again.
    pub async fn terminate(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: ContractId,
        on: &str,
    ) -> Result<Contract, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.terminated_on(on, actor.clone())?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            contract_changes(&before, &after),
        );
        self.write(&after, &audit).await?;
        Ok(after)
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> Result<Contract, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the contract, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's contract answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &ContractQuery,
        after: Option<&NameCursor>,
        limit: Option<usize>,
    ) -> Result<ContractPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(ContractPage { items, next })
    }

    /// The scheduled job: bills every cycle that has fallen due by `today`, for every active contract of
    /// every company. A contract that cannot be billed (a product was deactivated since) is left as it was and
    /// tried again on the next pass; the others go on. The day is the caller's, so a test can move the clock.
    pub async fn bill_all_due(
        &self,
        today: CalendarDate,
    ) -> Result<BillingReport, ApplicationError> {
        let mut report = BillingReport::default();
        // In batches by id, so a contract that keeps failing cannot fill the batch every hour and keep the
        // others from being looked at.
        let mut after = None;
        loop {
            let due = self
                .repository
                .list_to_bill(today, after, JOB_BATCH)
                .await?;
            let Some(last) = due.last().map(Contract::id) else {
                return Ok(report);
            };
            for contract in &due {
                match self.bill(contract, today).await {
                    Ok(count) => report.billed += count,
                    Err(_) => report.failed += 1,
                }
            }
            if due.len() < JOB_BATCH {
                return Ok(report);
            }
            after = Some(last);
        }
    }

    /// Bills the cycles between where the contract was billed through and `today`, each as a completed sale.
    /// A cycle that already has its sale is only finished, which is what makes a pass safe to repeat and to
    /// run in two places at once.
    async fn bill(
        &self,
        contract: &Contract,
        today: CalendarDate,
    ) -> Result<usize, ApplicationError> {
        if contract.status() != ContractStatus::Active {
            return Ok(0);
        }
        let days = contract.cycles_after(contract.billed_through(), today, MAX_PER_PASS);
        for day in &days {
            self.sales
                .bill_cycle(
                    contract.company_id(),
                    contract.saved_by(),
                    contract.sale_draft(*day),
                    ContractCycle {
                        contract_id: contract.id(),
                        cycle_on: *day,
                    },
                )
                .await?;
        }
        // A full pass was made when the days ran out before the cap: everything up to today, or up to the
        // last day of the contract, is done. Otherwise it goes on from the last day.
        let reached = if days.len() == MAX_PER_PASS {
            days.last().copied()
        } else {
            Some(contract.ends_on().map_or(today, |end| end.min(today)))
        };
        if let Some(through) = reached {
            self.repository
                .mark_billed(contract.company_id(), contract.id(), through)
                .await?;
        }
        Ok(days.len())
    }

    /// Bills after a write that is already stored. A failure is not the person's to see: the contract is
    /// saved, and the hourly pass tries again and counts it as failed.
    async fn bill_one(
        &self,
        contract: Contract,
        today: CalendarDate,
    ) -> Result<Contract, ApplicationError> {
        let _ = self.bill(&contract, today).await;
        self.find(contract.company_id(), contract.id()).await
    }

    async fn write(
        &self,
        contract: &Contract,
        audit: &conta_verde_domain::AuditEntry,
    ) -> Result<(), ApplicationError> {
        self.repository
            .update(contract, audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::ContractNotFound,
                RepositoryError::ContractChanged => ApplicationError::ContractChanged,
                other => other.into(),
            })
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: ContractId,
    ) -> Result<Contract, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::ContractNotFound)
    }

    /// What the contract's sales will point at must be usable now; one that is kept as it was is not asked
    /// again, so a contract can still be paused after a product was deactivated.
    async fn check_references(
        &self,
        contract: &Contract,
        before: Option<&Contract>,
    ) -> Result<(), ApplicationError> {
        let sale_of = |contract: &Contract| -> Result<Sale, ApplicationError> {
            Ok(Sale::new(
                SaleId::generate(),
                contract.company_id(),
                contract.sale_draft(contract.starts_on()),
            )?)
        };
        let after = sale_of(contract)?;
        let before = before.map(sale_of).transpose()?;
        self.sales.check_references(&after, before.as_ref()).await
    }
}
