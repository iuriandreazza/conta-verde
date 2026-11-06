---
title: "ADR 0037: The sales module: plan, shared rules and what it changes underneath"
status: accepted
date: 2026-11-06
tags: [adr, sales, phase-3]
---

# ADR 0037: The sales module: plan, shared rules and what it changes underneath

## Context

Phase 3 turns the registers and the financial control of Phases 1 and 2 into sales: a quote that becomes a
sale whose receivables show in the cash flow, contracts that bill themselves, commissions and a dashboard
with real figures. It touches things that Phase 2 settled on purpose (an entry is never cancelled, an
entry's origin is a recurrence, a payment term is only a register), so those decisions are made once here,
before the first slice, and each slice's own ADR only says what is new.

No `proposed` ADR belongs to this phase (0010, 0013 and 0015 are for Phases 4, 6 and 8), so there is
nothing to accept before starting.

## Decision

### The slices

Each is one pull request with its own ADR, a migration, both adapters, the routes and the pages.

1. **Quotes** (ADR 0038): items, discounts, validity, PDF, e-mail with a public approval link.
2. **Sales** (ADR 0039): orders and sales with a status lifecycle, receivables from a payment term,
   cancellation, returns, and turning a quote into a sale.
3. **Contracts** (ADR 0040): a billing cycle that makes a sale and its receivable per period, adjustment and
   termination, run by the hourly job.
4. **Commissions** (ADR 0041): rules per seller and per product and the statement of a seller.
5. **Price lists** (ADR 0042).
6. **Dashboard and sales reports** (ADR 0043): real figures instead of the sample data, sales by customer,
   product, seller and period, and the ABC curve.

### Items, and the one place sales round

A **line** of a quote, sale or contract is a product (or service) of the company, a description (a snapshot,
the product's name unless typed), a quantity above 0 with at most 3 decimals, a unit price with at most 6
decimals (the precision of `Amount`) and a discount in cents. The line's total is
`round(quantity × unit_price) − discount`, never negative; the discount is at most the rounded gross. A
document has 1 to 100 lines, and its total is the sum of the line totals and **is never stored**. There is no
discount on the document as a whole: a percentage is turned into a discount per line by whoever types it, so
every product keeps its own net figure for the reports and the commissions.

Rounding is a named rule (ADR 0003): **half away from zero, to the cent, by one function in `domain`**
(`round_to_cents`). It is used for the line (quantity × price), for a commission (rate × base), for an
adjustment (percentage × price) and nowhere else; splitting an amount over installments is not rounding, the
remainder goes to the last installment. A refund for part of a line is the line total times the returned part
of its quantity, taken **cumulatively** (what has been returned so far, rounded, less what was refunded
before), so the parts always add up to the line.

### Numbers

Quotes and sales have a number per company (1, 2, 3…), given by the store inside the write from a counter
row per company and kind (`document_counters`), so two writers never take the same one. It is shown to people
and never changes; the id is what the API uses.

### Applying a payment term

`PaymentTerm::split(total, from)` in `domain` is the one place a term is applied: each installment is the
amount times its percentage, **the remainder on the last**, due `days` after `from`; the result has the
installments of an entry. A document without a term has one installment due on its date. `web/src/lib/` has
its own copy for previews, in whole cents as a `bigint`, and a test holds the two to the same worked cases.

### A sale's receivable, and cancelling an entry

A completed sale makes **one receivable entry** whose installments come from the term, in the category the
sale names (an active, non-transfers category, checked like any entry's, and only when it changes), with the
customer as its party and the sale's number in its description. Where it came from is its **origin**.
`EntryOrigin` becomes an enum, `Recurrence { id, occurrence_on }` or `Sale { id }` (a new column and a
composite foreign key, never the old migration edited), a CHECK says at most one origin is set, and a unique
index gives a sale at most one such entry, which makes completing it twice, or in two places, safe, the way
ADR 0033 does for occurrences.

Phase 2 said an entry is settled and reversed, never cancelled. A cancelled sale needs the missing
primitive, so an entry gains a one-way **voided** mark, set only by the use case of whatever made it:

- It is refused (`409`) while the entry has a settlement that is not reversed: the money is reversed first.
- A voided entry stays listed (filter `status=voided`) and in its trail, cannot be edited or settled, and
  counts nowhere: not in the open amounts of the cash flow, the overdue list or the reports by group, and it
  is no candidate of a reconciliation. Each reader excludes it in both adapters, with a contract test.
- It is never un-voided: a sale that is cancelled is not reopened, another one is made.

A **return** records what came back of a completed sale (a quantity per line, at most what was sold less what
was returned), and makes a **payable** entry, the refund, for the returned part, in a category the person
names, with the customer as party and one installment due on the day they say. It does not touch the sale's
receivable: if the customer has not paid it yet, the two are netted by hand. One rule instead of a partial
voiding of an installment.

Completing and returning write in two steps (the document, then the entry): the entry's unique origin makes
the second step safe to repeat, and the use case runs it again when it finds a completed sale without one.

### Roles and routes

A new area, **`sales`**, joins the table of [ADR 0026](0026-membership-roles-and-permissions.md): owner and
admin write, the `sales` role writes, and every other role reads (the members area is still theirs alone). The
segments `quotes`, `sales`, `contracts`, `price-lists`, `commissions` and `sales-reports` are in it; the
`dashboard` segment is read only and in `finance`, which every role reads. A new family of routes is added to
`area_of` and to its test, or it is refused to everybody.

### The public approval link

A quote's link carries a 256-bit token whose SHA-256 digest is all the database keeps ([ADR 0027](0027-invitations.md)
does the same for invitations): `SecretService::new_token` and `digest_token`. The routes under `/public/`
are outside `/companies/…` and need no session: they are the **second** place the API answers without one
(after `GET /invitations/{token}`). They show one quote, serve its PDF, and take an approval or a refusal,
each once. The e-mail carries the link only, since `EmailMessage` has no attachments and the page serves the
PDF.

### One schedule

Contracts bill on a schedule like a recurrence. `Frequency` and the occurrence arithmetic (monthly counted
from the first day, the day clipped) move out of `Recurrence` into a type both use, with the existing
recurrence tests unchanged, rather than being written twice.

### The dashboard

The dashboard slice **supersedes the sample-data boundary** of [ADR 0020](0020-app-shell-charts-and-sample-data.md):
`web/src/dashboard/sampleData.ts` and its test are deleted when the real figures arrive, not kept.

## Consequences

- Every reader of entries learns "voided"; a mistake there would show cancelled sales in the cash flow, so
  each reader has its own contract test.
- A sale's receivable is an ordinary entry: it is settled, reconciled and reported like any other, and shows
  in the forecast of the cash flow ([ADR 0032](0032-cash-flow.md)) with no change to it.
- Returns do not reduce a receivable: they make a payable.
- The `sales` area changes `/auth/me` (a fifth permission) and the web's roles table.

## Revisit when

Stock arrives (Phase 5: a sale then moves stock and may be blocked without balance), fiscal documents
arrive (Phase 7: a sale is the document's source and a cancellation has fiscal rules), or a return has to
reduce an unpaid receivable instead of making a payable.
