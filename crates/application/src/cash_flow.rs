use conta_verde_domain::{AccountOpening, BankAccountId, CalendarDate, CompanyId, Decimal};

use crate::{
    ApplicationError, BalanceService, BankAccountRepository, FinancialEntryRepository,
    OpeningRepository, TransferRepository,
};

/// The most periods one answer may have, so a span stays small enough to read per account ([ADR 0032]).
const MAX_DAYS: usize = 400;
const MAX_MONTHS: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Day,
    Month,
}

/// What a cash flow is asked for: a span of days cut into periods, over every account or one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CashFlowQuery {
    from: CalendarDate,
    to: CalendarDate,
    granularity: Granularity,
    account: Option<BankAccountId>,
}

impl CashFlowQuery {
    /// `from` is not after `to`, and the span fits the periods an answer may have.
    pub fn new(
        from: CalendarDate,
        to: CalendarDate,
        granularity: Granularity,
        account: Option<BankAccountId>,
    ) -> Result<Self, ApplicationError> {
        if from > to {
            return Err(ApplicationError::InvalidSpan("from must not be after to"));
        }
        let query = Self {
            from,
            to,
            granularity,
            account,
        };
        let limit = match granularity {
            Granularity::Day => MAX_DAYS,
            Granularity::Month => MAX_MONTHS,
        };
        if query.cut().nth(limit).is_some() {
            return Err(ApplicationError::InvalidSpan(match granularity {
                Granularity::Day => "the span may cut into at most 400 days",
                Granularity::Month => "the span may cut into at most 60 months",
            }));
        }
        Ok(query)
    }

    pub fn from(&self) -> CalendarDate {
        self.from
    }

    pub fn to(&self) -> CalendarDate {
        self.to
    }

    pub fn granularity(&self) -> Granularity {
        self.granularity
    }

    pub fn account(&self) -> Option<BankAccountId> {
        self.account
    }

    /// The periods of the span, oldest first, as first and last day: a day each, or the calendar
    /// months clipped to the span.
    fn cut(&self) -> impl Iterator<Item = (CalendarDate, CalendarDate)> + '_ {
        let mut next = Some(self.from);
        std::iter::from_fn(move || {
            let start = next?;
            let end = match self.granularity {
                Granularity::Day => start,
                Granularity::Month => start.last_of_month().min(self.to),
            };
            next = end.succ().filter(|day| *day <= self.to);
            Some((start, end))
        })
    }
}

/// What happened in one period of the span, and what is still to happen in it ([ADR 0032]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashFlowPeriod {
    pub start: CalendarDate,
    pub end: CalendarDate,
    pub received: Decimal,
    pub paid: Decimal,
    pub transfers_in: Decimal,
    pub transfers_out: Decimal,
    /// `received - paid + transfers_in - transfers_out`.
    pub net: Decimal,
    /// The opening balances whose day falls in the period.
    pub new_opening_balances: Decimal,
    pub closing_balance: Decimal,
    /// What is open on the installments due in the period. `None` when one account is asked for:
    /// an installment has no account.
    pub to_receive: Option<Decimal>,
    pub to_pay: Option<Decimal>,
    /// The closing balance plus what is open and due up to the end of the period.
    pub projected_balance: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PastDue {
    pub receivable: Decimal,
    pub payable: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashFlow {
    /// The balance at the start of the first day.
    pub opening_balance: Decimal,
    /// What is still open on installments that fell due **before** the span, of receivables and of
    /// payables: late, and still to be settled, so the forecast counts it from the first period.
    /// `None` when one account is asked for, or the span starts on the first day there is.
    pub past_due: Option<PastDue>,
    pub periods: Vec<CashFlowPeriod>,
}

/// The period a day falls in: the periods cover the span, and every day asked for is inside it.
fn period_of(periods: &mut [CashFlowPeriod], day: CalendarDate) -> &mut CashFlowPeriod {
    let index = periods.partition_point(|period| period.end < day);
    &mut periods[index]
}

impl<A, E, T, O> BalanceService<A, E, T, O>
where
    A: BankAccountRepository,
    E: FinancialEntryRepository,
    T: TransferRepository,
    O: OpeningRepository,
{
    /// The cash flow of the company, or of one of its accounts, over the span of the query.
    pub async fn cash_flow(
        &self,
        company_id: CompanyId,
        query: &CashFlowQuery,
    ) -> Result<CashFlow, ApplicationError> {
        let mut accounts = self.accounts_of(company_id).await?;
        if let Some(wanted) = query.account {
            accounts.retain(|account| account.id() == wanted);
            if accounts.is_empty() {
                return Err(ApplicationError::BankAccountNotFound);
            }
        }
        let openings = self.openings.list_by_company(company_id).await?;
        let mut periods: Vec<CashFlowPeriod> = query
            .cut()
            .map(|(start, end)| CashFlowPeriod {
                start,
                end,
                received: Decimal::ZERO,
                paid: Decimal::ZERO,
                transfers_in: Decimal::ZERO,
                transfers_out: Decimal::ZERO,
                net: Decimal::ZERO,
                new_opening_balances: Decimal::ZERO,
                closing_balance: Decimal::ZERO,
                to_receive: query.account.is_none().then_some(Decimal::ZERO),
                to_pay: query.account.is_none().then_some(Decimal::ZERO),
                projected_balance: None,
            })
            .collect();

        let mut opening_balance = Decimal::ZERO;
        for account in accounts {
            let opening: Option<AccountOpening> = openings
                .iter()
                .find(|opening| opening.account_id() == account.id())
                .cloned();
            if let Some(before) = query.from.pred() {
                opening_balance += self
                    .balance_of(company_id, account.clone(), opening.clone(), before)
                    .await?
                    .balance;
            }
            // An account's history starts at its opening day: what is dated before it is left out.
            let first = opening
                .as_ref()
                .map_or(query.from, |opening| opening.on().max(query.from));
            if let Some(opening) = opening
                .as_ref()
                .filter(|opening| (query.from..=query.to).contains(&opening.on()))
            {
                period_of(&mut periods, opening.on()).new_opening_balances +=
                    opening.balance().as_decimal();
            }
            if first > query.to {
                continue;
            }
            for day in self
                .entries
                .cash_by_day(company_id, account.id(), first, query.to)
                .await?
            {
                let period = period_of(&mut periods, day.date);
                period.received += day.received;
                period.paid += day.paid;
            }
            for day in self
                .transfers
                .movement_by_day(company_id, account.id(), first, query.to)
                .await?
            {
                let period = period_of(&mut periods, day.date);
                period.transfers_in += day.received;
                period.transfers_out += day.paid;
            }
        }

        if query.account.is_none() {
            for day in self
                .entries
                .open_by_due_day(company_id, query.from, query.to)
                .await?
            {
                let period = period_of(&mut periods, day.date);
                period.to_receive = period.to_receive.map(|total| total + day.receivable);
                period.to_pay = period.to_pay.map(|total| total + day.payable);
            }
        }

        let past_due = match (query.account, query.from.pred()) {
            (None, Some(before)) => {
                let late = self
                    .entries
                    .open_by_due_day(company_id, CalendarDate::EARLIEST, before)
                    .await?;
                Some(PastDue {
                    receivable: late
                        .iter()
                        .map(|day| day.receivable)
                        .sum::<Decimal>()
                        .normalize(),
                    payable: late
                        .iter()
                        .map(|day| day.payable)
                        .sum::<Decimal>()
                        .normalize(),
                })
            }
            _ => None,
        };
        let mut closing = opening_balance;
        let mut still_open = past_due.map_or(Decimal::ZERO, |late| late.receivable - late.payable);
        for period in &mut periods {
            period.net = period.received - period.paid + period.transfers_in - period.transfers_out;
            closing += period.net + period.new_opening_balances;
            period.closing_balance = closing;
            if let (Some(to_receive), Some(to_pay)) = (period.to_receive, period.to_pay) {
                still_open += to_receive - to_pay;
                period.projected_balance = Some(closing + still_open);
            }
            for amount in [
                &mut period.received,
                &mut period.paid,
                &mut period.transfers_in,
                &mut period.transfers_out,
                &mut period.net,
                &mut period.new_opening_balances,
                &mut period.closing_balance,
            ] {
                *amount = amount.normalize();
            }
            for amount in [
                &mut period.to_receive,
                &mut period.to_pay,
                &mut period.projected_balance,
            ]
            .into_iter()
            .flatten()
            {
                *amount = amount.normalize();
            }
        }
        Ok(CashFlow {
            opening_balance: opening_balance.normalize(),
            past_due,
            periods,
        })
    }
}
