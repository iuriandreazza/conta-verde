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

- **The data:** `scripts/perf/seed.sql`, deterministic: one large company (3,000 parties of which 60 are sellers, 800 products, 120,000 entries with 1 to 3 installments (about 250,000), 190,000 settlements spread over its
  bank accounts, 70,000 sales of three lines with a seller, 240,000 stock movements in and out of every product, 1,000,000 audit rows, 8 bank accounts, five years) and three small companies, 640 MB.
- **The server:** the API as the runtime role, with the row-level policies on ([ADR 0093](../adr/0093-row-level-security.md)), against PostgreSQL 17 in a container on a laptop. One request at a time:
  this is latency, not load. Every number is **warm** (a warm-up call, then 20 timed; "p95" is the nearest rank, the second slowest of the 20).
- **Before and after on the same data:** the database is made by the binary of `main` before this change (`scripts/perf/prepare.sh` with `API_BIN`), measured with it, and then the binary of this change is started on
  the same database (it migrates: its migration builds the index) and measured again.
- **The budget:** p95 of a keyset page of a list ≤ **200 ms**; of a report, the DRE, the cash flow or a dashboard card ≤ **1,500 ms**.

```bash
scripts/ops/docker-pg-tools.sh /tmp/pg-tools && export PATH=/tmp/pg-tools:$PATH       # PostgreSQL 17 tools without installing them
OWNER_BASE_URL=postgres://conta_verde:...@127.0.0.1:5432 APP_BASE_URL=postgres://conta_verde_app:...@127.0.0.1:5432 DB=perf scripts/perf/prepare.sh
DATABASE_URL=$APP_BASE_URL/perf DATABASE_MIGRATE_URL=$OWNER_BASE_URL/perf BIND_ADDR=127.0.0.1:3193 target/debug/conta-verde &
python3 scripts/perf/bench.py http://127.0.0.1:3193 --runs 20
```

To see where an endpoint spends its time, load `auto_explain` (`session_preload_libraries`, `auto_explain.log_min_duration = 20`, `log_analyze = on`) on the benchmark database for one call and read the plans as the application
really runs them: **a plan checked in `psql` with a constant is not always the plan of the application's prepared statement** (the entries list was found this way: 0.2 ms in `psql`, 60 ms in the application).

## Before and after

| endpoint | before: median | before: p95 | after: median | after: p95 | budget | after |
|---|---:|---:|---:|---:|---:|---|
| entries: first page | 67.4 | 70.0 | 5.2 | 7.0 | 200 | ok |
| entries: first page, receivables | 49.1 | 52.6 | 5.0 | 5.8 | 200 | ok |
| entries: search | 42.7 | 44.6 | 42.7 | 45.4 | 200 | ok |
| entries: oldest first | 66.9 | 69.5 | 6.0 | 6.9 | 200 | ok |
| entry: one, with its installments | 2.6 | 3.1 | 2.9 | 3.6 | 200 | ok |
| entry: its settlements | 2.0 | 2.2 | 2.0 | 2.5 | 200 | ok |
| entry: its history | 3.0 | 3.4 | 2.2 | 2.8 | 200 | ok |
| parties: first page | 4.2 | 5.4 | 3.7 | 4.4 | 200 | ok |
| parties: search | 3.3 | 4.7 | 3.3 | 4.2 | 200 | ok |
| products: first page | 2.7 | 3.1 | 2.7 | 3.0 | 200 | ok |
| sales: first page | 5.8 | 6.6 | 5.5 | 6.4 | 200 | ok |
| sales: completed, newest | 6.1 | 7.8 | 5.3 | 6.5 | 200 | ok |
| stock: positions | 7.1 | 8.5 | 7.1 | 8.1 | 200 | ok |
| stock: position tiles | 77.6 | 81.5 | 79.9 | 86.0 | 1500 | ok |
| stock: movements | 83.4 | 85.9 | 2.8 | 3.2 | 200 | ok |
| stock: movements of a product | 3.2 | 4.6 | 2.7 | 3.0 | 200 | ok |
| stock: alerts | 80.1 | 83.5 | 80.5 | 85.7 | 1500 | ok |
| balances (dashboard) | 266.5 | 373.2 | 272.7 | 387.4 | 1500 | ok |
| cash flow, a year by month | 1381.5 | 1523.0 | 1509.1 | 1593.9 | 1500 | OVER |
| cash flow, 90 days by day | 1168.6 | 1223.8 | 1316.2 | 1444.6 | 1500 | ok |
| DRE, a year by month | 236.7 | 242.9 | 130.1 | 135.4 | 1500 | ok |
| report: entries by category, a year | 227.6 | 240.0 | 120.7 | 133.4 | 1500 | ok |
| report: entries by party, five years | 247.1 | 352.6 | 237.5 | 485.5 | 1500 | ok |
| report: overdue | 951.1 | 993.2 | 776.4 | 804.1 | 1500 | ok |
| delinquency | 808.2 | 862.7 | 728.8 | 800.0 | 1500 | ok |
| sales report, a month by customer | 50.5 | 53.0 | 47.8 | 56.0 | 1500 | ok |
| sales report, a month by product | 49.9 | 52.0 | 48.4 | 52.7 | 1500 | ok |
| stock valuation | 108.7 | 115.5 | 107.9 | 116.2 | 1500 | ok |
| stock movement summary, a year | 223.1 | 241.5 | 226.8 | 239.0 | 1500 | ok |
| commissions, a month | 50.5 | 53.4 | 51.2 | 54.2 | 1500 | ok |

After, keyset depth: 30 pages of the entries list, the first three 5.6 ms (median), the last three 4.8 ms
Before, keyset depth: 30 pages of the entries list, the first three 81.8 ms (median), the last three 64.1 ms

## What was found and changed

1. **The keyset lists ordered by an output column** (entries, stock movements, account transfers, commission rules, received invoices, statement lines, and the accountant's settlements export): the page read and sorted every
   row of the company because `ORDER BY competence_date` named the *text* the `SELECT` outputs under that name, not the indexed date. Fixed by ordering by the table's own column. **Measured:** the entries page went
   from 67.4 ms to 5.2 ms (13×), the stock ledger from 83.4 ms to 2.8 ms (29×), and the cost of a page no longer depends on how
   far along the list it is. The other four have the same pattern and the same fix **and were not measured** (the benchmark seeds no transfers, commission rules, received invoices or statement lines). No index was
   added for this: the index was there and unused.
2. **One index** ([migration `20270112000000`](../../crates/infrastructure/migrations/20270112000000_performance_indexes.sql)): `financial_entry_installments (company_id, due_date) INCLUDE (entry_id, number, amount)`. The DRE and
   the entries report went from about 236.7 to 130.1 ms and the overdue list from 951.1 to 776.4 ms.
3. **Tried and not kept** (measured, no gain): an index on the settlements by bank account (the balance of an account took 27 ms without it and 24 ms with it: the join to the 120,000 entries dominates, not the settlements), and a
   covering `financial_entries (company_id, id) INCLUDE (kind, voided)` for that join (10 to 15 % on the cash flow, not worth a third index on the table written most).

## What is still over its budget, and why

- **The cash flow: p95 1593.9 ms for a year by month and 1444.6 ms for 90 days by day, against 1,500** (it was 1,523 and 1,224 before: the index did not improve it, and the 5 to 10 % it measured slower is inside the run-to-run noise seen on this machine). Two queries make it: the open amounts by due day for the span (about 330 ms), and the
  **open amount of everything that fell due before the span** (`past_due`, about 850 ms), which counts, for each installment ever due, what is left of it. What is left is **derived on every read** from the installments and the
  settlements, never stored ([ADR 0030](../adr/0030-settlements.md), [ADR 0031](../adr/0031-transfers-and-balances.md)), so the second one reads about 190,000 installments and 180,000 settlements: an index cannot make that smaller. Rewriting the
  per-installment subquery as one aggregation of the settlements was measured at 246 ms against 417 ms for that one query; it was not made, because the same pattern is in the overdue list, the delinquency
  report and the cash flow and deserves its own change and its own contract checks. Other ways out: keep the open balance of each installment (breaking the "derived, never stored" rule), or keep a per-month snapshot
  of what is open.
- **The overdue list (776.4 ms) and the delinquency report (728.8 ms)** are inside the budget and made of the same read: they will be the next to cross it.
- **Position tiles and the minimum-stock alerts (about 80 ms each)** read every product's balance; fine here, linear in the number of products.

## A stress run: 24 bank accounts

The same data with three times the bank accounts (made by the new binary; nothing to compare it with, so no "before"):

| endpoint | median ms | p95 ms | budget ms | |
|---|---:|---:|---:|---|
| balances (dashboard) | 558.3 | 577.8 | 1500 | ok |
| cash flow, a year by month | 2008.6 | 2109.9 | 1500 | OVER |
| cash flow, 90 days by day | 1679.4 | 1760.3 | 1500 | OVER |
| report: overdue | 764.7 | 786.0 | 1500 | ok |
| delinquency | 696.4 | 714.6 | 1500 | ok |

## Limits of the design this showed

- The **sales report and the commissions refuse a span with more than 2,000 sales** ([ADR 0043](../adr/0043-sales-reports-and-dashboard.md)). This company makes about 38 a day, so neither can be asked for a year (the benchmark asks for a month, about 1,200 sales).
  That is a decision, not an index problem.
- A search on the entries list (`q`) takes about 42.7 ms: `strpos` over the descriptions of the company's entries, a scan. Fine at 120,000 entries, linear after.

## Not measured

Concurrency, another PostgreSQL version or machine, a cold cache, the hourly passes and the job queue under backlog, the accountant's package of a big span (built in memory, [ADR 0071](../adr/0071-accountant-exports.md)),
the PDF of a quote, the web app's own rendering, and the lists of transfers, commission rules, received invoices and statement lines (the data has none). See ADR 0097.
