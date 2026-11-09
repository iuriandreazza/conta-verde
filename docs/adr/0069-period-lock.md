---
title: "ADR 0068: Period lock"
status: accepted
date: 2026-11-09
tags: [adr, accounting, audit, phase-6]
---

# ADR 0068: Period lock

## Context

[ADR 0067](0067-accountant-and-reporting-plan.md) decides what the lock is for and which date locks each record; this
ADR is how it is built. A closed month has to stay what the accountant closed, whoever writes and however the write
arrives: a person through the API, the hourly passes (recurrences, contracts, reminders), a payment webhook.

## Decision

- **One row per company**, `period_locks(company_id, locked_through)` (migration `20261212000000`), replaced as a whole and
  audited like the other per-company settings (`AuditEntity::PeriodLock`, the audit id is the company's). The date is
  nullable: a company that reopened everything keeps its row. A `CHECK` repeats the one rule a row can show: the date is the
  last day of a month.
- **Domain.** `PeriodLock` (`locked_through`, `locks(day)`, `is_reopened_by`) and `reopen_reason` (one trimmed line of 1 to
  200 characters). Closing moves the date forward and needs no reason; **reopening** (an earlier date, or none) needs one,
  and the trail records it as a `reason` change beside the old and the new date. The same date again writes nothing.
- **Application.** `PeriodLockRepository` (`find`, `save` with its audit entry, `history`, and `locked_through`) and
  `PeriodLockService` (`get`, `replace`, `history`). The refusal is `ApplicationError::PeriodLocked { through }`, a `409`
  whose message names the day: `the period through 2026-10-31 is closed: reopen it to change what is dated on or before it`.
- **How the services ask.** The check is in the use cases, **before the first write of the operation**, through what they
  already hold: `EntryReferences` gains `locked_through` (implemented by `RegisterReferences`, which now takes the lock
  repository, so it cannot be built without one) and a provided `ensure_open(company, day)`. That reaches entries and
  settlements (`FinancialEntryService`), transfers (`TransferService`), sales and returns (`SaleService`), receipts
  (`PurchaseService`), the entries import and, through `register_with_origin`, everything that makes an entry: recurrences,
  contracts, charge fees, a statement line turned into an entry. `StockService` and `BalanceService`, which hold no
  references, take the lock port as one more type parameter that defaults to `NoPeriodLock`; the API's `Ports` names the lock
  in their types, so a deployment cannot leave it out, while the services' own tests, which are about something else, need
  not. Every manual or automatic stock movement, an inventory count's adjustments among them, goes through one funnel
  (`record_valued`) and a reversal also checks the **original's** day.
- **Entry edits** are refused on the old or the new competence date. A settlement is judged by its own day, so an October
  invoice is paid in November and nothing of October moves. **Open documents** (a draft sale, an approved order, a quote,
  an open count) are not the books and may be written in a closed month; completing, cancelling a completed one,
  returning or receiving is refused, before anything of it is stored (a return and a receipt are stored first, so their day
  is checked first).
- **Automatic flows.** The recurrence and contract passes already count a failure and try again on the next pass, so a
  closed month is refused and a reopened one catches up; both go **in order**, so an occurrence or a cycle in a closed month
  holds the ones after it of that recurrence or contract (the others go on) until the month is reopened, and each pass logs
  it as failed. A webhook whose payment day is closed settles nothing and enters
  no fee: the charge is kept **paid, with a note** that says why, the event is `not_applied`, and the provider is told
  `200` ([ADR 0046](0046-webhooks-and-automatic-settlement.md)).
- **Routes**, in the `accounting` area ([ADR 0067](0067-accountant-and-reporting-plan.md)): `GET` and `PUT
  /companies/{id}/accounting/period-lock` (`{locked_through, reason?}`) and `GET .../period-lock/history`. A read for
  every role that reaches the company, a write for the owner, admins and the accountant.
- **Web.** The Contador page shows the lock, closes a month (the last day of the month chosen), reopens with a reason, and
  lists who did what; the form is `useCanWrite("accounting")`.

## Consequences

- The tests that hold it: the lock itself, entries, settlements, transfers and openings, stock, sales and purchases, and a
  payment on a closed day, each on both adapters (`runs_the_period_lock`, `a_closed_month_refuses_what_the_books_hold`,
  `a_closed_month_refuses_stock_and_documents`, `settles_charges_from_webhooks`), and one through the API, from the
  accountant's own login to the `409` on every route that writes a dated record (`an_accountant_closes_a_month_and_the_books_of_it_refuse_changes`).
- Closed months do not change, so the DRE and the exports of a closed month give the same figures every time.

## Known limits

- **The check is made before the write, not in its transaction.** A write that began before the month was closed can land
  just after; two writes racing a closing are not ordered. Closing is rare and the accountant reads the month's figures
  after it. The fix, if it ever matters, is to read the lock inside each adapter's write with `FOR SHARE`.
- A request with several writes (an import, a sale that completes into an entry and stock) is checked at each, so a
  failure part way leaves what the earlier steps wrote, as those use cases already say about any failure; the steps that
  are stored first (a return, a receipt) are checked first for that reason.
- **Not locked**: attachments (a missing document is added to a closed month all the time), quotes, drafts and open orders,
  the registers (categories, parties, products), and a statement line's matching. A category's DRE group never changes
  ([ADR 0012](0012-managerial-accounting-model.md)), so classification cannot move a closed month.
- Applying an inventory count on a closed day is refused only when it would write an adjustment (a count where every line
  already agrees changes no figure). Any month-end can be closed, a far future one included: it freezes every write until it
  is reopened.
- The lock is one date per company: closing half a month, or locking only some areas, is not possible.

## Revisit when

A write racing a closing matters, an accountant needs to lock one area only, or the lock has to be undone by the owner
alone.
