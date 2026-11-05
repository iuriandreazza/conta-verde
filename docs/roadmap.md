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

## Phase 1 — Identity, tenancy and registers (v0.2)

Identity:
- [ ] `users`, `companies`, `memberships` with roles; password hashing and sessions ([ADR 0007](adr/0007-authentication-and-authorization.md)) — tables, Argon2id and cookie sessions are in; roles are not
- [x] Enforce `company_id` from the session, not the URL; foreign keys from every business table to `companies` — the URL id must match a membership of the caller (`CompanyAccess`)
- [ ] Invitations by e-mail; accountant role reserved for Phase 6
- [ ] Audit trail on every write ([ADR 0008](adr/0008-audit-trail.md)) — in for parties (creation and CNPJ re-checks, with a history panel); each new register must write its entries too
- [x] Pagination, sorting and filtering convention for list endpoints (parties first) ([ADR 0018](adr/0018-list-endpoints-search-filter-sort.md)) — `limit`, `after`, `q`, a named filter per attribute and `sort`; each new register's list follows it

Registers:
- [x] Parties: CNPJ checked against the federal registry and alphanumeric CNPJ support ([ADR 0017](adr/0017-cnpj-registry-lookup.md)); a flagged party ("unverified", "not found", inactive) can be checked again by hand
- [x] Parties: editing, carrier and seller roles, and one address ([ADR 0022](adr/0022-party-roles-address-and-editing.md))
- [x] Parties: contacts and tags ([ADR 0023](adr/0023-party-contacts-and-tags.md))
- [x] Parties: the CEP lookup that fills an address in, on request ([ADR 0024](adr/0024-postal-code-lookup.md))
- [x] Products and services: SKU, unit, NCM, prices, minimum stock, active flag ([ADR 0019](adr/0019-products-and-services.md)) — registering, editing and deactivating with a history; stock balances come with Phase 5
- [x] Categories as a tree with DRE group mapping; cost centers ([ADR 0012](adr/0012-managerial-accounting-model.md)) — the tree, its groups, the default template and the flat cost centers; entries and the DRE come with Phases 2 and 6
- [x] Bank accounts (checking, savings, cash, card), payment methods, payment terms ([ADR 0021](adr/0021-bank-accounts-payment-methods-and-terms.md)) — registers only: opening balances, applying a term to an amount and the fiscal payment codes come later
- [ ] CSV import for parties and products ([ADR 0025](adr/0025-csv-import.md)) — parties are in (check everything first, then write row by row, skip what is registered); products come next
- [x] Web app skeleton: login, company switcher, register screens ([ADR 0005](adr/0005-spa-frontend.md)) — sign in, sign up, company picker, and the parties list and form, with the AGPL "Source code" footer link
- [x] UX pass: sidebar shell, shared design tokens ([docs/design](design/README.md)) and a company dashboard with sample data ([ADR 0020](adr/0020-app-shell-charts-and-sample-data.md)); the numbers become real in Phase 2

Exit criteria: a new user signs up, creates a company, invites a colleague and fills every
register from the browser; no endpoint accepts a `company_id` the caller is not a member of.

## Phase 2 — Financial control (v0.3)

- [ ] Financial entries (receivable/payable) with installments, due dates, category, cost center, party, attachments
- [ ] Settlement: full and partial, interest, fine and discount, settlement date and bank account
- [ ] Transfers between accounts; opening balances; balance per account and consolidated
- [ ] Recurring entries generated ahead by a scheduled job ([ADR 0009](adr/0009-postgres-only-background-jobs.md))
- [ ] Cash flow: daily and monthly, realized and forecast, per account and consolidated
- [ ] OFX import and reconciliation: match suggestions, create entry from statement line, ignore line ([ADR 0011](adr/0011-bank-data-ingestion.md))
- [ ] Reports: entries by category, by cost center, by party, overdue list
- [ ] Web app: entries, settlement, cash flow, reconciliation screens; "Source code" link in the footer (AGPL section 13)

Exit criteria: a business can run its monthly financial routine (launch, settle, reconcile,
read cash flow) without another tool. First release recommended for real use.

## Phase 3 — Sales (v0.4)

- [ ] Quotes: items (products and services), discounts, validity, PDF, send by e-mail, public approval link
- [ ] Sales orders and sales: status lifecycle, receivables generated according to payment terms, cancellation and returns
- [ ] Recurring contracts: billing cycle, automatic sale and receivable per period, adjustment and termination
- [ ] Sellers and commissions: rules per seller or product, commission statement
- [ ] Price lists
- [ ] Dashboard: balances, receivables and payables due, sales of the month, top products and customers
- [ ] Reports: sales by customer, product, seller and period; ABC curve

Exit criteria: a quote becomes a sale and its receivables appear in cash flow; a recurring
contract bills itself for three cycles in a test clock.

## Phase 4 — Collection and notifications (v0.5)

- [ ] Payment-provider ports: registered boleto, Pix (dynamic QR), card payment link; fake adapter plus two real ones ([ADR 0010](adr/0010-payment-providers.md))
- [ ] Charges attached to installments; webhooks settle the installment and record fees
- [ ] Dunning rules: reminders before and after due date, cadence per company
- [ ] Notification ports: e-mail (SMTP) first, WhatsApp through a provider ([ADR 0014](adr/0014-notifications.md))
- [ ] Customer-facing payment page (charge details, boleto PDF, Pix copy-and-paste)
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
