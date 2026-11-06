---
title: "ADR 0035: Reports of the entries: by category, cost center and party, and the overdue list"
status: accepted
date: 2026-11-06
tags: [adr, finance, reports, phase-2]
---

# ADR 0035: Reports of the entries: by category, cost center and party, and the overdue list

## Context

The entries ([ADR 0029](0029-financial-entries.md)) carry a category, an optional cost center and an optional
party, a competence date and installments with due dates. The managerial model
([ADR 0012](0012-managerial-accounting-model.md)) wants to read them by those axes, and a business needs to
see what is late.

## Decision

### By category, cost center or party

`GET /companies/{id}/reports/entries?group=category|cost_center|party&basis=competence|due&from=&to=` adds up
the entries of the company over a span of days (`from` through `to`, both required, `from` not after `to`),
one row per **group**: the `receivable` total, the `payable` total, their difference (`net`, receivable less
payable) and the number of entries. A row whose `id` is `null` is what has no cost center, or no party (a
category is always there). Rows are ordered by the larger of the two totals, biggest first, then by id.

The **basis** says which day puts an entry in the span, and what is added up:

- `competence` (the default): the entries whose **competence date** is in the span, by their **total**: what
  the business earned and spent in the period, which is the managerial view;
- `due`: the **installments** whose **due date** is in the span, by their **amount**: what falls due in it. An
  entry with installments in several months counts in each of them (the count is of entries with at least one
  installment in the span).

Amounts are what was **entered**, not what was settled: whether it was paid is the cash flow's
([ADR 0032](0032-cash-flow.md)) and the overdue list's. Exact decimals, never floats
([ADR 0003](0003-money-as-decimal.md)); the response sends ids and the web app names them from the registers it
already reads.

### The overdue list

`GET /companies/{id}/reports/overdue?on=YYYY-MM-DD[&kind=receivable|payable][&limit=]` lists the installments
that fell due **before** `on` and still have something left (their amount less what the settlements that are not
reversed settled, [ADR 0030](0030-settlements.md)), oldest due date first. `on` is required: the server does not
know the person's today. Each item has the entry (its id, kind, description, party and category), the
installment number, the due day, the amount, what was settled, what is `open`, and `days_late`. The answer also
carries the **totals of everything that is overdue**, `receivable` and `payable` and the `count`, so that a list
cut at `limit` (500 at most, 100 by default) still says how much is late in all.

### Where the rules live

The sums are made by the entries' port (two methods, `report_entries` and `overdue_installments`), in SQL for
Postgres and by iteration in memory, and the contract tests hold both to the same answers. Nothing is stored.

### Roles

The `reports` segment is in the `finance` area ([ADR 0026](0026-membership-roles-and-permissions.md)) and
only reads.

## Consequences

- A person can see where the money goes by category, by cost center and by customer or supplier, over any span,
  and what is late, with no other tool.
- A report by competence does not move when an entry is settled; it moves only when an entry is changed.
- Reports by settled cash, per account, and exports (CSV, PDF) are not here.

## Revisit when

A report has to be exported or scheduled, a by-category report has to roll subcategories up into their parents
([ADR 0012](0012-managerial-accounting-model.md)), or a span is long enough that these sums need an index of
their own.
