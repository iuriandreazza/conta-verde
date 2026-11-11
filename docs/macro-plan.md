---
title: Macro plan
status: living
date: 2026-11-04
tags: [plan, scope, roadmap]
---

# Macro plan

conta-verde is an open-source, self-hostable ERP for Brazilian small businesses.
The functional reference is the cloud ERP Conta Azul: its module set, workflows and
vocabulary define what "complete" means for this project. We copy the **functionality**,
never the brand, visuals, copy or data (see [ADR 0001](adr/0001-license-agpl.md)).

This document says *what* is in scope, *in which order* and *why*. The
[roadmap](roadmap.md) tracks progress per phase, and the [ADRs](adr/README.md) record the
decisions that constrain the implementation.

## Who it is for

| User | What they need from the product |
|------|--------------------------------|
| Owner or manager of a small business (services, retail, wholesale) | Know how much is coming in and going out, sell, get paid, buy, control stock and issue invoices without a separate tool. |
| Accountant of that business | Read the books in real time, export entries and fiscal XMLs into their own accounting software. |
| Operator hosting the software | Run one instance for several companies (CNPJs) with a single database, upgrade safely, keep AGPL obligations. |

## Reference module map

Each row is a module of the reference product, what we build for it and the roadmap phase
that delivers it. "Parity" means a user coming from the reference product finds the same
capability, even if the screens differ.

| Module | Reference capability | What we build | Phase |
|--------|---------------------|---------------|-------|
| Companies and users | Several companies per account, users with permission profiles, accountant access | `companies`, memberships, roles (owner, financial, sales, stock, accountant, read-only), invitations | 1 |
| Registers | Customers, suppliers, carriers, sellers, products, services, categories, cost centers, bank accounts, payment methods and terms | All of them, scoped by company, with CPF/CNPJ validation, addresses (CEP lookup), import from CSV | 1 |
| Accounts receivable and payable | Entries with installments, partial and full settlement, interest, fine and discount, recurrence, attachments, transfers between accounts | Same model: `financial_entries` with installments and settlements; recurring rules generate entries ahead of time | 2 |
| Cash flow | Daily and monthly, realized vs. forecast, per account | Reports computed from entries and settlements, cached per day | 2 |
| Bank reconciliation | OFX import, automatic statement sync, match suggestions, create entries from statement lines | OFX first, automatic sync through an aggregator port later ([ADR 0011](adr/0011-bank-data-ingestion.md)) | 2, 8 |
| Managerial chart of accounts and DRE | Category tree mapped to a managerial income statement (DRE gerencial) | Category tree with DRE groups; DRE and results by cost center ([ADR 0012](adr/0012-managerial-accounting-model.md)) | 2, 6 |
| Sales | Quotes (PDF, e-mail, approval link), sales orders, sales of products and services, recurring contracts, commissions, price lists, returns | Quote → order → sale lifecycle generating receivables; recurring contracts; commissions per seller | 3 |
| Collection | Registered boleto, Pix, card payment link, automatic reminders before and after due date by e-mail and WhatsApp, automatic settlement on payment | Payment-provider ports with webhooks ([ADR 0010](adr/0010-payment-providers.md)), dunning rules, notifications ([ADR 0014](adr/0014-notifications.md)) | 4 |
| Purchases | Purchase orders, purchase entry from the supplier's NF-e XML, payables generated, stock input | Purchase orders, XML import (parsing only, no SEFAZ), payables and stock movements | 5 |
| Inventory | Balances, movements, average cost, minimum stock alerts, inventory adjustment, block sales without balance | Same, per company and warehouse (one warehouse at first) | 5 |
| Reports and dashboard | Home dashboard (balances, receivables, payables, sales), sales by customer/product/seller, delinquency, ABC curve, commissions | Dashboard in the SPA, reports as endpoints with CSV/XLSX export | 3, 6 |
| Accountant | Accountant profile, export of entries and XMLs, integration with accounting software | Accountant role, period lock, the DRE, exports of the books and of the files attached to entries, a monthly notice ([ADR 0013](adr/0013-accountant-access-and-exports.md), [ADR 0067](adr/0067-accountant-and-reporting-plan.md)); a vendor's own layout and our own fiscal XMLs are not in this phase | 6 |
| Fiscal documents | NF-e, NFS-e, NFC-e, A1 certificate, CFOP/NCM/CST, tax regime settings, cancellation, correction letter, DANFE | Behind an emission port; NF-e and national NFS-e first, NFC-e with the point of sale ([ADR 0006](adr/0006-fiscal-last.md)) | 7 |
| Point of sale | Online POS issuing NFC-e | Minimal POS screen on top of sales and NFC-e | 7 |
| Integrations | Public API, e-commerce and marketplace connectors, automation platforms | Versioned REST API with OpenAPI, API keys, webhooks ([ADR 0015](adr/0015-public-api-and-webhooks.md)); connectors as separate packages | 8 |
| Mobile | Native apps | Responsive SPA installable as a PWA; no native apps | 8 |
| Migration | Import from spreadsheets and other ERPs | CSV importers for registers and open entries | 1, 9 |

## Deliberately not copied

- **Banking products**: the reference product runs its own digital account and credit offers. We integrate with banks and payment providers; we never hold money.
- **Payroll and HR**: out of scope; it is a different regulatory domain.
- **AI assistant and app marketplace**: not before 1.0.
- **Native mobile apps**: a PWA covers the use cases we target.
- **Multi-currency**: BRL only. The money type leaves room for it, the UI does not.

## Guardrails

These hold for every phase. Each one is justified in an ADR.

1. Rust workspace with one crate per layer; dependencies point inwards ([ADR 0002](adr/0002-layered-cargo-workspace.md)).
2. Money is exact decimal ([ADR 0003](adr/0003-money-as-decimal.md)).
3. Every business row belongs to a company ([ADR 0004](adr/0004-company-scoped-tenancy.md)).
4. PostgreSQL is the only required infrastructure; background work runs on it ([ADR 0009](adr/0009-postgres-only-background-jobs.md)).
5. Every write to a business record is audited ([ADR 0008](adr/0008-audit-trail.md)).
6. External services (payments, banks, fiscal, messaging) sit behind ports with at least one real adapter and one fake ([ADR 0010](adr/0010-payment-providers.md), [0011](adr/0011-bank-data-ingestion.md), [0014](adr/0014-notifications.md)).
7. The SPA and the public `v1` API are two front doors to the same use cases, so no rule, check or audit entry can be skipped through either; the SPA's own routes are not a contract, and `v1` is a curated frozen set ([ADR 0005](adr/0005-spa-frontend.md), [ADR 0015](adr/0015-public-api-and-webhooks.md), amended by [ADR 0086](adr/0086-integrations-and-mobile-plan.md)).

## Releases

A release is cut when a phase reaches its exit criteria in the roadmap. Versions below 1.0
may break the API and the database schema between minors; migrations are still provided.
The versions 0.1 to 0.9 below are the **names of phases**: nothing was ever tagged, so the first tag (1.0) is the first
version an operator can upgrade from, and what it promises is written in [ADR 0092](adr/0092-one-point-zero-hardening-plan.md).

| Version | Name | A user can… | Phase |
|---------|------|-------------|-------|
| 0.1 | Foundation | Run the API and manage customers and suppliers (done). | 0 |
| 0.2 | Sign in | Create a company, invite users, fill every register, import registers from CSV. | 1 |
| 0.3 | Financial control | Record receivables and payables, settle them, see cash flow, import OFX and reconcile, all from the web app. **First release recommended for real use.** | 2 |
| 0.4 | Sell | Send quotes, turn them into sales, bill recurring contracts, pay commissions, see the dashboard. | 3 |
| 0.5 | Get paid | Issue boleto and Pix, send reminders, see payments settle themselves. | 4 |
| 0.6 | Buy and stock | Register purchases from XML, track stock and average cost. | 5 |
| 0.7 | Accountant | Give the accountant access, lock periods, export entries and documents, read the DRE. | 6 |
| 0.8 | Invoice | Issue NF-e, NFS-e and NFC-e, with a minimal POS. | 7 |
| 0.9 | Connect | Use the API with keys and webhooks, install as PWA, connect an e-commerce. | 8 |
| 1.0 | Stable | Upgrade in place, restore backups, rely on row-level security, read complete docs. | 9 |

## Sequencing rules

- **Identity before data**: nothing beyond the foundation ships without authentication, because every later table needs a real `company_id` and a user to audit.
- **Finance before sales**: sales exist to produce receivables; the receivable model must be stable first.
- **Collection right after sales**: getting paid is the main value for service businesses and only needs customers and receivables, so it comes before purchases and inventory.
- **Inventory before fiscal**: NF-e of products needs stock, NCM and purchase costs to be right.
- **Fiscal last**: highest legal and integration risk; the rest of the product is useful without it ([ADR 0006](adr/0006-fiscal-last.md)).
- **Integrations after the API is stable**: webhooks and keys expose the API contract; we only commit to it once the modules above exist.

## Risks

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Fiscal rules change or differ per state/municipality | Emission breaks or is wrong | Emission behind a port; prefer an existing gateway; NFS-e only through the national standard. |
| Payment providers change APIs or pricing | Collection stops | Port with at least two adapters and a fake; webhooks idempotent and replayable. |
| Scope creep from the reference product | Nothing ships | The module map is the scope; new modules need a macro-plan change first. |
| Single maintainer | Bus factor | ADRs and this plan keep the reasoning in the repository; CLAUDE.md keeps the conventions. |
| AGPL discouraging adoption | Smaller community | Accepted on purpose; see ADR 0001. |

## Working method

- One vertical slice per pull request: migration, domain, use case, adapters, endpoint, test, docs.
- Each phase starts by accepting the ADRs it depends on (status `proposed` → `accepted`) and ends by ticking its exit criteria in the roadmap.
- Significant deviations from this plan are recorded as an ADR and reflected here in the same PR.
