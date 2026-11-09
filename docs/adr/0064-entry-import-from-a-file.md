---
title: "ADR 0064: Entry import from a file"
status: accepted
date: 2026-11-06
tags: [adr, finance, import, migration]
---

# ADR 0064: Entry import from a file

## Context

A company that leaves another system brings years of receivables and payables, many already paid. The imports of
[ADR 0025](0025-csv-import.md) cover parties and products only, and typing thousands of entries by hand is not an option.
The export of the system being left has what an entry needs (category, cost center, party, dates, amounts, what was paid
and when), but not what an idempotent import needs: no row has a code of its own, so nothing tells the first run of a file
from the second, and an entry is never deleted (only cancelled once, [ADR 0037](0037-sales-module-plan.md)), so a duplicate
run leaves permanent noise.

## Options considered

1. **One entry per document, with installments** — needs a document code the exports do not have; installments of one
   document cannot be told from unrelated entries that look alike.
2. **A generic bulk endpoint taking JSON** — the web app cannot send a file's worth of rows through the assistant's channel,
   and a person cannot pick a JSON file.
3. **A CSV import like parties and products, one row per entry with one installment** — the person picks a file; every
   check and every rule stays in the domain; a script outside the app turns a foreign export into the file.

## Decision

Option 3: `POST /companies/{company_id}/entries/import` (area `finance`, write; body is the file, `?dry_run=true` to only
check) and an import control on the entries page.

- **A row is one entry with one installment**, optionally already settled. Columns, in any order, `;` or `,`: `reference`,
  `kind` (`receivable` or `payable`), `description`, `party_document`, `party_name`, `category`, `cost_center`,
  `competence_date`, `due_date`, `amount`, `settled_on`, `applied`, `interest`, `fine`, `discount`, `bank_account`.
  `reference`, `kind`, `description`, `category`, `competence_date`, `due_date` and `amount` are required; `bank_account` is
  required with `settled_on`, and the other settlement columns are refused without it. A blank `applied` settles the whole
  installment.
- **Idempotency is an origin.** `EntryOrigin::Import(ImportId)` joins the recurrence, sale, return, charge fee and purchase
  receipt origins: a column and a unique index on `financial_entries`, inside the existing at-most-one-origin `CHECK`. The id
  is a UUID version 5 of the reference in the namespace of the company, so the same reference is the same id in that company
  and a different one in any other, and the entry keeps nothing but the id. Editing an imported entry is allowed; it is an
  ordinary entry that remembers where it came from.
- **Names, not ids.** The use case finds what a row names among the **active** records of the company: a category by its
  full path (`Root > Child`, a root by its name, matching ignoring case, never the transfers category, because names repeat
  across trees and only a path is unique among siblings), a cost center and a bank account by name, a party by document and
  else by name (when several parties share a name, the one that plays the role of the entry's kind: customer for a receivable,
  supplier for a payable; otherwise the row is refused as ambiguous). A blank party is no party. The lookup is one trait,
  `EntryLookups`, answered by the registers' own ports like `EntryReferences`, so both adapters share it.
- **The whole file is checked before anything is written.** Every row is built as when registering by hand (the entry's and
  the settlement's own rules, an `applied` above the installment refused up front) and every problem comes back with its
  row; if one row is refused nothing is written. A reference repeated within the file refuses the second.
- **Each row is its own write** with its own audit entry (the entry, then its settlement, each audited as when done by hand).
  A row whose reference already exists is **skipped**; if that entry never got its settlement (a failure between the two
  writes), the settlement is made now, so sending the file again finishes what a failure left. At most 2,000 rows and 1 MiB per
  request: a longer file is sent in pieces, which the web control does (a dry run of every piece first, then the writes).
- **Never recreated:** sales, quotes, invoices, recurrences (a generated entry would duplicate the imported ones) and
  transfers between own accounts (they are not entries: they go through `/transfers`).

The conversion of a foreign export into this file is `scripts/export-to-entries-csv.py`, outside the application.

## Consequences

- A migration adds a column and a unique index; nothing existing changes.
- Amounts stay exact: the file carries decimal strings, the script works in whole cents, and no float is ever summed.
- A row cannot be corrected by sending it again with another value: the reference already exists and the row is skipped.
  A wrong import is corrected by editing or cancelling the entries, as with any entry.
- The importer does not check that an entry that was paid is reflected in the bank account's opening balance; balances are
  checked against the foreign system after the import (`GET /balances`).

## Revisit when

- A foreign export carries a stable code per document, which would let installments of one document become one entry.
- Files regularly exceed what the web control can send in pieces, which would call for a job ([ADR 0059](0059-job-queue.md)).
