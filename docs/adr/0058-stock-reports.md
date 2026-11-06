---
title: "ADR 0058: Stock reports: position, movements and valuation"
status: accepted
date: 2026-11-06
tags: [adr, inventory, reports, phase-5]
---

# ADR 0058: Stock reports: position, movements and valuation

## Context

The ledger ([ADR 0054](0054-stock-movements.md)) holds the truth; people need to read it as a table they can act on: what
is on the shelf and what it is worth, on a day they choose, and what happened over a period. They are reads: nothing is
stored, and the server reads no clock, so every day is the client's.

## Decision

All three are derived on every read, in the `purchases` area ([ADR 0052](0052-purchases-and-inventory-plan.md)), from the
products' port and the movements' port, never a query across tables.

- **Position** is `GET /stock/positions?on=` (ADR 0054): each product with its quantity, average cost and value, through a
  day. It lists every product of kind product, including those with nothing on hand.
- **Valuation** is `GET /stock-reports/valuation?on=YYYY-MM-DD` (`on` required): the products that had something on hand or
  some value through that day, **biggest value first** (then by name), each with quantity, average cost, value and its
  **share** of the total (percent, at most two decimals, 0 when the total is not above 0), and the **total value** in cents.
  The count of products with a negative balance is reported beside it, because their value is 0 and the total does not say
  they are short.
- **Movements** is `GET /stock-reports/movements?from=&to=` (both required, the days included, **at most 366 days**,
  `400` otherwise): for each product that moved in the span, by name, what it had the day before (`opening`), what came in,
  what went out (as positive numbers) and what counts and adjustments added (signed), and what it had at the end
  (`closing`). A movement that undoes another counts under the opposite kind (a reversal of an exit is an entry), so the
  columns add up to the closing balance: opening + entered - exited + adjusted = closing, in quantity and in value. The
  sums are made by the movements' port (SQL `SUM ... FILTER` by kind, and in memory), with the balances through `from - 1`
  and `to`.

The web page "Relatórios de estoque" shows the valuation as a bar chart of the biggest values with the table and the total,
and the movements as a table with the opening, in, out, adjustment and closing columns, using the report components and the
tokens of `docs/design/`.

## Consequences

- The balances the valuation reads are those of a day, so a backdated movement changes the past; what a sale took
  stays what it took (it was valued when written), so the report of a past day is what the ledger says now, not what the
  screen said then.
- Valuation scans the products 200 at a time and reads their balances in chunks of that size; a company with tens of
  thousands of products pays for it on every read.

## Revisit when

A report has to be exported, grouped by category or by brand, valued at a cost other than the average, or kept as a snapshot
so that an accountant sees the same figures later (the period lock of Phase 6).
