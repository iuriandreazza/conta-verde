---
title: "ADR 0032: Cash flow, realized and forecast"
status: accepted
date: 2026-11-06
tags: [adr, finance, phase-2]
---

# ADR 0032: Cash flow, realized and forecast

## Context

Settlements ([ADR 0030](0030-settlements.md)), transfers and opening balances
([ADR 0031](0031-transfers-and-balances.md)) say what moved in each account, and the open installments of
the entries ([ADR 0029](0029-financial-entries.md)) say what is still to come. The managerial model
([ADR 0012](0012-managerial-accounting-model.md)) asks for cash flow by **settlement date** (realized) and
by **due date** (forecast), per account and consolidated.

## Decision

`GET /companies/{id}/cash-flow?from=YYYY-MM-DD&to=YYYY-MM-DD&granularity=day|month[&account={id}]` returns the
money over a span of days, cut into **periods**. `from` and `to` are required (the server never reads a
clock, [ADR 0030](0030-settlements.md)), `from` may not be after `to`, and the span may cut into at most 400
days or 60 months; `granularity` is `month` unless said otherwise. A month period is a calendar month
clipped to the span, so the first and the last may be partial. An unknown value is `400` JSON
([ADR 0018](0018-list-endpoints-search-filter-sort.md)); an `account` of another company, or one that does not exist, is `404`.

### Realized

For every period, over the accounts of the company (inactive ones included) or only the one asked for:

- `received` and `paid`: the **cash** of the settlements that are not reversed, by the day they were
  settled, of receivables and of payables;
- `transfers_in` and `transfers_out`: the transfers that are not reversed, by the day they occurred. With
  no account chosen every transfer has both ends in the company, so the two are equal and the consolidated
  balance does not move; with an account chosen they are what came into it and went out of it;
- `net`: `received - paid + transfers_in - transfers_out`;
- `new_opening_balances`: the opening balances ([ADR 0031](0031-transfers-and-balances.md)) whose day falls
  in the period, which are money that enters the account's history that day;
- `closing_balance`: the balance at the end of the period.

The balance follows the rule of ADR 0031: an account counts from its opening day (everything counts when it
has no opening balance), so settlements and transfers dated before it are left out, and the balance before
it is 0. `opening_balance` of the answer is the balance at the start of `from`: the balance of the day
before. Then `closing_balance` of a period is the one before it, plus `net`, plus `new_opening_balances`.

### Forecast

What is still open on the installments of the entries, by **due date**: an installment's amount less what
its settlements that are not reversed settled, when more than 0. `to_receive` and `to_pay` are those of
receivables and of payables due in the period, and `projected_balance` is the realized `closing_balance`
plus the open amounts due up to the end of the period, as if all of it were settled on its due day.
What fell due **before** the span and is still open is late, and is told apart as `past_due`
(`to_receive` and `to_pay`) in the answer: the projected balances count it from the first period, as if it
were settled on the first day, because what is late is what the person has to look at. Late
installments inside the span are in the period of their due date. An installment has no bank account, so **the forecast is only for the consolidated
view**: with an `account` chosen the three forecast fields, and `past_due`, are `null`.

### Where the rules live

Like the balances, it is composed by `BalanceService` from the ports, never by one query across tables:
per account, the settled cash by day (entries' port) and the transfers by day (transfers' port), cut at the
account's opening day, and the open installments by due day (entries' port). The cutting into periods, the
running balance and the forecast are done in the use case, so both adapters answer alike. Amounts are
exact decimals, never floats ([ADR 0003](0003-money-as-decimal.md)).

### Roles

The `cash-flow` route is in the `finance` area ([ADR 0026](0026-membership-roles-and-permissions.md)); it only
reads.

## Consequences

- The web app's cash flow page asks for the span the person chooses and draws the realized balance with the
  forecast after it.
- A balance chart of the whole history is not what this is for: the span is bounded.
- Reconciliation ([ADR 0011](0011-bank-data-ingestion.md)) compares statement lines with the settlements and
  transfers this reads.

## Revisit when

A span longer than five years is needed, the forecast has to be per account (installments would need an
expected account), or reading a long span per account is slow enough to want a stored daily balance.
