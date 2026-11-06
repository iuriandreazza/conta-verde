---
title: Roadmap
status: living
date: 2026-11-04
tags: [roadmap]
---

# Roadmap

Progress tracker for the [macro plan](macro-plan.md). Phases are ordered by dependency;
each one builds on the data created by the previous one. A phase is done when every item
is ticked and its exit criteria hold on `main`.

## Phase 0 — Foundation (v0.1, done)

- [x] AGPL-3.0-or-later license ([ADR 0001](adr/0001-license-agpl.md))
- [x] Cargo workspace with `domain`, `application`, `infrastructure`, `api` crates ([ADR 0002](adr/0002-layered-cargo-workspace.md))
- [x] PostgreSQL migrations, CI, Docker Compose for local development, pre-commit checks
- [x] First vertical slice: parties (customers and suppliers) with CPF/CNPJ validation, scoped by company ([ADR 0004](adr/0004-company-scoped-tenancy.md))

## Phase 1 — Identity, tenancy and registers (v0.2, done)

Identity:
- [x] `users`, `companies`, `memberships` with roles; password hashing and sessions ([ADR 0007](adr/0007-authentication-and-authorization.md)) — tables, Argon2id and cookie sessions; roles and what each may do ([ADR 0026](adr/0026-membership-roles-and-permissions.md))
- [x] Enforce `company_id` from the session, not the URL; foreign keys from every business table to `companies` — the URL id must match a membership of the caller (`CompanyAccess`)
- [x] Invitations by e-mail; accountant role reserved for Phase 6 ([ADR 0027](adr/0027-invitations.md)) — invite, accept, withdraw and the members page, sent through SMTP (Mailpit in development) or logged without a server; signing up through the link makes no company of its own; changing a role and removing a member are in [ADR 0028](adr/0028-member-management.md), and the owner is never touched
- [x] Audit trail on every write ([ADR 0008](adr/0008-audit-trail.md)) — every register writes its entries in the same transaction as the write (parties with their re-checks, products, categories, cost centers, bank accounts, payment methods and terms, and each row of an import), with a history panel; membership, session and invitation writes are never audited; each new register must write its entries too
- [x] Pagination, sorting and filtering convention for list endpoints (parties first) ([ADR 0018](adr/0018-list-endpoints-search-filter-sort.md)) — `limit`, `after`, `q`, a named filter per attribute and `sort`; each new register's list follows it

Registers:
- [x] Parties: CNPJ checked against the federal registry and alphanumeric CNPJ support ([ADR 0017](adr/0017-cnpj-registry-lookup.md)); a flagged party ("unverified", "not found", inactive) can be checked again by hand
- [x] Parties: editing, carrier and seller roles, and one address ([ADR 0022](adr/0022-party-roles-address-and-editing.md))
- [x] Parties: contacts and tags ([ADR 0023](adr/0023-party-contacts-and-tags.md))
- [x] Parties: the CEP lookup that fills an address in, on request ([ADR 0024](adr/0024-postal-code-lookup.md))
- [x] Products and services: SKU, unit, NCM, prices, minimum stock, active flag ([ADR 0019](adr/0019-products-and-services.md)) — registering, editing and deactivating with a history; stock balances come with Phase 5
- [x] Categories as a tree with DRE group mapping; cost centers ([ADR 0012](adr/0012-managerial-accounting-model.md)) — the tree, its groups, the default template and the flat cost centers; entries and the DRE come with Phases 2 and 6
- [x] Bank accounts (checking, savings, cash, card), payment methods, payment terms ([ADR 0021](adr/0021-bank-accounts-payment-methods-and-terms.md)) — registers only: opening balances, applying a term to an amount and the fiscal payment codes come later
- [x] CSV import for parties and products ([ADR 0025](adr/0025-csv-import.md)) — check everything first, then write row by row and skip what is registered; creates only, never updates
- [x] Web app skeleton: login, company switcher, register screens ([ADR 0005](adr/0005-spa-frontend.md)) — sign in, sign up, company picker, and the parties list and form, with the AGPL "Source code" footer link
- [x] UX pass: sidebar shell, shared design tokens ([docs/design](design/README.md)) and a company dashboard with sample data ([ADR 0020](adr/0020-app-shell-charts-and-sample-data.md)); the numbers become real in Phase 2

Exit criteria: a new user signs up, creates a company, invites a colleague and fills every
register from the browser; no endpoint accepts a `company_id` the caller is not a member of.

## Phase 2 — Financial control (v0.3)

- [x] Financial entries (receivable/payable) with installments, due dates, category, cost center, party, attachments ([ADR 0029](adr/0029-financial-entries.md), [ADR 0036](adr/0036-entry-attachments.md)) — register, change and list entries with their installments and history, in the `finance` area; attach up to 20 files of 10 MB (PDF, image, text, CSV, XML) to an entry, kept behind a `BlobStore` on disk, downloaded as files and removed by marking; there is no cancelling of an entry, it is settled and reversed
- [x] Settlement: full and partial, interest, fine and discount, settlement date and bank account ([ADR 0030](adr/0030-settlements.md)) — pay an installment from an account, never for more than is left, reverse a settlement; an entry with settlements keeps its installments and kind; the list says paid, partial, overdue or open
- [x] Transfers between accounts; opening balances; balance per account and consolidated ([ADR 0031](adr/0031-transfers-and-balances.md)) — move money between two accounts and reverse it, set an account's opening balance (signed), read every account's balance on a day and the total; the bank accounts page shows today's balance
- [x] Recurring entries generated ahead by a scheduled job ([ADR 0033](adr/0033-recurring-entries.md), [ADR 0009](adr/0009-postgres-only-background-jobs.md)) — a recurrence (weekly, monthly or yearly, with an optional end) becomes ordinary entries up to 60 days ahead, once per occurrence day, made when it is saved and by an hourly job in the API process; it can be paused, and what was generated is never rewritten
- [x] Cash flow: daily and monthly, realized and forecast, per account and consolidated ([ADR 0032](adr/0032-cash-flow.md)) — `GET /cash-flow` cuts a span into days or months: what settlements and transfers moved, the running balance, and what is still open by due date (late installments counted), with a page that draws the realized and projected balance
- [x] OFX import and reconciliation: match suggestions, create entry from statement line, ignore line ([ADR 0034](adr/0034-statement-import-and-reconciliation.md), [ADR 0011](adr/0011-bank-data-ingestion.md)) — import an OFX statement (UTF-8 or Latin-1, safe to send twice), see the settlements and transfers that fit each line, match, unmatch, ignore, restore, or make a settled entry out of a line; CSV statements are not in
- [x] Reports: entries by category, by cost center, by party, overdue list ([ADR 0035](adr/0035-entry-reports.md)) — the entries added up by group over a span, by competence or by due day, and the installments that fell due and still have something left, with the totals of all; a Relatórios page with a chart and tables
- [x] Web app: entries, settlement, cash flow, reconciliation screens; "Source code" link in the footer (AGPL section 13) — pages for entries (with settlements and attachments), transfers, recurrences, cash flow, reconciliation and reports; the footer links to the source

Exit criteria: a business can run its monthly financial routine (launch, settle, reconcile,
read cash flow) without another tool. First release recommended for real use.

## Phase 3 — Sales (v0.4, done)

Planned in [ADR 0037](adr/0037-sales-module-plan.md), which also settles what the phase changes underneath
(a voided mark on entries, a sale as an entry's origin, applying a payment term, the `sales` area, the one
rounding rule). No proposed ADR belongs to this phase. One slice per pull request, in this order:

- [x] Quotes: items (products and services), discounts, validity, PDF, send by e-mail, public approval link ([ADR 0038](adr/0038-quotes.md)) — numbered quotes with discounted lines and a validity, a PDF, a link e-mailed to the customer (shown once, taken back when the quote changes or is sent again) where they approve or decline without an account, and the `sales` area; the quote becomes a sale in the next slice
- [x] Sales orders and sales: status lifecycle, receivables generated according to payment terms, cancellation and returns; a quote becomes a sale ([ADR 0039](adr/0039-sales.md)) — an open order becomes a completed sale that makes its one receivable by the payment term (in the cash flow forecast), is cancelled with its receivable (entries gain a one-way cancelled mark), takes returns that refund with a payable, and is made from an approved quote
- [x] Recurring contracts: billing cycle, automatic sale and receivable per period, adjustment and termination ([ADR 0040](adr/0040-contracts.md)) — a contract (monthly to yearly) bills each cycle on its day as a completed sale and its receivable, by an hourly job and when it is written, once per cycle day (a unique index), with a percentage adjustment and a termination; the tests move the clock through three cycles
- [x] Sellers and commissions: rules per seller or product, commission statement ([ADR 0041](adr/0041-commissions.md)) — a rate for one seller or all and one product or all over a span of sale days, the narrowest rule on the sale's day applying to each line, and a statement computed when read from the completed sales net of returns
- [x] Price lists ([ADR 0042](adr/0042-price-lists.md)) — a named, audited set of unit prices per product; choosing one in a quote, sale or contract form fills the lines' prices, and documents keep the prices they were filled with
- [x] Dashboard: balances, receivables and payables due, sales of the month, top products and customers (real figures replace the sample data) ([ADR 0043](adr/0043-sales-reports-and-dashboard.md)) — the home page reads the balance of each account and the statement lines waiting to be reconciled, mirrored receivable and payable blocks (overdue, due today, next days) and the due ranges, the cash flow of a chosen period and the top products and customers, each card on its own request, with an "atualizado em" stamp
- [x] Reports: sales by customer, product, seller and period; ABC curve ([ADR 0043](adr/0043-sales-reports-and-dashboard.md)) — `GET /sales-reports` ranks the completed sales of a span net of returns by customer, product or seller with each row's share and its ABC class, and the Relatórios page has a Vendas view

Exit criteria: a quote becomes a sale and its receivables appear in cash flow; a recurring
contract bills itself for three cycles in a test clock.

## Phase 4 — Collection and notifications (v0.5)

Planned in [ADR 0044](adr/0044-collection-module-plan.md), which also accepts [ADR 0010](adr/0010-payment-providers.md)
and settles what the phase changes underneath (installation-wide provider credentials, a charge that settles once
through the settlement path, the provider fee as a payable, signed public webhooks, the dunning job). One slice
per pull request, in this order:

- [x] Payment-provider port with the fake adapter; charges attached to installments: boleto, Pix (dynamic QR), card link ([ADR 0045](adr/0045-charges.md)) — a `PaymentProvider` port with a fake and an off adapter (`PAYMENT_PROVIDER`), and charges for what is left of an installment: issue a boleto, Pix or card link, see what the customer pays with, cancel, with a history; one open charge per installment
- [ ] Webhooks settle the installment and record the provider fee
- [ ] Customer-facing payment page (charge details, boleto, Pix copy-and-paste) behind a token
- [ ] Notification ports: e-mail (SMTP) first, WhatsApp through a provider ([ADR 0014](adr/0014-notifications.md)); dunning rules with reminders before and after the due date, cadence per company, sent by the hourly job
- [ ] First real provider adapter
- [ ] Second real provider adapter
- [ ] Delinquency report

Exit criteria: an installment issued as boleto or Pix settles itself when the provider's
webhook arrives, and the customer received the reminder sequence.

## Phase 5 — Purchases and inventory (v0.6)

- [ ] Purchase orders: items, supplier, expected dates, approval, receipt
- [ ] Purchase entry from NF-e XML (parsing and validation only, no SEFAZ calls): supplier, items, taxes, payables
- [ ] Stock movements: in, out, adjustment, transfer; balances and average cost per product
- [ ] Minimum stock alerts; block sale without balance (configurable)
- [ ] Inventory count and adjustment
- [ ] Reports: stock position, movements, valuation

Exit criteria: a purchase XML creates payables and stock; a sale reduces stock and the
average cost is right against a worked example in the tests.

## Phase 6 — Accountant and reporting (v0.7)

- [ ] Accountant role: read access to every company that invites them, no write
- [ ] Period lock: entries before the lock date cannot change without reopening
- [ ] DRE (managerial income statement) by month and cost center ([ADR 0012](adr/0012-managerial-accounting-model.md))
- [ ] Exports: entries and settlements (CSV/XLSX), fiscal XMLs and PDFs by period ([ADR 0013](adr/0013-accountant-access-and-exports.md))
- [ ] Report scheduling by e-mail (monthly package to the accountant)

Exit criteria: an accountant closes a month from their own login and downloads everything
they need for their accounting software.

## Phase 7 — Fiscal (v0.8)

- [ ] Fiscal settings per company: tax regime (Simples Nacional first, then Lucro Presumido), A1 certificate, series and numbering, operation natures (CFOP), CST/CSOSN rules
- [ ] Emission port with a gateway adapter and a fake ([ADR 0006](adr/0006-fiscal-last.md))
- [ ] NF-e: issue from a sale, DANFE PDF, cancellation, correction letter, number invalidation, contingency
- [ ] NFS-e through the national standard; municipal adapters only when the standard is not available
- [ ] NFC-e and a minimal point of sale screen
- [ ] Recipient manifestation for incoming NF-e
- [ ] Tax calculation review by an accountant before the release

Exit criteria: a product sale and a service sale produce authorized documents in the
provider's test environment and the XMLs reach the accountant export.

## Phase 8 — Integrations and mobile (v0.9)

- [ ] Public API `v1` frozen: OpenAPI document, API keys per company, rate limits ([ADR 0015](adr/0015-public-api-and-webhooks.md))
- [ ] Webhooks: subscriptions, signed deliveries, retries, delivery log
- [ ] Automatic bank statement sync through an aggregator adapter ([ADR 0011](adr/0011-bank-data-ingestion.md))
- [ ] First e-commerce connector as a separate package using only the public API
- [ ] PWA: installable, offline read of recent data

Exit criteria: a third-party client creates a sale and receives its webhook without any
private endpoint.

## Phase 9 — 1.0 hardening

- [ ] PostgreSQL row-level security policies as a second barrier to the application checks ([ADR 0004](adr/0004-company-scoped-tenancy.md))
- [ ] Backup and restore procedure, upgrade guide, published Docker image
- [ ] Migration importers: open entries and stock balances from CSV; guidance for leaving the reference product
- [ ] Performance baseline on a 5-year dataset; indexes reviewed
- [ ] User documentation complete in English; Portuguese translation of the UI

Exit criteria: an operator upgrades two consecutive versions in place with a restored backup
and all checks green.
