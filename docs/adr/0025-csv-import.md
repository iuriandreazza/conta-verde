---
title: "ADR 0025: CSV import of registers"
status: accepted
date: 2026-11-05
tags: [adr, registers, import]
---

# ADR 0025: CSV import of registers

## Context

A company arriving from a spreadsheet or another ERP has hundreds of customers and suppliers and a
few hundred products to bring along. Typing them in one by one is the first thing that would send
someone back. The Phase 1 item covers parties and products; this ADR sets the rules both follow
and builds the parties side first, the products side reuses it.

Two things pull against each other: a person fixing a file wants **every** problem at once and
wants to send the file again without fear, while the rows are written by use cases that each write
a row and its audit entry atomically ([ADR 0008](0008-audit-trail.md)).

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| One transaction for the whole file | A file is imported or it is not | A new batch port on every adapter, a duplicate found at row 600 refuses 599 good rows, one long transaction |
| Row by row, report what failed | No new port, a good row is never lost | A half-imported file after a failure |
| **Check every row first, then write row by row; skip what is registered (chosen)** | The common failures (a bad row, a repeated document) write nothing; sending the file again is safe for rows with a key; no new port | A storage failure in the middle leaves the rows before it, and rows without a key repeat if sent twice |

## Decision

### The file

- One header row names the columns, in any order and in any case; the names are the register's
  field names (`name`, `document`, `postal_code`, ...). A column the import does not know is
  **refused**, not ignored, so a misspelled one does not silently drop a field. A column that
  repeats, a missing required column, a file that is not UTF-8 (a spreadsheet's plain "CSV" is
  often Windows-1252), a file with no rows, more than **1000 rows** or more than **1 MiB** is
  refused whole, with a message that says what to change.
- A comma or a semicolon separates the cells, whichever the header uses (Brazilian spreadsheets
  save with `;`); quotes may hold the separator and a line break; a leading byte-order mark and
  blanks around a cell are dropped; a blank cell is not set. Rows are numbered as a spreadsheet
  shows them, the header being row 1.
- **Parties:** `name` is required, and at least one of `customer`, `supplier`, `carrier`, `seller`
  must be a column (`sim` or `não`, also `s`/`n`, `true`/`false`, `1`/`0`; blank is no). The
  address is `postal_code`, `street`, `number`, `complement`, `district`, `city`, `state`, all or
  nothing as in [ADR 0022](0022-party-roles-address-and-editing.md). Tags go in `tags`, separated by
  `|` (a comma or a semicolon would clash with the file's own). **Contacts have no column**: several
  people per party do not fit a row, and are added by hand.

### What an import does

- `POST /companies/{id}/parties/import` takes the file as the request body; `?dry_run=true` only
  checks. The answer is `200` with `valid`, `created`, `skipped` and `refused`, the last two lists of
  `{row, message}`. A file that cannot be read at all is `422` (or `413` past the size).
- **Every row is checked before any is written**, with the same rules as registering one, and a
  document that repeats within the file is refused (`the same document as row 2`). If a single row is
  refused, **nothing is created**, and the report lists every refused row, including the ones that
  could not even be read (`customer must be sim or não`).
- Then the rows are written **one by one**, each with its own `created` audit entry under the user
  who imported. A row whose document is **already registered is skipped, never overwritten**:
  an import creates, it does not update. This also makes sending the same file twice safe for the
  rows that have a document.
- It is not one transaction. A storage failure in the middle answers `500` and leaves the rows
  before it; sending the file again finishes it. **A party without a document cannot be recognized**,
  so the same file sent twice repeats those rows: the dry run exists to look before sending, and the
  page says that a skipped row means "already registered".
- **The registry is not asked about the CNPJs of the file** ([ADR 0017](0017-cnpj-registry-lookup.md)).
  Hundreds of calls would use the installation's shared rate limit and leave most rows unverified
  anyway. A CNPJ is stored as `Unverified`, so the list offers "check again" for it; with
  `CNPJ_LOOKUP=off` it is `NotApplicable`, as for any registration, which is what
  `CnpjRegistry::is_enabled` is for. CPFs are as always.

### Layers

Reading text into a table is a format concern and lives in `api` (`csv_import.rs`, on the `csv`
crate: hand-rolled quoting rules are a bug waiting for a customer's file). The use case,
`PartyService::import`, takes rows that already are drafts and knows nothing of CSV. Both registers
share `ImportRow`, `ImportReport` and the limits.

## Consequences

- The web page offers a template to download, "Verificar arquivo" (the dry run) and "Importar", and
  lists the problems with their rows.
- The 1000-row limit is a guard for a request that is read in memory and written one row at a time;
  a bigger file is split by the person. Background jobs ([ADR 0009](0009-postgres-only-background-jobs.md))
  are the answer if that stops being acceptable.
- There is no permission for importing yet: any member of the company can, as for any write, until
  roles exist (Phase 1).
- Updating registered rows from a file (matching by document or SKU) is deliberately left out: it
  turns a typo in a column into a silent overwrite of many records.

## Revisit when

Products are imported (the same rules, matching by SKU), roles exist and importing becomes a
permission, files of thousands of rows are common, or people ask to update registers from a file.
