---
title: "ADR 0029: Financial entries and installments"
status: accepted
date: 2026-11-05
tags: [adr, finance, phase-2]
---

# ADR 0029: Financial entries and installments

## Context

Phase 2 starts here. Every later piece of it (settlement, transfers, cash flow, reconciliation,
recurrence, reports) reads the same record: money a company is owed or owes, classified by a category,
split over dates. [ADR 0012](0012-managerial-accounting-model.md) fixed that every entry has exactly
one category and an optional cost center, and [ADR 0021](0021-bank-accounts-payment-methods-and-terms.md)
fixed how a term splits an amount. What is open is the shape of the entry, of its installments, and of
the dates and amounts in them, which are expensive to change once settlements hang from them.

## Decision

### The entry

A **financial entry** is a receivable or a payable (`kind`), with:

- a **description**, 1 to 200 characters, one line;
- an optional **party** (a customer for a receivable, a supplier for a payable; the kind does not
  force the party's role, since one party can be both);
- a **category**, required: it exists in the company, is **active** and is **not the transfers
  category** (a transfer is not an entry, it is its own movement, in a later slice);
- an optional **cost center** of the company, active;
- a **competence date**, the day the entry belongs to for the managerial income statement
  ([ADR 0012](0012-managerial-accounting-model.md)), which is not its due date;
- one to **120 installments**.

The entry has no total of its own: its total is the sum of its installments, never stored, so the two
cannot disagree.

### Installments

An installment has a **number** (from 1, in order), a **due date** and an **amount**.

- **Amounts** are exact decimals, more than 0, with **at most two decimals** (cents) and at most
  13 digits before the point, which is what [ADR 0003](0003-money-as-decimal.md)'s `Amount` allows. More
  than two decimals is refused, never rounded.
- **Due dates** do not decrease with the number (two installments may fall on the same day). They are
  not tied to the competence date: a bill for last month is entered after it fell due.
- Splitting a total into installments (the payment term's shares, "the remainder goes to the last
  one") is done by whoever asks for the entry, the web form today, and the API takes the installments
  as they are. The entry never records which term it came from.

### Dates

A **calendar date** is a year, a month and a day, with no time and no zone: a due date does not move
when the server's zone does. The domain has its own small type for it (`CalendarDate`, written
`YYYY-MM-DD`, years 1900 to 2200, a real day of a real month), so that `domain` keeps its two
dependencies; the database column is `DATE`, and the adapters convert through text.

### Editing

An entry is replaced as a whole by `PUT`, with its installments, and audits only the difference, like
every register ([ADR 0019](0019-products-and-services.md)). The installments are one field in the
trail, in a canonical text (`1:2026-12-10:150.50,2:2027-01-10:150.50`).

There is **no delete in this slice**, and no cancellation: an entry that was entered wrongly is
changed. What an entry becomes once an installment has been settled (which fields still change, and
how an entry is called off) is decided with settlements, in the next slice, and is not guessed here.

### References

The category, the cost center and the party are checked by the use case before it writes
(`422` naming the field), through a small port that answers whether a reference is usable
(`EntryReferences`: both adapters implement it over what they already store). The database also
refuses a reference to another company (composite foreign keys on `(company_id, id)`), which is the
rule that matters for tenancy; whether the category is active and is not the transfers one is not seen
by the database, and two requests at the same moment can slip past it, as with the categories' own
neighbour rules.

An edit checks only the references it **changes**: a category or cost center that was deactivated
after the entry was made does not stop the description or an installment from being fixed, it only
cannot be chosen again.

### The list

`GET /companies/{company_id}/entries` follows [ADR 0018](0018-list-endpoints-search-filter-sort.md):
`limit`, `after`, `q` (the description, ignoring case, bound with `strpos`), a `kind` filter, a
`category` filter (the id) and `sort` by `competence_date` or `-competence_date` (newest first by
default), always ending in the id. A list of **installments by due date** (what is due this week, what
is overdue) is the next slice's, with settlements, since "overdue" needs to know what was paid.

### Roles

The routes are in the `finance` area ([ADR 0026](0026-membership-roles-and-permissions.md)): reading
for every member, writing for the owner, admin and financial.

## Consequences

- An entry cannot be removed or called off yet. That is deliberate and short-lived; the alternative,
  a delete, would have to be undone once the trail of settlements exists.
- The competence date and the due dates are independent, which is what lets cash flow (by due and
  settlement dates) and the income statement (by competence) read the same entries.
- Sums over installments are done in the database or the domain as exact decimals, and the web app
  prints them from decimal strings (`formatMoney`) without turning them into numbers.
- Recurring entries ([ADR 0009](0009-postgres-only-background-jobs.md)) will generate ordinary
  entries ahead of time; nothing here needs to change for them.

## Revisit when

Settlements arrive (editing and cancelling), an entry needs more than one category (splitting a bill
across categories), or entries need attachments.
