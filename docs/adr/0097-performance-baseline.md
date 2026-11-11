---
title: "ADR 0097: A performance baseline on five years of data, and what it changed"
status: accepted
date: 2026-11-11
tags: [adr, performance, postgres, phase-9]
---

# ADR 0097: A performance baseline on five years of data, and what it changed

## Context

[ADR 0092](0092-one-point-zero-hardening-plan.md) asked for a baseline on a five-year dataset and an index review, with a stated budget, and said an
index is added only when a recorded plan justifies it. Until now no list, report or dashboard card had been timed on more than a few
rows: the tests use tens of records. Two things made the question worth asking before 1.0: every query now pays the row-level policy
([ADR 0093](0093-row-level-security.md)), and every list is keyset-paginated ([ADR 0018](0018-list-endpoints-search-filter-sort.md)), a promise that a page costs the same however far
along the list it is, which nothing had ever checked.

## Decision

### The dataset and the tools

`scripts/perf/seed.sql` makes, **directly in SQL as the schema's owner** and deterministically (every id is an md5 of what the row is, nothing is
random), one large company and three small ones (so the planner sees other tenants and the policies have something to hide): per large company
3,000 parties, 800 products, 120,000 entries with 1 to 3 installments (about 250,000), 190,000 settlements, 70,000 sales with three lines, 240,000 stock
movements, 1,000,000 audit rows and 8 bank accounts (a parameter), spread over five years; 640 MB. It runs in 25 seconds, which is why it is SQL and not a
generator through the API (a five-year history cannot be made through the API in a reasonable time) and why it lives in `scripts/`, not in the application.
`scripts/perf/prepare.sh` makes the database the way a real one is made (migrated by the server itself, as the owner, with the runtime role and the
policies), registers the user that owns the large company, and seeds. `scripts/perf/bench.py` signs in and times 30 endpoints (the lists, one record,
the reports, the DRE, the cash flow, the dashboard's sources, the stock reports) 20 times each after a warm-up, and the depth of a keyset list.

### The budget

On the large company, measured at the client on the server's own machine, **p95 of 20 runs**: a **keyset page of a list ≤ 200 ms** and **every report, the DRE, the
cash flow and each dashboard card ≤ 1,500 ms**. This is a baseline to compare against and a number to notice a regression with, **not a service level**: one
machine, one PostgreSQL version, synthetic data, one request at a time (no load).

### What it found

1. **The entries list read and sorted the whole company** (67 ms), and the stock movements list too (81 ms), the transfers and the commission rules' lists likewise.
   Their `SELECT` outputs the date as text under the column's own name (`to_char(competence_date, ...) AS competence_date`), and in PostgreSQL an unqualified
   `ORDER BY competence_date` resolves to the **output column**, so the planner sorted by an expression no index serves: a parallel sequential scan of every entry of the
   company, then a top-N sort. The keyset's `WHERE (competence_date, id) < ...` was right (a `WHERE` cannot see output names); the `ORDER BY` was not. The fix is to order by the
   table's own column (`financial_entries.competence_date`), in the five lists that had the pattern, with a comment saying why. The entries first page is now **5 ms**, the movements **2 ms**, and
   the 30th page of the entries list costs what the first does. No index was needed: the index that existed was never used.
2. **Two indexes**, each for the queries named in its migration (`20270112000000_performance_indexes.sql`):
   - `financial_entry_installments (company_id, due_date) INCLUDE (entry_id, number, amount)`: the cash flow's forecast, the overdue list, the delinquency report and the reports on the
     due basis all read "this company's installments due between two days", and the table's only index was its primary key, so each read every installment of every company.
   - `entry_settlements (company_id, bank_account_id, settled_on) WHERE NOT reversed`: the balance of an account and the cash flow's realized part read one account's settlements by day;
     the existing indexes start with `(company_id, settled_on)` or `(company_id, entry_id)`.
3. **Tried and not kept**: a covering `financial_entries (company_id, id) INCLUDE (kind, voided)` for the settlement join. It made the cash flow 10 to 15 % faster, which does not pay for a third
   index on the biggest table written on every entry.

### Where it stands

The tables are in [`docs/performance/baseline.md`](../performance/baseline.md): before and after, for 8 bank accounts and for a 24-account stress. **Everything is inside its budget except the cash flow of a
year by month** (p95 about 1.6 s against 1.5 s; the 90-day view is at the line). It is made of two queries that recompute, for every installment ever due, what is left of it: the balance of an installment is
**derived on every read** and never stored ([ADR 0030](0030-settlements.md)), so the cost is the history, and no index shrinks it. The number of bank accounts does not matter (8 and 24 cost the same). One
rewrite (aggregating the settlements once instead of a lookup per installment) was measured at 246 ms against 417 ms for the heaviest query and **not made**: the same pattern is in the overdue list and the
delinquency report, and it deserves its own change. The baseline records the cash flow as over budget instead of moving the budget.

## Consequences

- A new keyset list must order by the table's own column whenever its `SELECT` outputs that column under the same name (the comment at each of the five places says it). Nothing automatic catches it:
  a plan test would need a big table. The baseline does: `bench.py` shows a first page above a few milliseconds on 120,000 rows at once.
- The two indexes add write cost to installments and settlements (an insert maintains one more index each) and a few percent of their size. A migration that builds an index locks the table against writes for
  the seconds it takes on a big installation (the build runs inside the migration's transaction), which the upgrade notes of the release that carries it say.
- The policy's cost ([ADR 0093](0093-row-level-security.md): 0.04 to 0.1 ms a statement) is inside every number here: the baseline is measured as the runtime role, with the policies on.
- The sales report and the commissions refuse a span with more than 2,000 sales ([ADR 0043](0043-sales-reports-and-dashboard.md), [ADR 0041](0041-commissions.md)); the benchmark asks them for a month. A company that sells 70,000 times in five years
  cannot ask either of them for a year. That is a limit of the design, not of the indexes, and the baseline records it.

## Not measured

Concurrent requests (one request at a time), a different PostgreSQL version or machine, a cold cache (every number is warm), the hourly passes and the job queue under a large backlog, the accountant's
package of a big span (built in memory, [ADR 0071](0071-accountant-exports.md)), the PDF of a quote, the web app's own rendering of a page of 200 rows, and a company with ten times these sizes.

## Revisit when

A company grows past these sizes, the cash flow is asked for by a company with many accounts, or a change touches a list's `SELECT` or `ORDER BY`.
