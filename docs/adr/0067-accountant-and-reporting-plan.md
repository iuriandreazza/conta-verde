---
title: "ADR 0067: Accountant and reporting: plan, shared rules and what it changes underneath"
status: accepted
date: 2026-11-09
tags: [adr, accounting, reporting, phase-6]
---

# ADR 0067: Accountant and reporting: plan, shared rules and what it changes underneath

## Context

Phase 6 gives the company's external accountant what they need to close a month: their own login, a way to freeze what
they have closed, the managerial income statement, and every number of the month in files their own software can read.
It touches what earlier phases settled on purpose: every dated record could be changed or undone at any time, the
`accountant` role was only a viewer with another name ([ADR 0026](0026-membership-roles-and-permissions.md)), the DRE
groups were stored but never summed ([ADR 0012](0012-managerial-accounting-model.md)), and the only way out of the
product was the browser's own CSV of a list. Those decisions are made once here; each slice's ADR says only what is new.

Two proposed ADRs belong to the phase and are **accepted** with this one: [ADR 0012](0012-managerial-accounting-model.md)
(it is already `accepted`; this phase builds its DRE) and [ADR 0013](0013-accountant-access-and-exports.md) (accountant
access and exports, which is now `accepted`; where it is more general than what follows, what follows is the decision).

## Decision

### The slices

Each is one pull request with its own ADR where it has something new to decide, a migration where it needs one, both
adapters, the routes and the pages. The order of the roadmap holds; nothing forced a change.

1. **This plan.**
2. **The accountant's access** (decided here, no ADR of its own): the `accounting` area and the table below, with a
   test that tries every route with every role.
3. **Period lock** (ADR 0069).
4. **DRE by month and cost center** (ADR 0070).
5. **Exports and the Contador page** (ADR 0071).
6. **Scheduled package by e-mail** (ADR 0072).

### The accountant is invited like any member, and reads everything

An accountant is a membership with the role `accountant`, made by the invitation of [ADR 0027](0027-invitations.md): the
owner or an admin invites the address, and the accountant, who may have an account already, accepts with their own
login. A person who is the accountant of several companies has one membership in each ([ADR 0026](0026-membership-roles-and-permissions.md)).
There is no separate accountant account and no way into a company that was not invited.

The accountant **reads every area but `members`** (who belongs to the company stays with those who run it) and **writes
none of the business data**: no entry, party, product, sale, purchase or stock movement. That is what the table already
said; what changes is what the role is for, and one new area:

A new area `Accounting` (`accounting` in the permissions the web app reads) holds the route segment `accounting`: the
period lock, the exports, the schedule of the monthly package. Its table row is the only place where the accountant
writes:

| role       | parties | products | finance | sales | purchases | accounting | members |
|------------|---------|----------|---------|-------|-----------|------------|---------|
| owner      | write   | write    | write   | write | write     | write      | write   |
| admin      | write   | write    | write   | write | write     | write      | write   |
| financial  | write   | read     | write   | read  | read      | read       | -       |
| sales      | write   | read     | read    | write | read      | read       | -       |
| stock      | read    | write    | read    | read  | write     | read       | -       |
| accountant | read    | read     | read    | read  | read      | **write**  | -       |
| viewer     | read    | read     | read    | read  | read      | read       | -       |

Why the accountant writes `accounting`: closing a month is what an accountant does, and the exit criterion is that they do
it **from their own login**. The lock changes no record; it only refuses later changes to the ones it covers. `financial`
reads `accounting` but does not close a month: the person who enters the data should not be the one who freezes it.
Everything under `accounting` that is a `GET` is a read for every role that reaches it; the exports contain nothing a
reader of the other areas cannot already see, except that they gather it, which is why they are an `accounting` read and
not a free-for-all of the `finance` area.

The **test** for the table walks every company route of the router with the accountant and the viewer and expects `403` on
every write of every area but `accounting` (a `POST` that only previews counts as a write: the route decides by method),
and a reply that is not `403` on every read outside `members`; a second test reads a list from each area and expects `200`.
The walker is read from the router's own source, and fails if it misses a company route, so a route added without a decision
fails it ([ADR 0026](0026-membership-roles-and-permissions.md)'s
fail-closed rule, applied to the whole table).

### Period lock: what it is, and which date locks each record

The company has one **lock date**, `locked_through`, the last day of a month (the lock closes whole months; there is no
way to close half of one) or none. **A write that makes, changes or undoes what the books hold on a day on or before it
is refused** with `409` and a message that names the lock date; reopening is the only way past it. What "the books hold"
is a short list, on purpose: records that move money or stock, and the transitions of documents that make them. A draft,
a quote, an open order or an open count is not in the books, so it can still be written in a closed month and is refused
only when it becomes one.

| What is written | Date that locks it |
|-----------------|--------------------|
| An entry (register, replace, cancel, import, made by a recurrence, a sale, a return, a charge fee) | its **competence date** |
| A settlement, and reversing one | the settlement's **`settled_on`** |
| A transfer, and reversing one | its **`occurred_on`** |
| An opening balance | its day |
| A stock movement, and reversing one | the movement's **day**, and for a reversal also **the original's** |
| Completing a sale, cancelling a completed one | `sold_on` |
| A sale return | `returned_on` |
| A purchase receipt | `received_on` |
| Applying an inventory count | the count's day (its adjustments carry it) |

Rules that follow:

- **An edit that moves a date is refused on either the old or the new date.** An entry taken out of a closed month and put
  in an open one changes the closed month's figures just as one put in.
- **Reversals and cancellations of locked records are refused even when done today**: a reversal is a change to the day of
  the original (the books of that day would no longer add up to what was closed), which is why a settlement made on a
  closed day is not reversed from an open one. What to do after the fact is a new entry on an open day (a correction), or
  reopening.
- **A settlement of an entry whose competence is closed is allowed when the settlement's own day is open**: an October
  invoice paid in November changes November's cash and nothing of October. The entry itself is not touched.
- **Attachments are not locked**: a missing document is added to a closed month's entry all the time, and it changes no
  figure.
- **Automatic flows** are refused like any other write and then **retry or note**: the recurrence and contract passes skip
  what a lock refuses and try again on the next pass (so a reopened month catches up, and a closed one stays closed); a
  payment webhook whose payment day is closed settles nothing and is kept as a paid charge **with a note**, as
  [ADR 0046](0046-webhooks-and-automatic-settlement.md) already does for what cannot be applied.
- **Who locks and reopens**: anyone with `accounting` write (owner, admin, accountant). **Closing** moves the date forward;
  **reopening** moves it back (or removes it) and requires a short reason. Both are audited like any write
  ([ADR 0008](0008-audit-trail.md)), with the old and the new date and the reason, so "who reopened October" has an answer.
- **How it is enforced**: by the use cases, through a port that tells the lock date, held by the services that write dated
  records, **before the first write of the operation**; no migration trigger (a rule that looks at another table is a
  use-case rule, [ADR 0002](0002-layered-cargo-workspace.md)). The wiring is decided in slice 3; the rule is the same in
  both adapters and the contract tests run it on both.
- **Known limit**: the check is made before the write, not inside the same transaction. A write that began before the
  month was closed can land just after. Closing is rare and the accountant checks the month's figures after it; the
  limit is written in ADR 0069 and revisited if it ever bites.

### The DRE

The **managerial income statement** is derived on every read, never stored, from the entries ([ADR 0012](0012-managerial-accounting-model.md)):

- **Basis: competence.** An entry counts, **by its total**, in the month of its competence date; its installments and
  settlements are irrelevant to the DRE (cash is the cash flow's). Cancelled entries do not count. The transfers group and
  entries of the transfers category never appear.
- **Sign.** The amount of a receivable counts **positive** and a payable **negative** in its group, so a group's figure is what
  the group contributed to the result (revenue positive, costs negative; a refund booked as a payable in a revenue category
  reduces the revenue).
- **Lines**, in this order, with the three subtotals worked out from them by the domain and never typed:
  `gross_revenue`, `deductions` → **net revenue**; `costs` → **gross profit**; `operating_expenses` → **operating
  result**; `financial_result` → **net result**. Below the line, outside the result and shown apart:
  `investments` and `outside_result` (loans and owner capital move cash, they are not earned or spent).
- **By month and by cost center.** A span of at most 24 months gives one column per calendar month and a total; the same
  span can instead give one column per cost center (and one for entries without one) and a total; either can be narrowed
  to one cost center or to "without a cost center". A subcategory rolls up into the root that carries the group.
- The DRE says in plain words that it is **managerial**: it depends on how the company classified its entries, and it is not
  the statutory statement ([ADR 0012](0012-managerial-accounting-model.md)).
- It is a `finance` read (`GET /dre`), like the other reports, with a page, a chart and the export and print every list has.

### Exports

- **What a month's package holds**, for a span `from`..`to` (at most 366 days): **entries** (one row per installment, by
  the entry's competence date), **settlements** (one row each, by `settled_on`, the cash basis), **transfers**, and **the
  files attached to the entries** of the span, zipped. Entries cancelled and settlements and transfers reversed are left
  out: they have no effect on the books, and the audit trail ([ADR 0008](0008-audit-trail.md)) keeps them. The layout of
  each file's columns is written in ADR 0071 and does not change without a new ADR.
- **Formats.** CSV, UTF-8 with a byte-order mark, `;` as separator, `.` as decimal point, dates `YYYY-MM-DD`, amounts exactly as
  stored (never through a float), a cell that a spreadsheet would run as a formula quoted the way the web app's CSV does.
  XLSX of the same columns. **No new crate**: an XLSX is a zip of five small XML parts and a zip may store its files
  uncompressed, so both are written by hand in `api` (a zip writer with the CRC-32 table, and the parts), tested byte by byte
  like the quote's PDF ([ADR 0038](0038-quotes.md)). The cost is files that are bigger than compressed ones and no styles in
  the spreadsheet (dates are text); the gain is nothing new to audit and no native build.
- **Fiscal XMLs and PDFs.** What exists today is **the files someone attached to an entry** ([ADR 0036](0036-entry-attachments.md)):
  the supplier's NF-e XML and the boleto or invoice PDF belong there, and they go into the zip under the entry. The
  product does **not** keep the XML of an NF-e it enters ([ADR 0057](0057-purchase-entry-from-nfe-xml.md)), and it does not
  issue fiscal documents at all: that is Phase 7 ([ADR 0006](0006-fiscal-last.md)), whose exit criterion is that its XMLs
  reach this export. The quote's PDF is a commercial document, not a fiscal one, and is not in the package. The Contador page
  says so plainly.
- **Not in the package, on purpose:** the audit log and the stock position (reachable as their own pages and CSVs today), SPED
  files, any vendor's own layout ([ADR 0013](0013-accountant-access-and-exports.md): added behind one interface only when a
  contributor maintains a public layout).
- **Limits** so that a package fits in memory: at most 50,000 rows per file, 500 attached files and 200 MiB in all; past them
  the answer is `422` telling the person to narrow the span, never a cut-off file.

### The monthly package by e-mail

Scheduling reuses what exists ([ADR 0059](0059-job-queue.md), [ADR 0048](0048-notifications-and-dunning.md)):

- A per-company switch and a **day of the month** (1 to 28). The **hourly pass** sends, for the **previous month**, once the day
  has come, an e-mail through the queue to **every member whose role is `accountant`**. No other address can be typed in: the
  link below needs a login, so a recipient is someone who has one.
- **The e-mail carries no company data and no file.** It says that the month's package is ready and links to the
  **authenticated** Contador page for that month (`{PUBLIC_URL}/...`), where the accountant downloads the files after
  signing in. Why not attach the files or put a tokened download link in the e-mail: the e-mail transport is text only
  today, a package is megabytes, and a link that downloads a company's books without a login would be a way to leak them
  to whoever has the message. Login-gated is the only option that cannot leak.
- **Once per company and month**: a row in a table claimed by one statement (unique `(company, month)`) before anything is
  queued, so however often the hourly pass runs, and on however many instances, the month goes out once. A pass that fails to
  queue gives the claim back. A company with no accountant sends nothing and the page says so. Turning the switch on in the
  middle of a month may send the previous month that day.

### The web app

Every slice ships its pages with Vitest tests that assert what the person sees, and is checked in the real app in a browser.
A page gates its forms and buttons on `useCanWrite("accounting")` or the area it belongs to. The two-level menu gets a
**Contador** entry (the lock, the exports, the schedule) and the DRE joins the reports. Layout follows the design tokens and
the reference product only for structure ([docs/design](design/README.md)); nothing of its name, copy or screenshots.

## Consequences

- An accountant needs one login and no help from the company to close and download a month; the company decides who they are
  by inviting them and can end it by removing the membership ([ADR 0028](0028-member-management.md)).
- The lock touches every service that writes a dated record: finance, sales, purchases and stock. Its tests run on both
  adapters because the rule is in the use cases and the adapters only answer what the lock date is.
- Closed months do not change, so a report read later gives the figures the accountant closed (the stock reports of
  [ADR 0058](0058-stock-reports.md) were built to say the same).
- Everything the package holds is derived on read; nothing is stored but the lock, the schedule and the claim of a month.
- No statutory statement, no SPED and no fiscal XML of our own come out of this phase.

## Revisit when

Closing a month must be undone only by the owner (the table says "anyone with `accounting` write" today); the check before
the write loses a race that matters (then the lock moves into the transaction of the adapters); an accountant wants to
close half a month; the package must carry its files in the e-mail; or a vendor publishes a layout worth supporting.
