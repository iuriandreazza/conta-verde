---
title: Roadmap
status: living
date: 2026-11-04
tags: [roadmap]
---

# Roadmap

Progress tracker for the [macro plan](macro-plan.md). The "v0.N" in a phase title names the phase, not a tagged release (nothing has been tagged yet, [ADR 0092](adr/0092-one-point-zero-hardening-plan.md)). Phases are ordered by dependency;
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

## Phase 2 — Financial control (v0.3, done)

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

## Phase 4 — Collection and notifications (v0.5, done)

Planned in [ADR 0044](adr/0044-collection-module-plan.md), which also accepts [ADR 0010](adr/0010-payment-providers.md)
and settles what the phase changes underneath (installation-wide provider credentials, a charge that settles once
through the settlement path, the provider fee as a payable, signed public webhooks, the dunning job). One slice
per pull request, in this order:

- [x] Payment-provider port with the fake adapter; charges attached to installments: boleto, Pix (dynamic QR), card link ([ADR 0045](adr/0045-charges.md)) — a `PaymentProvider` port with a fake and an off adapter (`PAYMENT_PROVIDER`), and charges for what is left of an installment: issue a boleto, Pix or card link, see what the customer pays with, cancel, with a history; one open charge per installment
- [x] Webhooks settle the installment and record the provider fee ([ADR 0046](adr/0046-webhooks-and-automatic-settlement.md)) — a public, signed route per company and provider; a payment settles the installment through the settlement path (once per charge, cash exactly what was paid) and enters the provider's fee as a settled payable; the same notice twice, or interrupted, settles once; what cannot be applied is kept with a note on the charge; a Cobrança page chooses the account the money lands in
- [x] Customer-facing payment page (charge details, boleto, Pix copy-and-paste) behind a token ([ADR 0047](adr/0047-payment-page.md)) — a link per charge (many allowed, only the digest kept, shown once) leads, with no session, to a page that shows the amount and what to pay with while there is something to pay, and says so when it was paid or can no longer be
- [x] Notification ports: e-mail (SMTP) first, WhatsApp through a provider ([ADR 0014](adr/0014-notifications.md)); dunning rules with reminders before and after the due date, cadence per company, sent by the hourly job ([ADR 0048](adr/0048-notifications-and-dunning.md)) — a plan per company (when and by which channel), an hourly pass that reminds of the receivables' installments that still owe, once per rule however often it runs (a claimed row per installment, rule and channel), with the payment page link; a `MessageSender` port with the WhatsApp Cloud API adapter; the reminders are listed under the entry
- [x] First real provider adapter ([ADR 0049](adr/0049-asaas-provider.md)) — Asaas (`PAYMENT_PROVIDER=asaas`, `ASAAS_API_KEY`, `ASAAS_WEBHOOK_TOKEN`): boleto, Pix and card page, the customer found or made by CPF/CNPJ, the exact amount, webhooks authenticated by Asaas's token header with the fee read as `value - netValue`; tested against a local stand-in, with an ignored sandbox test for homologation
- [x] Second real provider adapter ([ADR 0050](adr/0050-stripe-provider.md)) — Stripe (`PAYMENT_PROVIDER=stripe`, `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`): a boleto and a Pix by PaymentIntents (the payer's address now travels with the charge), a card page by a Checkout Session, signed webhooks with replay protection; no fee in Stripe's notice and a boleto that cannot be cancelled are documented limits
- [x] Delinquency report ([ADR 0051](adr/0051-delinquency-report.md)) — `GET /delinquency?on=`: what is late on a day by customer and by 1–30, 31–60, 61–90 and over 90 days, biggest debt first, with an Inadimplência page

Exit criteria: an installment issued as boleto or Pix settles itself when the provider's
webhook arrives, and the customer received the reminder sequence. Both hold in the tests: a Pix charge settles its
installment through a signed webhook, once however many times it arrives (`settles_charges_from_webhooks`, on both
adapters and through the API), and the hourly pass, on a moved clock, sends each rule's reminder once with the
payment link (`sends_reminders_on_schedule`).

## Phase 5 — Purchases and inventory (v0.6, done)

- [x] Purchase orders: items, supplier, expected dates, approval, receipt ([ADR 0053](adr/0053-purchase-orders.md)) — `/purchase-orders`: draft, approved, partly or fully received, cancelled; each receipt (partial ones allowed) values what arrived cumulatively and makes its own payable by the payment term; a `purchases` area (the `stock` role writes it)
- [x] Purchase entry from NF-e XML (parsing and validation only, no SEFAZ calls): supplier, items, taxes, payables ([ADR 0057](adr/0057-purchase-entry-from-nfe-xml.md)) — the XML is read without entities and limited to 1 MiB, checked in the domain (key check digit, totals, installments), previewed and entered once by its access key as a purchase received whole: payable on the invoice's installments, stock at what each item costs
- [x] Stock movements: in, out, adjustment; balances and average cost per product ([ADR 0054](adr/0054-stock-movements.md)) — a ledger never edited (reversed instead), the weighted average cost with one rounding rule held in the domain, receipts, completed sales, returns and cancellations move it; one stock, so no transfer ([ADR 0052](adr/0052-purchases-and-inventory-plan.md))
- [x] Minimum stock alerts; block sale without balance (configurable) ([ADR 0055](adr/0055-minimum-stock-and-the-sale-block.md)) — `GET /stock/alerts` lists the active products below their minimum; `PUT /stock/settings` turns on the per-company block, which refuses completing a sale the stock cannot cover with a `409` naming the product
- [x] Inventory count and adjustment ([ADR 0056](adr/0056-inventory-count.md)) — an open count holds what was found and shows the difference to the stock now; applying writes the adjustments once, valued at the average, on the day of the count
- [x] Reports: stock position, movements, valuation ([ADR 0058](adr/0058-stock-reports.md)) — `GET /stock-reports/valuation?on=` (by product and in total, with shares) and `GET /stock-reports/movements?from=&to=` (opening, in, out, adjusted, closing per product, up to 366 days), with the Relatórios de estoque page
- [x] Screens aligned with the reference product after a side-by-side review (layout only, nothing copied): summary tiles with counts on the stock position, the movements and the purchases list (the numbers come from the server: `GET /purchase-orders/status-counts`, `GET /stock/positions/summary`, `GET /stock/movements/kind-counts`, and `?situation=` on the position); a period filter, the expected day and the NF number on the purchases list; one "Novo" menu on Compras; export and print of the three lists; a column chooser (kept in the browser); ticking orders to approve or cancel them in one go; the NF-e entry in five steps with a last look; an inventory count started from a spreadsheet; an empty state with a drop zone
- [x] E-mail and background work that Phase 5 needed ([ADR 0059](adr/0059-job-queue.md), [ADR 0060](adr/0060-password-reset.md), [ADR 0061](adr/0061-document-emails.md), [ADR 0062](adr/0062-account-notices.md), [ADR 0063](adr/0063-email-confirmation.md)) — a queue of jobs in Postgres with a worker that retries; e-mail goes through it (invitations, quotes, reminders), plus password reset, a purchase order to its supplier and a payment receipt to the customer, two account notices, and the confirmation of an address

Not in Phase 5, on purpose: stock locations and reservations, transfers between stocks and cost adjustments (there is **one stock** per company, [ADR 0052](adr/0052-purchases-and-inventory-plan.md)), purchase quotations and recurring purchase orders, and any call to SEFAZ (the NF-e is only read, [ADR 0006](adr/0006-fiscal-last.md)). The e-mail work was closed afterwards: a limit and a log for the documents a member can e-mail ([ADR 0065](adr/0065-document-mail-limits-and-log.md)) and the recovery of an address held by an account nobody confirmed, released only to whoever proves the mailbox ([ADR 0066](adr/0066-recovering-an-unconfirmed-address.md)).

Exit criteria: a purchase XML creates payables and stock; a sale reduces stock and the
average cost is right against a worked example in the tests. Both hold in the tests: an NF-e entered through the API
makes the payable on the invoice's installments and the stock at what each item costs, and a sale then takes the stock at
the average cost (`a_purchase_xml_creates_payables_and_stock_and_a_sale_takes_it_at_the_average_cost`; on both adapters
`enters_a_supplier_invoice_once` and `stock_follows_purchases_and_sales`, the worked example of ADR 0054: 10 at 5 and
10 at 7 are 20 worth 120, a sale of 5 takes 30).

## Phase 6 — Accountant and reporting (v0.7, done)

Planned in [ADR 0067](adr/0067-accountant-and-reporting-plan.md), which also accepts [ADR 0013](adr/0013-accountant-access-and-exports.md)
and settles what the phase changes underneath (an `accounting` area where the accountant writes only the lock, the exports and
the schedule; which date locks each record; the DRE's sign and lines; the package's contents and limits; a monthly e-mail that
links to a login-gated page). One slice per pull request, in this order:

- [x] Plan ([ADR 0067](adr/0067-accountant-and-reporting-plan.md))
- [x] Accountant role: read access to every company that invites them, no write ([ADR 0067](adr/0067-accountant-and-reporting-plan.md)) — the `accountant` role reads every area but members and writes only the new `accounting` area (the lock, the exports, the schedule), the one table of route segments is `SEGMENT_AREAS`, and a test walks every route of the router with the accountant and the viewer
- [x] Period lock: entries before the lock date cannot change without reopening ([ADR 0069](adr/0069-period-lock.md)) — the company closes whole months (the owner, admins and the accountant), and what is dated on or before the lock cannot be made, changed or undone: entries by competence date, settlements, transfers, openings, stock movements, completed sales, returns and receipts; reopening needs a reason and every change is in the trail; a payment on a closed day is kept paid with a note; a Contador page closes and reopens
- [x] DRE (managerial income statement) by month and cost center ([ADR 0012](adr/0012-managerial-accounting-model.md), [ADR 0070](adr/0070-income-statement.md)) — `GET /dre` adds the entries up by competence date, one column per month or per cost center and the total, with the subtotals worked out from the groups and investments and loans below the line; a DRE view on the Relatórios page with a chart, export and print
- [x] Exports: entries and settlements (CSV/XLSX) and the files attached to entries by period; the XMLs of the documents the product issues join the package in Phase 7 ([ADR 0076](adr/0076-nfe-documents.md)); it keeps none of the purchase invoices it enters ([ADR 0013](adr/0013-accountant-access-and-exports.md), [ADR 0071](adr/0071-accountant-exports.md)) — `GET /accounting/exports/{entries,settlements,transfers,package}` for a span of at most 366 days, as CSV or XLSX written by hand, with the layout of every file fixed in the ADR; the package is a zip with the three files and the files attached to the entries (the product keeps no NF-e XML and issues no fiscal document, and says so); an `accounting` read, with limits that answer `422` instead of cutting a file; a downloads card on the Contador page
- [x] Report scheduling by e-mail (monthly package to the accountant) ([ADR 0072](adr/0072-monthly-package-notice.md)) — a per-company switch and a day of the month (1 to 28); an hourly pass announces the previous month to every member with the accountant role, through the queue, once per company and month (a row claimed by one statement, given back when nothing could be queued); the e-mail carries only a link to the Contador page, which asks for a login, never the files or a link that works without one

- [x] Screens and checks that came with the slices: the Contador page leads with what is closed (a badge and the day in large type), closes and reopens in two cards, offers the month after the lock, has the downloads and the notice cards; the DRE table keeps the names of its lines in view while it scrolls and explains itself behind an (i); `scripts/e2e-accountant.py` runs the exit criterion against a running API (an owner invites an accountant, who closes a month from their own login, is refused writes and downloads files checked against the books, and turns the notice on)

Not in Phase 6, on purpose: a statutory statement, SPED files, any accounting vendor's own layout (the files have one fixed layout, in [ADR 0071](adr/0071-accountant-exports.md)), and fiscal XMLs of our own (the product keeps no NF-e XML and issues no fiscal document until Phase 7, [ADR 0006](adr/0006-fiscal-last.md)).

Left open from this phase, each written in the ADR that made the choice:

- **The lock is checked before the write, not inside its transaction** ([ADR 0069](adr/0069-period-lock.md)): a write that races a closing can pass. The fix is to move the check into the adapters' transactions; do it if the race ever matters.
- **A package is built in memory** ([ADR 0071](adr/0071-accountant-exports.md)): up to 200 MiB per request with no cap on packages built at once, so a deployment that fears many readers asking for the biggest span at the same time limits them in front of the API. Streaming the zip to the response is the way out.
- **Names in the export are read one party at a time**: a month with many thousands of parties is slow; a batch read is the next step. An attached file whose bytes are gone makes the package answer `500` with the attachment named in the log.
- **The monthly notice is at most once per company and month** ([ADR 0072](adr/0072-monthly-package-notice.md)): a crash between the claim and the last message leaves some accountants not told, the catch-up window ends with the month, and a notice for day D goes out at about 21:00 Brazil time on D. It is sent whether or not the month was closed. An accountant cannot opt out of one company's notice without leaving it, and the schedule's trail (`GET .../package-schedule/history`) has no page yet.
- **Not covered by a test**: some messages of a notice queued and others not (the claim must stay), and a member removed or changed between two passes (the members are read live, so it holds).

Exit criteria: an accountant closes a month from their own login and downloads everything
they need for their accounting software. Both hold: through the API an accountant is invited, closes October, is refused every
write outside `accounting` (`an_accountant_closes_a_month_and_the_books_of_it_refuse_changes`, with the sweep of every route
of the router), the closed month refuses changes on both adapters (`a_closed_month_refuses_what_the_books_hold`), and the
files hold the month's entries, settlements and transfers exactly, byte for byte, with the attached files
(`the_accountant_downloads_the_books_of_a_month_exactly`; on both adapters `exports_the_books_of_a_span` and
`names_everything_for_the_export`). The same flow ran against the real API and Postgres (`scripts/e2e-accountant.py`), the
zip and the spreadsheet were opened with `unzip`, Python's `zipfile` and `openpyxl`, the notice reached a mailbox once however
often the API restarted, and the pages were used in a browser.

## Phase 7 — Fiscal (v0.8, built; the accountant's sign-off is open)

Planned in [ADR 0073](adr/0073-fiscal-module-plan.md), which also settles what the phase changes underneath (a `fiscal` area, the
`FiscalGateway` port with Focus NFe as the first real adapter, the gateway token in the environment and the A1 certificate left at
the gateway, no tax law embedded in the code, the document's state machine, the gateway owning the numbers, fiscal files in the
`BlobStore` and in the accountant's package, the period lock for cancelling). [ADR 0006](adr/0006-fiscal-last.md) is already
accepted and no other proposed ADR belongs to the phase. One slice per pull request, in this order:

- [x] Plan ([ADR 0073](adr/0073-fiscal-module-plan.md))
- [x] Fiscal settings per company: tax regime (Simples Nacional first, then Lucro Presumido), A1 certificate, series and numbering, operation natures (CFOP), CST/CSOSN rules ([ADR 0074](adr/0074-fiscal-settings.md)) — one audited record per company in the new `fiscal` area (owner, admin and `financial` write): the issuer (CNPJ, IE, address), the regime and the gateway's environment (production needs the certificate's day), the series of each kind, and up to 50 operation natures with their CFOPs and tax rules (CSOSN/CST, PIS, COFINS, rates held as decimals), all codes the company's own since the product embeds no tax table; the A1 certificate is **not uploaded** (it stays at the gateway, ADR 0073): only the day it expires is kept, with a warning on the page; the number of a document is the gateway's, so there is no counter
- [x] Emission port with a gateway adapter and a fake ([ADR 0006](adr/0006-fiscal-last.md), [ADR 0075](adr/0075-fiscal-gateway-port.md)) — a `FiscalGateway` port (submit, query, cancel, correction letter and number invalidation for the NF-e, idempotent by the document's reference, the gateway assigning the numbers), a fake that follows the real moves, an off adapter and Focus NFe (`FISCAL_GATEWAY=focus`, tokens per environment and the issuer's CNPJ in the environment), tested against a local stand-in with an ignored sandbox test that needs a real CNPJ and certificate; `GET /fiscal/gateway` and a note on the settings page
- [x] NF-e: issue from a sale, DANFE PDF, cancellation, correction letter, number invalidation, contingency ([ADR 0076](adr/0076-nfe-documents.md), in five pull requests: the store, issuing and the poll, the events, the accountant's package and the screens) — the Documentos page lists, issues, follows and cancels; the XMLs reach the package. **Proved against the fake gateway and a local stand-in of the Focus adapter, on both storage adapters and through the API; never against a real authority** (a real CNPJ and A1 certificate are needed even in homologation, ADR 0073). Simples Nacional only, customers with a CPF only; contingency is shown, never chosen by the product.
- [x] Two fiscal gateways, **each installation chooses its own** with `FISCAL_GATEWAY=focus|nfeio` ([ADR 0077](adr/0077-choosing-a-fiscal-gateway.md), [ADR 0078](adr/0078-gateway-reference-and-queued-events.md), [ADR 0079](adr/0079-nfeio-adapter.md)): the port serves a gateway that keeps our reference and one that assigns its own, and events answered at once or later; **every fiscal call that follows is built for both adapters**. Neither was exercised against its real service (NFE.io production issues are refused until `NFEIO_SEARCH_CONFIRMED=true`).
- [x] NFS-e through the national standard; municipal adapters only when the standard is not available ([ADR 0080](adr/0080-nfse-plan-and-settings.md), in three pull requests)
  - [x] The settings: the issuer's IBGE municipality code and the ISSQN terms of a service nature (national code, municipal code, how it is charged, rate), all the company's and its accountant's
  - [x] The document, the DPS numbering and the port's NFS-e calls ([ADR 0081](adr/0081-nfse-documents.md)) — an NFS-e kind with its own identity (no access key), the DPS numbered by the company once and never reused, issued from the **service lines** of a sale (the NF-e now takes the product lines only), `submit_nfse`/`query_nfse`/`cancel_nfse` for the fake and Focus NFe (from its documentation, never exercised); **NFE.io answers `Unsupported` and sends nothing**, because its published reference does not say how to find a service invoice by our reference
  - [x] The screens: the Documentos page lists NF-e and NFS-e with their kind, issues an NFS-e (every choice the person's; only a service nature with its ISSQN terms is offered), follows it by its number or its DPS, shows its own identity, downloads the DANFSe, cancels it and sends a rejected one again under the same DPS; it has no correction letter. **Proved against the fake and a stand-in, never a real authority.**
- [x] NFC-e and a minimal point of sale screen ([ADR 0082](adr/0082-nfce-and-point-of-sale.md)) — an NFC-e kind with an NF-e's identity, issued from the product lines of a sale (no change: what is paid adds up to the total), `submit_nfce`/`query_nfce`/`cancel_nfce` for the fake and Focus NFe (never exercised), **NFE.io answers `Unsupported`** (its consumer API is not written in the adapter yet); the **Caixa** page makes, completes and invoices a sale in one go. No correction letter; the DANFCe (HTML) is not kept.
- [x] Recipient manifestation for incoming NF-e and the gateway webhook ([ADR 0083](adr/0083-received-invoices-and-the-fiscal-webhook.md)) — the invoices issued against the company are a **mirror** of the gateway's list (fetched on request, newest versions only), the company declares awareness and then one final word through the gateway, recorded in the name of the person when the authority takes it; `POST /public/fiscal/{company_id}/webhook` (a secret in an operator-chosen header, `FISCAL_WEBHOOK_SECRET`) **only wakes the server up**: nothing it carries is believed. Focus NFe from its documentation, never exercised; **NFE.io answers `Unsupported`**.
- [x] The gateway's settings per company, edited in the app with the secrets sealed ([ADR 0085](adr/0085-gateway-settings-per-company.md)) — the company's own gateway kind, tokens, NFE.io ids and webhook secret, owner and admin only, AES-256-GCM with `FISCAL_SECRETS_KEY`, write-only, never in the trail
  - [x] Stored, sealed and shown: the page **Gateway fiscal** under Configurações, the API, both adapters and the contract
  - [x] The gateway that resolves a company's settings at the moment of a call (a `FiscalGatewayResolver`, per issuer and per document, never per pass; a row that is not enough is a `422` and never the server's), the webhook by company (the company's secret only, the same `401` for a company that does not exist), and the tests that one company's settings are never used for another's
- [ ] Tax calculation review by an accountant before the release — **stays unticked until a human accountant signs** [`docs/fiscal/accountant-checklist.md`](fiscal/accountant-checklist.md); software never ticks it ([ADR 0084](adr/0084-tax-calculation-behind-a-port.md))
  - [x] A tax calculation behind a port (`TaxCalculator`, `RuleRateCalculator`, `domain::item_tax`/`document_tax`): pure, from the company's own rates, tested against an independent integer reference; **connected to no issue**, and the regimes other than the Simples Nacional stay refused (a contract on both adapters holds it)
  - [ ] The accountant's review and signature (twelve points listed with what the code assumes), then what is sent to the gateway for the regular regimes

Exit criteria: a product sale and a service sale produce authorized documents in the
provider's test environment and the XMLs reach the accountant export.

## Phase 8 — Integrations and mobile (v0.9, done)

The plan, the order of the slices and what each settles are in [ADR 0086](adr/0086-integrations-and-mobile-plan.md).

- [x] Public API `v1` frozen: OpenAPI document, API keys per company, rate limits ([ADR 0015](adr/0015-public-api-and-webhooks.md), [ADR 0087](adr/0087-public-api-v1.md)) — keys with scopes over five areas (never members, accounting or fiscal), shown once and stored as a digest, rotated with a grace and revoked, with the key as the actor in the audit trail; `/v1` of parties, products and their stock, categories, payment terms, sales (made once by an `external_reference`) and the charges of a sale, in problem documents with `RateLimit-*` headers and a per-process limit per key; the hand-written document `docs/api/v1/openapi.json` held by tests to the router, to the real answers and to a frozen copy; the page **Chaves de API**
- [x] Webhooks: subscriptions, signed deliveries, retries, delivery log ([ADR 0088](adr/0088-webhooks.md)) — subscriptions per company (address checked, events, a secret that is editable and write-only, sealed), `sale.created|completed|cancelled`, `charge.paid`, `stock.below_minimum` and a test, raised by the adapters after their writes; deliveries signed in the Standard Webhooks scheme, tried again for about nine hours through the job queue (four workers), refused at every try for a target that is not public (no redirect, no proxy), a 30-day log with a way to send one again; the page **Webhooks**. The exit criterion is proved by a test with a real listener
- [x] Automatic bank statement sync through an aggregator adapter ([ADR 0011](adr/0011-bank-data-ingestion.md), [ADR 0089](adr/0089-bank-statement-sync.md)) — a `BankAggregator` port (off, a fake and **Pluggy**, picked by `BANK_AGGREGATOR`; the keys are the installation's and no company secret is stored), a connection per bank account (audited, one live per account), pulls of the posted movements through a job kind (first 90 days back, then from 7 days before the last day reached) into the same statement lines as an OFX file, once each; a bank that needs the person is shown, never retried; the page **Conexões bancárias**. **The Pluggy adapter has never run against the real service** and the consent widget is not embedded (ADR 0089 lists both)
- [x] First e-commerce connector as a separate package using only the public API ([ADR 0090](adr/0090-ecommerce-connectors.md)) — a fixed catalogue and the page **Configurações > Conectores** (several per company, owners and admins): turning one on makes the key (with the least scopes) and the webhook subscription through the same services as their pages, turning it off revokes the key and switches the subscription off, and what is shown is read from them every time; the package `connectors/woocommerce` (Node 22, no dependency, own CI job) imports nothing from the repository, polls the store's paid orders into open sales with a per-store reference and tells the store when a sale is completed or cancelled through verified webhooks. **Never run against a real store**; `scripts/e2e-woocommerce.py` runs it against the real API
- [x] PWA: installable, offline read of recent data ([ADR 0091](adr/0091-pwa-and-offline-reading.md)) — a manifest and a service worker (production only), the application's own files kept so the app opens offline, and an **opt-in per device** offline reading of an **allowlist** of the company's `GET`s (the dashboard's sources and the lists a person opens, and the profile), per person and company, cleared first at sign-out and on any `401`, expiring after 7 days, bounded (1 MB, 40 answers), with a banner that says the data is the device's copy and from when; no offline writes; the page **Uso offline** says what stays on the device. The rules are one module tested with fake `caches`/`fetch`; `scripts/e2e-pwa.py` checks them in a real Chromium

Exit criteria: a third-party client creates a sale and receives its webhook without any
private endpoint.

## Phase 9 — 1.0 hardening (in progress)

Planned in [ADR 0092](adr/0092-one-point-zero-hardening-plan.md), which says what 1.0 promises (the `/v1` API, forward-only
migrations, configuration names, file layouts), how each item below is proven and what it does not cover. One slice per pull
request, in this order:

- [x] PostgreSQL row-level security policies as a second barrier to the application checks ([ADR 0004](adr/0004-company-scoped-tenancy.md), [ADR 0093](adr/0093-row-level-security.md)) — a policy on every table with a `company_id` (a test fails when one has none), the company set per request from what was verified (the membership, the key, the token), the server serving requests as a role that owns nothing and bypasses nothing (it refuses to start otherwise) and the owner migrating; a test proves that with the application's check skipped another company's rows are not found and a write into them is refused; background work runs in a system scope and is **not** protected by it
- [ ] Backup and restore procedure, upgrade guide, published Docker image ([ADR 0096](adr/0096-operations.md), [`docs/operations/`](operations/README.md)) — **done and rehearsed:** a non-root image built in CI, a production compose example behind a TLS proxy with sign-in rate limiting, `scripts/ops/backup.sh` and `restore.sh`, a restore rehearsal in CI that compares every row and starts the server on the copy, and an upgrade rehearsal (run by hand) through consecutive releases in place with the way back by restore. **Still open:** the publishing workflow (`release.yml`) has never run, because nothing is tagged; the exit criterion needs a real upgrade between two tags
- [x] Migration importers: entries, open or paid, from CSV (done, [ADR 0064](adr/0064-entry-import-from-a-file.md), with `scripts/export-to-entries-csv.py` to convert a reference product's export); stock balances from CSV ([ADR 0094](adr/0094-stock-balance-import.md): `POST /stock/import` opens the stock of products with none, once per file name, checked whole before any write); [guidance for leaving the reference product](migration/leaving-a-reference-product.md) (never tried against a real export of any vendor)
- [x] Performance baseline on a 5-year dataset; indexes reviewed ([ADR 0097](adr/0097-performance-baseline.md), [`docs/performance/baseline.md`](performance/baseline.md)) — a deterministic seed and a benchmark (`scripts/perf/`), the keyset lists that sorted a whole company fixed (entries 67 → 5 ms and stock movements 83 → 3 ms measured, four more by inspection; flat with depth), one index justified by a plan; **the cash flow of a year by month is over its 1.5 s budget (p95 ~1.6 s)** because it recomputes what is left of every installment ever due, which the design derives and never stores
- [ ] User documentation complete in English; the UI behind an i18n layer with English and Portuguese catalogues ([ADR 0095](adr/0095-i18n.md): the layer, the checks and the shell and sign-in pages are in; **every other page is still Portuguese in both languages** until its area's pull request, and the English user guide is not written)

Exit criteria: an operator upgrades two consecutive versions in place with a restored backup
and all checks green.
Nothing has been tagged, so this is rehearsed between two commits first and closed by the first real upgrade between two
tags ([ADR 0092](adr/0092-one-point-zero-hardening-plan.md)); the phase is ticked only with the second.
