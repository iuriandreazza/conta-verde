---
title: Architecture decision records
status: living
date: 2026-11-04
tags: [adr]
---

# Architecture decision records

Each ADR records one decision that is expensive to reverse: the context that forced it,
the options that were considered, the choice, and its consequences. ADRs are never edited
to change a decision; a new ADR supersedes the old one and both link to each other.

Status values: `proposed` (agreed direction, accepted when its phase starts),
`accepted`, `superseded by NNNN`, `rejected`.

| ADR | Title | Status | Phase |
|-----|-------|--------|-------|
| [0001](0001-license-agpl.md) | License the project under AGPL-3.0-or-later | accepted | 0 |
| [0002](0002-layered-cargo-workspace.md) | Layered Cargo workspace | accepted | 0 |
| [0003](0003-money-as-decimal.md) | Money is an exact decimal | accepted | 0 |
| [0004](0004-company-scoped-tenancy.md) | Company-scoped data from day one | accepted | 0 |
| [0005](0005-spa-frontend.md) | Separate SPA frontend | accepted | 1 |
| [0006](0006-fiscal-last.md) | Fiscal documents come last | accepted | 7 |
| [0007](0007-authentication-and-authorization.md) | Authentication and authorization | accepted | 1 |
| [0008](0008-audit-trail.md) | Audit trail on every business write | accepted | 1 |
| [0009](0009-postgres-only-background-jobs.md) | PostgreSQL is the only infrastructure; jobs run on it | accepted | 2 |
| [0010](0010-payment-providers.md) | Payment providers behind ports | proposed | 4 |
| [0011](0011-bank-data-ingestion.md) | Bank data: OFX first, aggregator later | accepted | 2, 8 |
| [0012](0012-managerial-accounting-model.md) | Managerial chart of accounts, not a general ledger | accepted | 1, 2, 6 |
| [0013](0013-accountant-access-and-exports.md) | Accountant access and exports | proposed | 6 |
| [0014](0014-notifications.md) | Notifications behind ports | accepted | 4 |
| [0015](0015-public-api-and-webhooks.md) | Public API and webhooks | proposed | 8 |
| [0016](0016-ui-motion-and-icons.md) | Motion for animation, vendored Lucide Animated icons | accepted | 1 |
| [0017](0017-cnpj-registry-lookup.md) | CNPJ checked against the federal registry behind a port | accepted | 1 |
| [0018](0018-list-endpoints-search-filter-sort.md) | List endpoints share one search, filter and sort convention | accepted | 1 |
| [0019](0019-products-and-services.md) | Products and services as one register | accepted | 1 |
| [0020](0020-app-shell-charts-and-sample-data.md) | Sidebar app shell, hand-drawn SVG charts and a sample-data boundary | accepted | 1, 2 |
| [0021](0021-bank-accounts-payment-methods-and-terms.md) | Bank accounts, payment methods and payment terms | accepted | 1 |
| [0022](0022-party-roles-address-and-editing.md) | Party roles, address and editing | accepted | 1 |
| [0023](0023-party-contacts-and-tags.md) | Party contacts and tags | accepted | 1 |
| [0024](0024-postal-code-lookup.md) | Postal code (CEP) lookup behind a port | accepted | 1 |
| [0025](0025-csv-import.md) | CSV import of registers | accepted | 1 |
| [0026](0026-membership-roles-and-permissions.md) | Membership roles and what each may do | accepted | 1 |
| [0027](0027-invitations.md) | Invitations | accepted | 1 |
| [0028](0028-member-management.md) | Changing and removing members | accepted | 1 |
| [0029](0029-financial-entries.md) | Financial entries and installments | accepted | 2 |
| [0030](0030-settlements.md) | Settlements of entry installments | accepted | 2 |
| [0031](0031-transfers-and-balances.md) | Transfers, opening balances and balance per account | accepted | 2 |
| [0032](0032-cash-flow.md) | Cash flow, realized and forecast | accepted | 2 |
| [0033](0033-recurring-entries.md) | Recurring entries generated ahead by a scheduled job | accepted | 2 |
| [0034](0034-statement-import-and-reconciliation.md) | Statement import (OFX) and reconciliation | accepted | 2 |
| [0035](0035-entry-reports.md) | Reports of the entries: by group and the overdue list | accepted | 2 |
| [0036](0036-entry-attachments.md) | Files attached to entries, and where their bytes are kept | accepted | 2 |
| [0037](0037-sales-module-plan.md) | The sales module: plan, shared rules and what it changes underneath | accepted | 3 |
| [0038](0038-quotes.md) | Quotes, answered through a public link | accepted | 3 |
| [0039](0039-sales.md) | Orders and sales, their receivables, cancellation and returns | accepted | 3 |

## Template

```markdown
---
title: "ADR NNNN: Short imperative title"
status: proposed
date: YYYY-MM-DD
tags: [adr, area]
---

# ADR NNNN: Short imperative title

## Context
## Options considered
## Decision
## Consequences
## Revisit when
```
