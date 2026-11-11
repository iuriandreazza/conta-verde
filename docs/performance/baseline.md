---
title: Performance baseline
status: living
date: 2026-11-11
tags: [performance, operations]
---

# Performance baseline

What the main screens cost on **five years of a busy company**, how it was measured, what was found and fixed, and what is still over its budget.
The decisions are in [ADR 0097](../adr/0097-performance-baseline.md). This is a baseline to compare against and to notice a regression with, **not a service level**.

## What was measured

- **The data:** `scripts/perf/seed.sql`, deterministic: one large company (3,000 parties, 800 products, 120,000 entries with 1 to 3 installments (about 250,000), 190,000 settlements,
  70,000 sales of three lines, 240,000 stock movements, 1,000,000 audit rows, 8 bank accounts, five years) and three small ones, 640 MB.
- **The server:** the API as the runtime role, with the row-level policies on ([ADR 0093](../adr/0093-row-level-security.md)), against PostgreSQL 17 in a container on a laptop. One request at a time:
  this is latency, not load. Every number is **warm** (a warm-up call, then 20 timed).
- **The budget:** p95 of a keyset page of a list ≤ **200 ms**; of a report, the DRE, the cash flow or a dashboard card ≤ **1,500 ms**.

```bash
scripts/ops/docker-pg-tools.sh /tmp/pg-tools && export PATH=/tmp/pg-tools:$PATH       # PostgreSQL 17 tools without installing them
OWNER_BASE_URL=postgres://conta_verde:...@127.0.0.1:5432 APP_BASE_URL=postgres://conta_verde_app:...@127.0.0.1:5432 DB=perf scripts/perf/prepare.sh
DATABASE_URL=$APP_BASE_URL/perf DATABASE_MIGRATE_URL=$OWNER_BASE_URL/perf BIND_ADDR=127.0.0.1:3193 target/debug/conta-verde &
python3 scripts/perf/bench.py http://127.0.0.1:3193 --runs 20
```

To see where an endpoint spends its time, set `log_min_duration_statement = 0` (or load `auto_explain` with `log_analyze`) on the benchmark database for one call and read the plans as the application
really runs them: a plan checked in `psql` with a constant is not always the plan of the application's prepared statement.

## Before: what the first run showed (no changes)

The same data (with 24 bank accounts), the code and the indexes as they were on `main`:

| endpoint | median ms | p95 ms | budget ms | |
|---|---:|---:|---:|---|
| entries: first page | 66.8 | 77.1 | 200 | ok, but 30× what it should cost |
| stock: movements | 81.5 | 86.3 | 200 | ok, but 30× what it should cost |
| balances (dashboard) | 482.6 | 566.0 | 1500 | ok |
| cash flow, a year by month | 1718.3 | 1819.2 | 1500 | **over** |
| cash flow, 90 days by day | 1513.5 | 1660.6 | 1500 | **over** |
| DRE, a year by month | 230.5 | 253.7 | 1500 | ok |
| report: entries by category, a year | 216.3 | 234.6 | 1500 | ok |
| report: overdue | 903.0 | 930.8 | 1500 | ok |
| delinquency | 767.9 | 811.2 | 1500 | ok |

## What was found and changed

1. **The five keyset lists ordered by an output column** (entries, stock movements, account transfers, commission rules, received invoices): the page read and sorted every row of the company
   because `ORDER BY competence_date` named the *text* the `SELECT` outputs under that name, not the indexed date. Fixed by ordering by the table's own column. The entries page went from 67 ms to
   **5 ms**, the ledger from 81 ms to **2.5 ms**, and the cost of a page no longer depends on how far along the list it is (below). No index was added: the index was there and unused.
2. **Two indexes** ([migration `20270112000000`](../../crates/infrastructure/migrations/20270112000000_performance_indexes.sql)): `financial_entry_installments (company_id, due_date) INCLUDE (entry_id, number, amount)` and
   `entry_settlements (company_id, bank_account_id, settled_on) WHERE NOT reversed`. The balances went from 483 to 226 ms, the DRE and the entries report from ~220 to ~125 ms, the overdue list from 903 to 796 ms.
3. **Tried and not kept:** a covering `financial_entries (company_id, id) INCLUDE (kind, voided)`: 10 to 15 % on the cash flow, not worth a third index on the table written most.

## After: the baseline (8 bank accounts)

| endpoint | median ms | p95 ms | worst ms | budget ms | |
|---|---:|---:|---:|---:|---|
| entries: first page | 5.4 | 8.2 | 8.2 | 200 | ok |
| entries: first page, receivables | 4.9 | 6.0 | 6.0 | 200 | ok |
| entries: search | 43.1 | 45.4 | 45.4 | 200 | ok |
| entries: oldest first | 5.6 | 6.6 | 6.6 | 200 | ok |
| entry: one, with its installments | 2.8 | 3.6 | 3.6 | 200 | ok |
| entry: its settlements | 2.0 | 2.7 | 2.7 | 200 | ok |
| entry: its history | 2.2 | 3.1 | 3.1 | 200 | ok |
| parties: first page | 3.3 | 4.7 | 4.7 | 200 | ok |
| parties: search | 3.2 | 4.4 | 4.4 | 200 | ok |
| products: first page | 2.8 | 3.6 | 3.6 | 200 | ok |
| sales: first page | 5.7 | 7.1 | 7.1 | 200 | ok |
| sales: completed, newest | 5.9 | 6.9 | 6.9 | 200 | ok |
| stock: positions | 7.4 | 8.5 | 8.5 | 200 | ok |
| stock: position tiles | 103.5 | 115.0 | 115.0 | 1500 | ok |
| stock: movements | 2.7 | 3.6 | 3.6 | 200 | ok |
| stock: movements of a product | 2.7 | 3.7 | 3.7 | 200 | ok |
| stock: alerts | 103.5 | 110.5 | 110.5 | 1500 | ok |
| balances (dashboard) | 226.1 | 233.1 | 233.1 | 1500 | ok |
| cash flow, a year by month | 1453.6 | 1611.3 | 1611.3 | 1500 | OVER |
| cash flow, 90 days by day | 1329.3 | 1446.4 | 1446.4 | 1500 | ok |
| DRE, a year by month | 124.6 | 132.1 | 132.1 | 1500 | ok |
| report: entries by category, a year | 120.9 | 137.9 | 137.9 | 1500 | ok |
| report: entries by party, five years | 241.4 | 270.0 | 270.0 | 1500 | ok |
| report: overdue | 795.7 | 900.0 | 900.0 | 1500 | ok |
| delinquency | 727.6 | 769.9 | 769.9 | 1500 | ok |
| sales report, a month by customer | 49.4 | 53.7 | 53.7 | 1500 | ok |
| sales report, a month by product | 48.4 | 51.2 | 51.2 | 1500 | ok |
| stock valuation | 132.7 | 149.7 | 149.7 | 1500 | ok |
| stock movement summary, a year | 269.6 | 284.2 | 284.2 | 1500 | ok |
| commissions, a month | 2.6 | 3.6 | 3.6 | 1500 | ok |

keyset depth: 30 pages of the entries list, the first three 5.1 ms (median), the last three 5.3 ms

## A stress run: 24 bank accounts

The same data with three times the bank accounts. The cost does **not** follow the number of accounts (the balances took 226 ms with 8 and 235.7 ms with 24):

| endpoint | median ms | p95 ms | worst ms | budget ms | |
|---|---:|---:|---:|---:|---|
| entries: first page | 4.9 | 7.0 | 7.0 | 200 | ok |
| entries: first page, receivables | 4.9 | 5.7 | 5.7 | 200 | ok |
| entries: search | 43.0 | 47.3 | 47.3 | 200 | ok |
| entries: oldest first | 5.5 | 6.9 | 6.9 | 200 | ok |
| entry: one, with its installments | 2.5 | 3.8 | 3.8 | 200 | ok |
| entry: its settlements | 1.9 | 2.5 | 2.5 | 200 | ok |
| entry: its history | 2.1 | 4.5 | 4.5 | 200 | ok |
| parties: first page | 3.4 | 4.0 | 4.0 | 200 | ok |
| parties: search | 2.9 | 3.4 | 3.4 | 200 | ok |
| products: first page | 2.6 | 3.0 | 3.0 | 200 | ok |
| sales: first page | 5.4 | 6.3 | 6.3 | 200 | ok |
| sales: completed, newest | 5.2 | 6.0 | 6.0 | 200 | ok |
| stock: positions | 7.0 | 9.1 | 9.1 | 200 | ok |
| stock: position tiles | 98.1 | 120.8 | 120.8 | 1500 | ok |
| stock: movements | 2.5 | 2.8 | 2.8 | 200 | ok |
| stock: movements of a product | 2.6 | 3.3 | 3.3 | 200 | ok |
| stock: alerts | 100.5 | 112.2 | 112.2 | 1500 | ok |
| balances (dashboard) | 235.7 | 291.3 | 291.3 | 1500 | ok |
| cash flow, a year by month | 1418.4 | 1658.0 | 1658.0 | 1500 | OVER |
| cash flow, 90 days by day | 1461.8 | 1758.2 | 1758.2 | 1500 | OVER |
| DRE, a year by month | 120.0 | 130.7 | 130.7 | 1500 | ok |
| report: entries by category, a year | 119.1 | 141.1 | 141.1 | 1500 | ok |
| report: entries by party, five years | 235.3 | 499.9 | 499.9 | 1500 | ok |
| report: overdue | 775.5 | 813.7 | 813.7 | 1500 | ok |
| delinquency | 710.2 | 1040.8 | 1040.8 | 1500 | ok |
| sales report, a month by customer | 49.6 | 53.9 | 53.9 | 1500 | ok |
| sales report, a month by product | 51.0 | 57.0 | 57.0 | 1500 | ok |
| stock valuation | 138.1 | 159.1 | 159.1 | 1500 | ok |
| stock movement summary, a year | 270.5 | 291.3 | 291.3 | 1500 | ok |
| commissions, a month | 2.7 | 4.4 | 4.4 | 1500 | ok |

keyset depth: 30 pages of the entries list, the first three 5.7 ms (median), the last three 4.9 ms

## What is still over its budget, and why

- **The cash flow of a year by month: p95 about 1.6 s against 1.5 s** (the 90-day view is at the line). Two queries make it: the open amounts by due day for the span (about 330 ms), and the
  **open amount of everything that fell due before the span** (`past_due`, about 850 ms), which counts, for each installment ever due, what is left of it. What is left is **derived on every read** from the installments and the
  settlements, never stored ([ADR 0030](../adr/0030-settlements.md), [ADR 0031](../adr/0031-transfers-and-balances.md)), so the second one reads about 190,000 installments and 180,000 settlements. An index cannot make that smaller. Rewriting the
  per-installment subquery as one aggregation of the settlements was measured at 246 ms against 417 ms for that one query; it was not made, because the same pattern is in the overdue list, the delinquency
  report and the cash flow and deserves its own change and its own contract checks. Other ways out: keep the open balance of each installment (breaking the "derived, never stored" rule), or keep a per-month snapshot
  of what is open.
- **The overdue list (about 0.8 s) and the delinquency report (about 0.75 s)** are inside the budget and made of the same read: they will be the next to cross it.
- **Position tiles and the minimum-stock alerts (about 100 ms each)** read every product's balance; fine here, linear in the number of products.

## Limits of the design this showed

- The **sales report and the commissions refuse a span with more than 2,000 sales** ([ADR 0043](../adr/0043-sales-reports-and-dashboard.md)). This company makes about 38 a day, so neither can be asked for a year (the benchmark asks for a month).
  That is a decision, not an index problem.
- A search on the entries list (`q`) takes about **43 ms**: `strpos` over the descriptions of the company's entries, a scan. Fine at 120,000 entries, linear after.

## Not measured

Concurrency, another PostgreSQL version or machine, a cold cache, the hourly passes and the job queue under backlog, the accountant's package of a big span (built in memory, [ADR 0071](../adr/0071-accountant-exports.md)),
the PDF of a quote, and the web app's own rendering. See ADR 0097.
