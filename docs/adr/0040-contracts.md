---
title: "ADR 0040: Recurring contracts that bill themselves"
status: accepted
date: 2026-11-06
tags: [adr, sales, jobs, phase-3]
---

# ADR 0040: Recurring contracts that bill themselves

## Context

A mensalidade, a maintenance plan, a subscription: the same lines sold to a customer every cycle. Phase 3 asks for a
billing cycle, "automatic sale and receivable per period", adjustment and termination. [ADR 0033](0033-recurring-entries.md)
did the same for entries, with a job that runs every hour and is safe to repeat; a contract reuses that shape, with the sale of
[ADR 0039](0039-sales.md) as what each cycle makes.

## Decision

### The contract

A contract (`/companies/{id}/contracts`, `sales` area, audited) has a name, a customer, an optional seller and payment term, the
category (and cost center) of the receivable, notes, the lines of [ADR 0037](0037-sales-module-plan.md), a **billing cycle**
(`monthly`, `bimonthly`, `quarterly`, `semiannual`, `yearly`), the day of the first cycle (`starts_on`), an optional last day
(`ends_on`) and a status: `active`, `paused` or `terminated`. What a cycle bills is the sum of the lines, never stored. It is
replaced as a whole by `PUT`, which must say whether it is active (a body without it would switch a paused contract back on).

The cycles are `starts_on` plus 0, 1, 2… cycles, **counted from `starts_on`** with the day clipped to the end of a short month
(the 31st gives the 28th and then the 31st again), by the same `CalendarDate::plus_months` the recurrences use; there was no
need to extract more than that.

### A cycle is a sale

A cycle that has fallen due becomes an ordinary **completed sale** (ADR 0039) dated on the cycle day, with the contract's
customer, seller, term, category, cost center, notes and lines, and so its one receivable by the term. The sale remembers its
contract and cycle day, and **a contract has at most one sale per cycle day** (a unique index), so billing twice, or in two
places at once, cannot duplicate one. A cycle that already has its sale is only **finished** (completed, its receivable made):
asking again repairs what a failure left undone.

A contract bills **on the cycle day, not ahead**, unlike a recurring entry (ADR 0033): a sale dated in the future is odd, and the
receivable's due dates are what the cash flow forecasts. It remembers the day it was billed through (`billed_through`), so a pass
only looks at what is new, and a contract that starts in the past, or is written late, **catches up** (24 cycles at most per
pass). A change to the cycle or the first day applies from where it was billed through and does not start over. A paused
contract bills nothing, and one brought back goes on from today: what fell due while it was paused is skipped, not made up.
A terminated one is fixed but **still bills the cycles that fall due up to its last day**, each on its own day, and none after it. A cycle whose sale a person cancelled is **consumed**: it is not billed again and does not stop the cycles after it. A contract whose cycle would bill nothing is refused, since it would fail every time. The job re-reads each contract before billing it, so a pause, an adjustment or a termination that landed after its batch was read is respected, and the day it bills up to is **yesterday by the UTC clock** (`customer_today`, as for a customer's answer: UTC turns the day at 21:00 in Brazil, and a cycle must not be billed a day early). Correcting the first day of a contract that was never billed (its watermark is before it) starts from the top.

The sales are made **in the name of whoever last saved the contract** (there is still no system actor in the trail). If a cycle
cannot be billed (a product was deactivated since), that contract is left as it was and tried again on the next pass; the others
go on. A failure while writing the contract is not shown to the person: it is stored and the hourly pass tries again.

### When it runs

It runs when a contract is written (with yesterday by the server's UTC clock, the second place besides the recurrences and the quote link
where the server reads its clock) and **every hour** in a task inside the API process, for every company. The use case takes
the day as a parameter, so the tests move the clock: `bill_all_due(day)` three times, at three advancing days, makes three sales
and three receivables, and running it again adds nothing.

### Adjustment and termination

`POST .../adjust` raises (or lowers, below 0) the **unit price of every line by a percentage** between -99.99 and 1000, each new
price rounded to the cent by the one rule of ADR 0037 (a discount that would exceed its line is refused). It applies to the
cycles billed from then on; what was billed is never rewritten. `POST .../terminate` ends the contract on a day: it sets its
last day (never later than it already had), the cycles already due up to it are billed at once, and it is **fixed** from then on (`409`): to bill again, a new contract is made. Both
are audited like any change.

## Consequences

- A contract's sales are listed with `GET /sales?contract=` and say they come from one; their receivables show in the entries and
  the cash flow like any other.
- The contract has no number and no PDF; it is found by name.
- A contract that bills a product that was deactivated fails every pass until the contract is changed; the list shows how far it
  was billed, which stops moving.

## Revisit when

A contract needs a different price per period beyond a percentage (an index table), a first cycle that is not on a cycle day
(a pro rata), billing ahead of the day, or sales made in the name of a system user.
