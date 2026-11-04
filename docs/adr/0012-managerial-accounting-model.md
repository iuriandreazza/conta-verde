---
title: "ADR 0012: Managerial chart of accounts, not a general ledger"
status: proposed
date: 2026-11-04
tags: [adr, finance, reporting]
---

# ADR 0012: Managerial chart of accounts, not a general ledger

## Context

The reference product gives owners a managerial view (categories, cost centers, cash flow,
a managerial income statement called DRE gerencial) and leaves statutory accounting to the
accountant's own software. Users classify entries by category; nobody books journal
entries. A double-entry general ledger would be more rigorous but is not what the target
user operates.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Full double-entry ledger | Rigorous; statutory reports possible | Users do not think in debits and credits; every feature must post balanced entries; large scope |
| Categories only, no structure | Simplest | DRE and results by cost center cannot be derived reliably |
| Category tree mapped to DRE groups (chosen) | Matches the reference product and user mental model; DRE derived from the same entries | Statutory accounting stays outside the product |

## Decision

Categories form a tree per company with a default template (revenue, deductions, costs,
operating expenses, financial result, investments, transfers) where each leaf carries a
DRE group and a cash-flow group. Every financial entry has exactly one category and an
optional cost center. Transfers between accounts use a reserved category and are excluded
from results.

Reports are derived, never stored as truth:

- Cash flow: by settlement date (realized) and due date (forecast).
- DRE gerencial: by competence date (the entry's period), grouped by DRE group and
  optionally by cost center.
- Results per category, cost center and party.

Materialized daily aggregates may cache reports; they are rebuilt from entries on demand.

Statutory accounting is served by exports ([ADR 0013](0013-accountant-access-and-exports.md)),
not by a ledger inside the product.

## Consequences

- Features that create money movements (sales, purchases, charges, fees) only need to
  pick a category; no posting rules.
- The DRE is managerial: it depends on how users classify, and the product says so in the
  report.
- A future ledger would be a separate bounded context fed by the same entries, not a
  rewrite.

## Revisit when

Accountants ask for statutory reports (balance sheet, trial balance) inside the product;
that would be a new ADR introducing a ledger context.
