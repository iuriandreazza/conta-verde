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
| [0010](0010-payment-providers.md) | Payment providers behind ports | accepted | 4 |
| [0011](0011-bank-data-ingestion.md) | Bank data: OFX first, aggregator later | accepted | 2, 8 |
| [0012](0012-managerial-accounting-model.md) | Managerial chart of accounts, not a general ledger | accepted | 1, 2, 6 |
| [0013](0013-accountant-access-and-exports.md) | Accountant access and exports | accepted | 6 |
| [0014](0014-notifications.md) | Notifications behind ports | accepted | 4 |
| [0015](0015-public-api-and-webhooks.md) | Public API and webhooks (amended by 0086) | accepted | 8 |
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
| [0040](0040-contracts.md) | Recurring contracts that bill themselves | accepted | 3 |
| [0041](0041-commissions.md) | Sellers and commissions | accepted | 3 |
| [0042](0042-price-lists.md) | Price lists | accepted | 3 |
| [0043](0043-sales-reports-and-dashboard.md) | Sales reports and the dashboard | accepted | 3 |
| [0044](0044-collection-module-plan.md) | The collection module: plan, shared rules and what it changes underneath | accepted | 4 |
| [0045](0045-charges.md) | Charges for installments, made at a payment provider | accepted | 4 |
| [0046](0046-webhooks-and-automatic-settlement.md) | Provider webhooks settle the installment and enter the fee | accepted | 4 |
| [0047](0047-payment-page.md) | The customer's payment page | accepted | 4 |
| [0048](0048-notifications-and-dunning.md) | Reminders to customers: the dunning plan and its hourly pass | accepted | 4 |
| [0049](0049-asaas-provider.md) | Asaas, the first real payment provider | accepted | 4 |
| [0050](0050-stripe-provider.md) | Stripe, the second real payment provider | accepted | 4 |
| [0051](0051-delinquency-report.md) | The delinquency report | accepted | 4 |
| [0052](0052-purchases-and-inventory-plan.md) | Purchases and inventory: plan, shared rules and what it changes underneath | accepted | 5 |
| [0053](0053-purchase-orders.md) | Purchase orders, their receipts and the payables they make | accepted | 5 |
| [0054](0054-stock-movements.md) | Stock movements, balances and the average cost | accepted | 5 |
| [0055](0055-minimum-stock-and-the-sale-block.md) | Minimum stock alerts and the block of a sale without balance | accepted | 5 |
| [0056](0056-inventory-count.md) | Inventory count and adjustment | accepted | 5 |
| [0057](0057-purchase-entry-from-nfe-xml.md) | Purchase entry from a supplier's NF-e XML | accepted | 5 |
| [0058](0058-stock-reports.md) | Stock reports: position, movements and valuation | accepted | 5 |
| [0059](0059-job-queue.md) | A queue of jobs and a worker, with e-mail sent through it | accepted | — |
| [0060](0060-password-reset.md) | Password reset by an e-mailed link | accepted | — |
| [0061](0061-document-emails.md) | A purchase order and a payment receipt sent by e-mail | accepted | — |
| [0062](0062-account-notices.md) | Notices to the owner of an address when the account changes | accepted | — |
| [0063](0063-email-confirmation.md) | Confirming the address of an account | accepted | — |
| [0064](0064-entry-import-from-a-file.md) | Entry import from a file | accepted | — |
| [0065](0065-document-mail-limits-and-log.md) | A limit and a log for documents sent by e-mail | accepted | — |
| [0066](0066-recovering-an-unconfirmed-address.md) | Recovering an address held by an account nobody confirmed | accepted | — |
| [0067](0067-accountant-and-reporting-plan.md) | Accountant and reporting: plan, shared rules and what it changes underneath | accepted | 6 |
| [0068](0068-email-layout.md) | One layout for every e-mail | accepted | — |
| [0069](0069-period-lock.md) | Period lock | accepted | 6 |
| [0070](0070-income-statement.md) | The managerial income statement (DRE) | accepted | 6 |
| [0071](0071-accountant-exports.md) | Exports for the accountant | accepted | 6 |
| [0072](0072-monthly-package-notice.md) | The monthly package notice, by e-mail to the accountants | accepted | 6 |
| [0073](0073-fiscal-module-plan.md) | The fiscal module: plan, shared rules and what it changes underneath | accepted | 7 |
| [0074](0074-fiscal-settings.md) | Fiscal settings per company | accepted | 7 |
| [0075](0075-fiscal-gateway-port.md) | The fiscal gateway port, its fake and the Focus NFe adapter | accepted | 7 |
| [0076](0076-nfe-documents.md) | NF-e documents: the store, issuing from a sale, events and the package | accepted | 7 |
| [0077](0077-choosing-a-fiscal-gateway.md) | Two fiscal gateways, chosen by each installation | accepted | 7 |
| [0078](0078-gateway-reference-and-queued-events.md) | A reference the gateway assigns and events it answers later | accepted | 7 |
| [0079](0079-nfeio-adapter.md) | The NFE.io adapter | accepted | 7 |
| [0080](0080-nfse-plan-and-settings.md) | NFS-e: the plan and what the settings must say first | accepted | 7 |
| [0081](0081-nfse-documents.md) | NFS-e documents, the DPS numbering and the port's NFS-e calls | accepted | 7 |
| [0082](0082-nfce-and-point-of-sale.md) | NFC-e at the counter and the minimal point of sale | accepted | 7 |
| [0083](0083-received-invoices-and-the-fiscal-webhook.md) | Received invoices, the recipient's manifestation and the fiscal webhook | accepted | 7 |
| [0084](0084-tax-calculation-behind-a-port.md) | The tax calculation behind a port, and the accountant's checklist | accepted | 7 |
| [0085](0085-gateway-settings-per-company.md) | The gateway's settings, per company, with the secrets sealed | accepted | 7 |
| [0086](0086-integrations-and-mobile-plan.md) | Integrations and mobile: the plan for Phase 8 | accepted | 8 |
| [0087](0087-public-api-v1.md) | The public API v1: keys, scopes, limits, the document and the first resources | accepted | 8 |
| [0088](0088-webhooks.md) | Webhooks: subscriptions, signed deliveries, retries and a log | accepted | 8 |
| [0089](0089-bank-statement-sync.md) | Automatic bank statement sync through an aggregator | accepted | 8 |
| [0090](0090-ecommerce-connectors.md) | E-commerce connectors: the settings page and the WooCommerce package | accepted | 8 |
| [0091](0091-pwa-and-offline-reading.md) | The PWA: installable, with offline reading under written cache rules | accepted | 8 |
| [0092](0092-one-point-zero-hardening-plan.md) | What 1.0 means, and the plan for Phase 9 (hardening) | accepted | 9 |
| [0095](0095-i18n.md) | The screens in two languages | accepted | 9 |

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
