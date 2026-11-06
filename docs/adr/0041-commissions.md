---
title: "ADR 0041: Sellers and commissions"
status: accepted
date: 2026-11-06
tags: [adr, sales, commissions, phase-3]
---

# ADR 0041: Sellers and commissions

## Context

Phase 3 asks for "sellers and commissions: rules per seller or product, commission statement". A sale already names a seller
(a party marked `is_seller`, [ADR 0039](0039-sales.md)) and returns take value back from it. What is missing is how much a
seller earns and the statement that says so.

## Decision

### The rule

A **commission rule** (`/companies/{id}/commission-rules`, `sales` area, audited, replaced as a whole by `PUT` that must say
`active`) is a rate for **one seller or all** (no seller is every seller; a party that is not a seller is refused), for **one product or all**,
over the sales made from `starts_on` through `ends_on` (optional). The rate is a percentage above 0 and at most 100 with at most
two decimals, a string like every other figure. A rule is never deleted: it is switched off, and the trail keeps it.

The days are what keeps a statement from changing under a rule that was edited later: **a new rate is a new rule that starts
later**, and a sale is judged by the rules in force on **its own day** (`sold_on`), not on the day the statement is read.

### Which rule applies

For a line of a sale, by a seller, on a day, the rule is the **narrowest** that covers it: seller and product, then the seller
only, then the product only, then the general one. Inactive rules and rules whose days do not include the sale's are ignored.
Among equals the one that **started last** wins and, to be independent of the order they were read in, then the greater id.
A line that no rule covers earns nothing, but its value still counts in the base.

### The statement

`GET /companies/{id}/commissions?from=&to=&seller=` (both dates required, since the server reads no clock; `from` after `to` is a
`400`) is **computed on every read**, never stored, like the balances (ADR 0031): from the **completed** sales sold in the span
that have a seller (an order and a cancelled sale earn nothing, and one without a seller is not listed), net of returns.
Per line, the **base** is the line's total less what was refunded of it (the cumulative refund of the return, the same rounding
as ADR 0039) and the **commission** is `round_to_cents(base × rate / 100)` by the one rounding rule of ADR 0037, line by line, so
the sum of a sale's lines is what is paid. The answer groups the sales by seller with each sale's base and commission and the
totals. A return made after the span is still taken off: the statement says what a sale is worth now. More than 2000 sales in a
span are refused (`400`): narrow it.

Nothing is "paid" yet: there is no payable made from a statement. Paying a commission is an ordinary payable entry the person
makes; tying the two together (a closing of the month that can not be repeated) is a later slice if it is asked for.

### Where it lives

`CommissionService` composes the rule port and the **sales' port** (`SaleRepository::list_by_company` with a `sold_between`
filter and `returns_of_sales`) instead of a query across tables, so both adapters answer alike. The rules table has composite
foreign keys (company, seller) and (company, product), a `CHECK` on the rate and on the days, and the audit entry of each write
in the same transaction. The `commissions` and `commission-rules` segments are the `sales` area: owner, admin and sales write
(rules), everyone who reads sales reads the statement.

## Consequences

- A statement is as old as its rules and sales: changing a rule that already covers sold days changes the statement for those
  days. The way to change a rate from now on is to end the old rule and start a new one, which the form says.
- There is no lock on a month: the statement is a report, not a closing.
- A seller is judged by one rule per line, never by a sum of rules; a tiered rate (more above a target) would be a new kind of rule.
