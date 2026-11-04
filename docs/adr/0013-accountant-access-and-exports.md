---
title: "ADR 0013: Accountant access and exports"
status: proposed
date: 2026-11-04
tags: [adr, accounting, integrations]
---

# ADR 0013: Accountant access and exports

## Context

Every small business in Brazil has an external accountant who needs the month's
movements and fiscal documents. The reference product offers an accountant login with
read access and integrations with accounting software. We have no commercial agreements
with accounting software vendors and their import formats are proprietary or semi-public.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Accountant uses the owner's login | Nothing to build | No audit, no read-only guarantee |
| Accountant role + file exports (chosen) | Covers the real workflow; vendor-neutral | The accountant imports files by hand into their software |
| Direct integrations with accounting software | Seamless | Per-vendor formats and agreements; maintenance tail |
| Full statutory accounting inside the product | No export needed | Rejected in [ADR 0012](0012-managerial-accounting-model.md) |

## Decision

- The `accountant` role ([ADR 0007](0007-authentication-and-authorization.md)) is read-only
  across all modules of the companies that invite it, and may set and lift the
  **period lock** (a company-level date before which entries are immutable, with an
  audited reopen).
- Exports per period: entries and settlements (CSV and XLSX with a documented column
  layout), audit log, fiscal XMLs and PDFs as a zip, stock position. A monthly package can
  be scheduled by e-mail ([ADR 0009](0009-postgres-only-background-jobs.md),
  [ADR 0014](0014-notifications.md)).
- Vendor-specific layouts are added as export formats behind one `Exporter` interface only
  when a contributor maintains them and the layout is public.

## Consequences

- Accountants get a stable, documented CSV they can map once in their software.
- Period lock is enforced in `application` for every write on dated records, which touches
  finance, sales, purchases, stock and fiscal.
- No SPED or statutory files are produced by the product.

## Revisit when

An accounting software vendor publishes a stable public import API, or users ask for SPED
Fiscal/Contribuições generation, which would require the fiscal module to be complete.
