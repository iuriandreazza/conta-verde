---
title: "ADR 0039: Orders and sales, their receivables, cancellation and returns"
status: accepted
date: 2026-11-06
tags: [adr, sales, finance, phase-3]
---

# ADR 0039: Orders and sales, their receivables, cancellation and returns

## Context

A quote that was approved has to become money owed ([ADR 0038](0038-quotes.md), [ADR 0037](0037-sales-module-plan.md)): a sale
whose receivables show in the cash flow ([ADR 0032](0032-cash-flow.md)). ADR 0037 already decided the shapes this slice
needs (lines and the one rounding rule, numbering, applying a payment term, a sale as an entry's origin, an entry that can be
cancelled); this one says how they work together.

## Decision

### The register and its life

A sale (`/companies/{id}/sales`, `sales` area, audited) has a number, a customer, an optional seller (a party with the seller
role), the day of the sale, an optional payment term, the **category of the receivable** (an active category that is not the
transfers one, with an optional cost center), notes, the lines of ADR 0037, the quote it came from (if any) and a status:

- `open`: an **order**. Agreed, not billed, and the only state in which it can change (`PUT` replaces it as a whole).
- `completed`: **billed**. Its receivable exists and its items are fixed.
- `cancelled`: final. It is not reopened; another is made.

What it points at is checked when it is written and only when it changes, like a quote's. A write names the status it was made
from and is refused (`409`) if the sale is in another by then.

### Completing

`POST .../complete` makes the sale's **one receivable entry**: kind receivable, the customer as party, the sale's category and
cost center, competence date the day of the sale, the description "Venda nº N", and installments from the payment term
(`PaymentTerm::split`: each share is the total times its percentage **cut to the cent**, the last takes the rest, each falls due
`days` after the day of the sale), or one installment due on the day of the sale when there is no term. The entry's origin is the
sale, and a unique index gives a sale at most one entry.

The receivable is validated **before** the sale changes (the entry's own rules and what it points at, through
`FinancialEntryService::validate`); then the sale is marked completed, and then the entry is written. Asking again for a
completed sale writes the entry if a failure left it unmade, and `DuplicateOrigin` (it exists) is the success: completing is
idempotent, safe to repeat and to run twice at once. A sale worth nothing, or too little to be split by its term, is refused.

### Cancelling

Cancelling an open order only marks it. Cancelling a completed sale **cancels its receivable first** (the primitive of ADR 0037):

- It is refused (`409`) while the receivable has a settlement that is not reversed: the money is reversed first, by the person.
- It is refused (`409`) while the sale has returns, which stand: what was refunded cannot be taken from a sale that never was.
- A voided entry is still listed (`voided=true|false` narrows the list), marked, and cannot be edited or settled; it is out of
  the open amounts of the cash flow, the overdue list and the reports by group, by construction in both adapters. It needs no
  other reader to change: a voided entry has no settlement that counts, so the cash and the reconciliation never see it.
- Asking again finishes a cancellation a failure left half done (the entry voided, the sale not yet).

### Returns

A **return** records what came back of a completed sale: a quantity per line (a line once, more than 0, never more than was sold
and not returned yet), a day and a reason. It never changes. It makes a **payable** entry for the refund, to the customer, in a
category the person names, with one installment that falls due where they say: a return does **not** reduce the receivable (if the
customer has not paid it yet the two are netted by hand), which keeps one rule for every case.

The refund of a line is its total times the returned part of its quantity, taken **cumulatively** (ADR 0037): what has been
returned so far, rounded to the cent, less what was refunded before, so the parts of a line add up to the line exactly. The
adapter checks the quantities with the sale locked, so two returns at the same moment cannot both pass.

The return is stored first and the payable second (the payable's foreign key needs the return). The payable is validated before
anything is written, so only a storage failure can leave a return without its refund; the answer then says `refund_entry_id:
null` and the return list shows it. There is no way yet to make that payable again: it is written by hand.

### From a quote

`POST /quotes/{id}/convert` (with the day, the category of the receivable and, optionally, a cost center, a seller and another
term) makes an `open` sale from a quote that is not declined or converted: the customer, the term, the notes and the lines are
copied, and the quote becomes `converted` (it keeps its link, which then shows it approved). A quote makes **one** sale (a unique
index): asking again gives that sale, and finishes marking the quote if a failure left it unmarked.

### Concurrency

A transition (completing, cancelling) writes **only the status** (`set_status`), under the sale's row lock, so an edit that landed
meanwhile is never overwritten with an older copy, and a completed sale's receivable is worked out from the sale **as stored**
once it is completed. Cancelling is refused at the store, atomically, if the sale has returns; a return is refused at the store
unless the sale is still completed and has exactly the returns the use case worked its refund out from (`SaleChanged`, `409`: ask
again), so the cumulative refund never rests on a stale read. If a sale is cancelled between its completion and the write of its
receivable, the receivable is cancelled at once. Concurrent edits of an open order are last write wins, like every register.
What a sale or a return made is **not edited as an entry of its own** (`409`): it changes through the sale.

### Roles

The `sales` and `quotes` segments are in the `sales` area, so a user with sales write may cancel a sale and record a return, which write finance entries as their consequence (cancelling a receivable, making a refund payable): the areas are coarse ([ADR 0026](0026-membership-roles-and-permissions.md)) and the person who sells is the one who takes it back; a completed sale's receivable is an entry, which the `finance` area
reads and settles like any other.

## Consequences

- A sale's receivable shows in the cash flow forecast through the entries' port with no change to the cash flow itself; the
  exit criterion of the phase is a test: a quote, approved through its link, converted, completed, in the forecast, and out of it
  when cancelled.
- The entries page says where an entry came from and marks a cancelled one.
- A completed sale whose receivable a storage failure left unmade is finished by asking `POST .../complete` again; no screen offers it yet (the API answer for a sale says `receivable_id`), and no job looks for it.
- Commissions and the dashboard of the next slices read sales (completed, net of returns) and not entries.

## Revisit when

Stock arrives (completing a sale then moves stock, and may be blocked without a balance), fiscal documents arrive (a sale is
their source, and cancelling has fiscal rules), a return has to reduce an unpaid receivable, or the payable of a refund has to be
made again by the system.
