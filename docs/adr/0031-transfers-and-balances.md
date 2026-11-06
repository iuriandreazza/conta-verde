---
title: "ADR 0031: Transfers, opening balances and balance per account"
status: accepted
date: 2026-11-05
tags: [adr, finance, phase-2]
---

# ADR 0031: Transfers, opening balances and balance per account

## Context

Settlements ([ADR 0030](0030-settlements.md)) move money in and out of bank accounts, but an account
still has no balance, no way to say how much it held before the product was used, and no way to move
money between two accounts of the company. [ADR 0021](0021-bank-accounts-payment-methods-and-terms.md)
left all three for Phase 2, and cash flow and reconciliation need them.

## Decision

### Opening balance

An account may have one **opening balance**: an amount and the **day it is the balance of, at the start
of that day**. It is its own record, one per account, kept apart from the account's register so that
registering and changing an account is untouched: `PUT /companies/{id}/bank-accounts/{id}/opening-balance`
with `{ "balance": "1500.50", "on": "2026-01-01" }` sets or replaces it, and a change is recorded in
the audit trail ([ADR 0008](0008-audit-trail.md)). The account must exist in the company; it need not be
active.

The amount is a **signed** decimal in cents, up to thirteen digits before the point: a card or an account
in overdraft starts below zero. A signed amount is its own domain type; an `Amount` stays non-negative
([ADR 0003](0003-money-as-decimal.md)).

### Transfers

A **transfer** moves money from one bank account of the company to another, on a day: `from` and `to`
(different, both of the company and active), an `amount` above 0 in cents, an `occurred_on` date and an
optional one-line note of at most 200 characters. It is **not an entry**: it has no category, no party and
no installments, and nothing about it appears in the managerial income statement, which is what the
reserved transfers category of [ADR 0012](0012-managerial-accounting-model.md) was for.

Like a settlement, a transfer is never edited or deleted: `DELETE` **reverses** it (`reversed`), it stops
counting, and the trail keeps it. A transfer to a card account is how a card bill is paid. Reversing a
transfer already reversed is `404`.

### Balance

The balance of an account **on a day** is its opening balance (0 when it has none) plus what its
settlements and transfers moved in it, from the opening day up to and including that day:

- a settlement's **cash** ([ADR 0030](0030-settlements.md)) adds for a receivable and takes off for a
  payable, by the day it was settled;
- a transfer adds to the account it goes to and takes off the one it comes from, by the day it occurred;
- reversed settlements and transfers do not count, and neither does anything dated before the opening
  day: the opening balance already holds it. Without an opening balance, everything counts.

`GET /companies/{id}/balances?on=YYYY-MM-DD` gives, for every account of the company (inactive ones
included), the opening balance, what was received and paid through settlements, what came in and went out
through transfers, and the balance, plus the **consolidated** total (the sum of the balances). `on` is
required: the server never reads a clock, because today depends on the person's zone
([ADR 0030](0030-settlements.md)); the web app sends today.

Nothing is stored: the balance is added up on every read, from the settlements and the transfers. A
materialized daily balance, if it is ever needed for a long history, is a cache rebuilt from them.

### Where the rules live

The balance is composed by a use case from three ports, not by one query across tables, so that every
adapter, in memory included, answers it the same way: the accounts of the company, the settled cash of one
account over a span of days (a method of the entries' port, which owns the settlements), and what a
transfer moved in one account over a span (the transfers' port). Postgres sums in the database; the
number of accounts of a company is small, so one pair of sums per account is the cost.

### Roles

All of it is in the `finance` area ([ADR 0026](0026-membership-roles-and-permissions.md)): reading for every
member, writing for the owner, admin and financial. The routes are `transfers`, `balances` and the opening
balance under `bank-accounts`.

A read is not one snapshot: each account costs two separate queries, so a transfer committed while the
balances are being read can be counted on one side and not yet on the other, and the total is then off by
that amount until the next read. Setting an opening balance reads the stored one and then writes, so two
first writes at once both record a creation. Both are accepted for now (nothing stored is wrong, and the
next read is right); a `REPEATABLE READ` read like the one parties use is the fix if it ever matters.

## Consequences

- A person sets the opening balance of each account once, as of the day they start; before it was set the
  balance of an account is only what the product has seen.
- Paying a card bill is a transfer to the card account, so it does not touch the income statement, and the
  card's balance is its debt (negative) until paid.
- Cash flow, forecasts and reconciliation read the same settlements and transfers.

## Revisit when

A balance has to be read for years of history at once (cache), a bank statement has to be matched to a
transfer or a settlement (reconciliation), or transfers between companies are asked for.
