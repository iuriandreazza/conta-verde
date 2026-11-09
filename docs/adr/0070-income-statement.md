---
title: "ADR 0070: The managerial income statement (DRE)"
status: accepted
date: 2026-11-09
tags: [adr, reporting, finance, phase-6]
---

# ADR 0070: The managerial income statement (DRE)

## Context

[ADR 0012](0012-managerial-accounting-model.md) decided that the DRE is derived from the entries by competence date, grouped
by the DRE group of each category and optionally by cost center, and [ADR 0067](0067-accountant-and-reporting-plan.md)
fixed its sign and its lines. This ADR is how it is read and drawn. Nothing is stored: a closed month
([ADR 0069](0069-period-lock.md)) gives the same figures every time.

## Decision

- **Rows from the entries' port.** `FinancialEntryRepository::competence_totals(company, since, until)` returns what the
  entries that are not cancelled, with a competence date in the span, add up to **by their total** (the sum of their
  installments, never stored) for each calendar month, category and cost center: receivable and payable apart, ordered by
  month, category and cost center with the entries that have none first. Postgres does it in one `GROUP BY`; the in-memory
  adapter walks the entries; the contract `reports_the_income_statement` holds both to the same figures.
- **The groups come from the categories**, composed in `DreService` (the entries' port and the categories' port, like the
  cash flow composes its ports), so no adapter knows what a DRE group is. A row whose category is unknown is left out; the
  `transfers` group is never added (it has no entries anyway: the transfer category cannot be chosen for an entry).
- **Domain.** `DreLines` holds the seven groups as signed decimals (a receivable positive, a payable negative) and works the
  four subtotals out: net revenue = gross revenue + deductions; gross profit = net revenue + costs; operating result = gross
  profit + operating expenses; net result = operating result + financial result. `investments` and `outside_result` (loans and
  capital) are **below the line**: shown, but not in any subtotal.
- **Columns.** `by=month` gives one column per calendar month the span touches, **an empty month included as a column of
  zeros** (so the chart is continuous); `by=cost_center` gives a column per cost center that has entries and one for the
  entries that have none, last. Either is narrowed by `cost_center=<id>` or `cost_center=none`. The total of the columns is
  the total of the statement. The span is by **days** (`from` and `to` are both required: the server reads no clock), so the
  first and last months may be partial; at most 24 calendar months, else `400`.
- **Route.** `GET /companies/{id}/dre?from=&to=&by=&cost_center=` in the `finance` area (a read for every role that reaches
  the company). Amounts are decimal strings; a column's key is `YYYY-MM`, a cost center's id, or `null`.
- **Web.** A **DRE** view on the Relatórios page (period filter, columns, cost center), a table of every line with the
  subtotals marked and what is below the line set apart, a bar chart of net revenue against costs and expenses per column
  (with the table that is its accessible twin), a CSV export of the table and print. The page says plainly that the
  statement is managerial: it follows the competence dates and the classification of the entries, and does not replace the
  accounting statement.

## Consequences

- The DRE needs no new table and stays right when entries change (until a month is closed, after which they cannot).
- An entry counts by its **total in the month of its competence date**, whatever the installments say or when they are paid:
  cash is the cash flow's ([ADR 0032](0032-cash-flow.md)).
- A refund booked as a payable in a revenue category lowers revenue, as [ADR 0067](0067-accountant-and-reporting-plan.md) says.

## Known limits

- One level: a subcategory counts in the group of its root, but the statement does not break a group down by category; the
  entries report ([ADR 0035](0035-entry-reports.md)) does.
- The chart draws magnitudes (net revenue, and costs plus operating expenses), so a loss shows as costs above revenue; the
  signed result is in the table.
- No comparison with a previous period and no percentages of revenue.

## Revisit when

An accountant wants the statement broken down by category inside each group, a comparison column, or the cash basis.
