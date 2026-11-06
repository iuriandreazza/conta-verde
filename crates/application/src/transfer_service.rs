use conta_verde_domain::{
    AccountOpening, Actor, AuditAction, BankAccount, BankAccountId, CalendarDate, CompanyId,
    Decimal, SignedAmount, Transfer, TransferDraft, TransferId, opening_changes,
    opening_creation_changes, transfer_changes, transfer_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, BankAccountQuery, BankAccountRepository, DateCursor, EntryReferences,
    FinancialEntryRepository, NameCursor, OpeningRepository, RepositoryError, TransferQuery,
    TransferRepository,
};

#[derive(Debug, Clone)]
pub struct TransferPage {
    pub items: Vec<Transfer>,
    /// Present when more transfers exist after this page.
    pub next: Option<DateCursor>,
}

/// Money moved between two accounts of the company ([ADR 0031]).
pub struct TransferService<R, F> {
    repository: R,
    references: F,
}

impl<R: TransferRepository, F: EntryReferences> TransferService<R, F> {
    pub fn new(repository: R, references: F) -> Self {
        Self {
            repository,
            references,
        }
    }

    /// Validates the draft and that both accounts are usable in the company (they exist there and are
    /// active), and stores the transfer; the creation is recorded in the audit trail in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: TransferDraft,
    ) -> Result<Transfer, ApplicationError> {
        let transfer = Transfer::new(TransferId::generate(), company_id, draft)?;
        for (account, field) in [(transfer.from(), "from"), (transfer.to(), "to")] {
            if !self
                .references
                .bank_account_is_usable(company_id, account)
                .await?
            {
                return Err(ApplicationError::UnusableReference(field));
            }
        }
        let audit = audit_entry(
            &transfer,
            actor,
            AuditAction::Created,
            transfer_creation_changes(&transfer),
        );
        self.repository.save(&transfer, &audit).await?;
        Ok(transfer)
    }

    /// Undoes a transfer: it is marked reversed, stops counting, and stays in the trail. One that is
    /// not there, or was reversed already, is not found.
    pub async fn reverse(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: TransferId,
    ) -> Result<Transfer, ApplicationError> {
        let stored = self
            .repository
            .find(company_id, id)
            .await?
            .filter(|transfer| !transfer.is_reversed())
            .ok_or(ApplicationError::TransferNotFound)?;
        let reversed = stored.clone().with_reversed(true);
        let audit = audit_entry(
            &reversed,
            actor,
            AuditAction::Updated,
            transfer_changes(&stored, &reversed),
        );
        self.repository
            .reverse(&reversed, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::TransferNotFound,
                other => other.into(),
            })?;
        Ok(reversed)
    }

    /// The transfers a bank statement line may be, see `transfer_candidates` of the port.
    pub async fn candidates(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        incoming: bool,
        amount: Decimal,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<Transfer>, ApplicationError> {
        Ok(self
            .repository
            .transfer_candidates(company_id, account, incoming, amount, since, until)
            .await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &TransferQuery,
        after: Option<&DateCursor>,
        limit: Option<usize>,
    ) -> Result<TransferPage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(TransferPage { items, next })
    }
}

/// What an account held on a day, and how: its opening balance, what its settlements and transfers
/// moved, and the balance that comes of them ([ADR 0031]).
#[derive(Debug, Clone)]
pub struct AccountBalance {
    pub account: BankAccount,
    pub opening: Option<AccountOpening>,
    /// What settled receivables brought in.
    pub received: Decimal,
    /// What settled payables took out.
    pub paid: Decimal,
    pub transfers_in: Decimal,
    pub transfers_out: Decimal,
    pub balance: Decimal,
}

/// The balance of every account on a day, and of all of them together.
#[derive(Debug, Clone)]
pub struct Balances {
    pub accounts: Vec<AccountBalance>,
    pub total: Decimal,
}

/// The most accounts a company is read with to add its balances up; a company has a handful.
const ACCOUNT_PAGE: usize = 200;

/// Balances per account and the opening balance they start from ([ADR 0031]). It composes the answer
/// from the ports of the accounts, of the entries (which own the settlements), of the transfers and of
/// the opening balances, so every adapter gives the same answer.
pub struct BalanceService<A, E, T, O> {
    pub(crate) accounts: A,
    pub(crate) entries: E,
    pub(crate) transfers: T,
    pub(crate) openings: O,
}

impl<A, E, T, O> BalanceService<A, E, T, O>
where
    A: BankAccountRepository,
    E: FinancialEntryRepository,
    T: TransferRepository,
    O: OpeningRepository,
{
    pub fn new(accounts: A, entries: E, transfers: T, openings: O) -> Self {
        Self {
            accounts,
            entries,
            transfers,
            openings,
        }
    }

    /// Sets or replaces the opening balance of an account of the company, which need not be active.
    /// The same balance and day again writes nothing and records nothing.
    pub async fn set_opening(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        account: BankAccountId,
        balance: &str,
        on: &str,
    ) -> Result<AccountOpening, ApplicationError> {
        let opening = AccountOpening::new(company_id, account, balance, on)?;
        if self.accounts.find(company_id, account).await?.is_none() {
            return Err(ApplicationError::BankAccountNotFound);
        }
        let audit = match self.openings.find(company_id, account).await? {
            None => audit_entry(
                &opening,
                actor,
                AuditAction::Created,
                opening_creation_changes(&opening),
            ),
            Some(before) => {
                let changes = opening_changes(&before, &opening);
                if changes.is_empty() {
                    return Ok(opening);
                }
                audit_entry(&opening, actor, AuditAction::Updated, changes)
            }
        };
        self.openings.set(&opening, &audit).await?;
        Ok(opening)
    }

    /// The balance of every account of the company on `on`, and the total. On a day before an
    /// account's opening day its balance is 0: its history starts at the opening day.
    pub async fn balances(
        &self,
        company_id: CompanyId,
        on: CalendarDate,
    ) -> Result<Balances, ApplicationError> {
        let openings = self.openings.list_by_company(company_id).await?;
        let accounts = self.accounts_of(company_id).await?;
        let mut balances = Vec::with_capacity(accounts.len());
        let mut total = Decimal::ZERO;
        for account in accounts {
            let opening = openings
                .iter()
                .find(|opening| opening.account_id() == account.id())
                .cloned();
            let item = self.balance_of(company_id, account, opening, on).await?;
            total += item.balance;
            balances.push(item);
        }
        Ok(Balances {
            accounts: balances,
            total: total.normalize(),
        })
    }

    /// Every account of the company, inactive ones included, by name.
    pub(crate) async fn accounts_of(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<BankAccount>, ApplicationError> {
        let mut accounts = Vec::new();
        let mut after: Option<NameCursor> = None;
        loop {
            let page = self
                .accounts
                .list_by_company(
                    company_id,
                    &BankAccountQuery::default(),
                    after.as_ref(),
                    ACCOUNT_PAGE + 1,
                )
                .await?;
            let more = page.len() > ACCOUNT_PAGE;
            let page = &page[..page.len().min(ACCOUNT_PAGE)];
            accounts.extend_from_slice(page);
            match (more, page.last()) {
                (true, Some(last)) => after = Some(NameCursor::from(last)),
                _ => return Ok(accounts),
            }
        }
    }

    pub(crate) async fn balance_of(
        &self,
        company_id: CompanyId,
        account: BankAccount,
        opening: Option<AccountOpening>,
        on: CalendarDate,
    ) -> Result<AccountBalance, ApplicationError> {
        let since = opening.as_ref().map(AccountOpening::on);
        if since.is_some_and(|day| on < day) {
            return Ok(AccountBalance {
                account,
                opening,
                received: Decimal::ZERO,
                paid: Decimal::ZERO,
                transfers_in: Decimal::ZERO,
                transfers_out: Decimal::ZERO,
                balance: Decimal::ZERO,
            });
        }
        let settled = self
            .entries
            .cash_of_account(company_id, account.id(), since, on)
            .await?;
        let moved = self
            .transfers
            .movement_of_account(company_id, account.id(), since, on)
            .await?;
        let start = opening
            .as_ref()
            .map_or(SignedAmount::ZERO, AccountOpening::balance)
            .as_decimal();
        let balance =
            (start + settled.received - settled.paid + moved.received - moved.paid).normalize();
        Ok(AccountBalance {
            account,
            opening,
            received: settled.received.normalize(),
            paid: settled.paid.normalize(),
            transfers_in: moved.received.normalize(),
            transfers_out: moved.paid.normalize(),
            balance,
        })
    }
}
