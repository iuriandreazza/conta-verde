---
title: "ADR 0012: Managerial chart of accounts, not a general ledger"
status: accepted
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

## Implementation notes

First slice (`feat/categories-cost-centers`): the registers only. Entries, reports and the DRE come with Phases 2 and 6, and
the rules below are what they will rely on. Where this differs from the decision above, the decision stays the target.

**Groups.** The DRE group is one of `gross_revenue`, `deductions`, `costs`, `operating_expenses`, `financial_result`,
`investments`, `outside_result` (loans and owner capital: cash that moves but is not a result) and `transfers`; the cash-flow
group is one of `operating`, `investing`, `financing` and `transfer`. The keys are stored (a `CHECK` and the audit trail), so they
do not change; the Portuguese labels live in the web app. `transfers` and `transfer` go together.

**The tree.**
- A category has a name, an optional parent, its two groups and an active flag. Names are unique among siblings, ignoring case, and
  among the roots too. The tree is at most five levels deep.
- Only a root chooses the groups. A subcategory inherits both from its parent; the server sets them and a request cannot. They are
  stored on every row, so a report reads a column instead of walking the tree.
- **The parent and the groups are fixed once the category exists.** Moving a category needs cycle and depth checks over its
  subtree, and changing a group has to cascade over it, one audit entry per row. Until entries exist, "deactivate it and create the
  right one" is the answer; both are in "Revisit when".
- Deactivating a category with active subcategories is refused (`409`), and so is adding a subcategory to, or reactivating one under,
  an inactive category: an active category is never under an inactive one. There is no delete; entries will reference categories.
- **The transfer category** is the root whose group is `transfers`. There is at most one per company (a unique index), it has no
  subcategories and cannot be deactivated, but can be renamed, so Phase 2 finds it by its group and never by its name.
- **The default template** (revenue, deductions, costs, operating expenses, financial result, investments, loans and capital, and
  transfers, with their usual subcategories) is applied on request, only to a company that has no categories, and writes every
  category and its audit entry in one transaction. It is not part of signing up, which would tie accounts to categories; the empty
  page offers it.
- The list of categories is the whole tree as a flat list with the parent id, in a fixed order, not a page: a tree has to be drawn
  whole, and a company has dozens of categories, not thousands. It is the one list that does not follow
  [ADR 0018](0018-list-endpoints-search-filter-sort.md)'s paging.

**Cost centers** are a flat register: a name, unique per company ignoring case, and an active flag, listed as ADR 0018 says.
They are not a tree for now.

## Revisit when

Moving a category, or changing its groups, becomes necessary once entries exist (it then needs a rule for the entries already
classified), or cost centers need a hierarchy. And, as before:


Accountants ask for statutory reports (balance sheet, trial balance) inside the product;
that would be a new ADR introducing a ledger context.
