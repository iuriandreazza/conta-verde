---
title: "ADR 0094: Opening the stock from a file"
status: accepted
date: 2026-11-11
tags: [adr, stock, import, migration, phase-9]
---

# ADR 0094: Opening the stock from a file

## Context

[ADR 0092](0092-one-point-zero-hardening-plan.md) lists, among the importers a person needs to leave another product, the stock
balances. Registers (ADR 0025) and entries (ADR 0064) already come in from a CSV file; the stock does not, and it is the one
part of a migration that cannot be typed (hundreds of products, each with a quantity and a cost). Phase 5 made the stock a
**ledger of movements**, never edited, with the quantity, value and average cost all derived ([ADR 0054](0054-stock-movements.md)),
and gave every movement an optional origin `(kind, id, line)` under a partial unique index, which is what makes each step of a
document idempotent. An importer has to live inside that: no balance is ever written, only movements, and the average cost
after an import must be exactly what a person who had received the goods would have.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| A purchase receipt per file | Reuses the receipt's values | A receipt makes a payable too; an opening balance is not a debt |
| An inventory count from the file ([ADR 0056](0056-inventory-count.md)) | Already exists, corrects a stock to what was found | A count writes *adjustments* valued at the average cost, which is zero for a product with no stock: it cannot carry the cost the file gives |
| Manual movements, one by one, in the browser | Nothing new | Hundreds of products; no way to send a file twice safely |
| **An `in` movement per product, with its own origin kind** (chosen) | The ledger, the average cost and the audit trail are the ordinary ones; the origin makes sending twice safe | A new origin kind (one `CHECK` in the table) |
| Allow it on a product that already has stock, as a correction | One importer for every case | Sending the file on top of a running stock silently doubles it; the right tool for a correction is a count |

## Decision

`POST /companies/{id}/stock/import?reference=&on=&dry_run=` (the `purchases` area, writers only) takes a CSV body like the other
importers (comma or semicolon, any column order, UTF-8, 1 MiB and 1,000 rows at most, an unknown column refused). The columns are
`sku` and `quantity` (required) and `unit_cost` (what one unit cost, up to six decimals; blank means the product's own cost price,
and a product with none refuses the row). A quantity or cost may use the decimal comma (`12,5`) with the same refusal of the
ambiguous `1.234` as the products import.

- **`on` is the day of the balances** and **`reference` is the name the person gives the file** (up to 100 characters). Neither is
  guessed: the server reads no clock, and the reference is what the file's rows are remembered under.
- **Every row is checked before any is written**, and every problem of the file is told at once: an unknown SKU (a service has no
  stock and so no row), a SKU that repeats in the file, a quantity that is not an amount of up to three decimals above zero, a cost
  that is not one, a day or reference that is not usable, a product that already has movements. If one row is refused, **nothing is
  written**. `dry_run` goes through the same checks and writes nothing.
- **The movement of a row is an `in`** on the day, valued exactly like one typed by hand (`StockMovement::new` against the empty
  ledger the product must have): the cost is `quantity x unit_cost` to the cent, the average cost is derived. Its note says
  "Saldo inicial: <reference>". Its audit entry is the ordinary one. All the rows are written in **one** all-or-none call of the ledger's
  `record` (products locked in id order, the balances read again and the write retried when another one moved them), so a partial
  opening cannot be left behind.
- **Sending the same file twice adds nothing.** The origin is `(import, v5(company, reference, SKU), 1)`: the second time the row
  finds its movement and is `skipped`, as is a row of another file under the same reference. A row that was reversed by hand is
  written again (the origin index ignores a reversal's pair), which is the same rule as every other document.
- **A product that already has movements is refused**, whatever they are, unless they came from this reference. An opening balance
  is for a product that has none; correcting a balance is what an inventory count is for. This is the guard against sending a file
  on a running stock and doubling it. A quantity of **zero opens nothing** and is skipped with a note (an export lists products with
  nothing on hand).
- The **period lock** applies to the day like any movement: a day on or before a closed month refuses the whole file.
- The web app adds **Importar saldos** to the stock page (writers only): the name of the import and the day, then the usual CSV card
  with its template, a check that writes nothing, and the import.

### Migration

`20270111000000_stock_import_origin.sql` widens the `CHECK` on `stock_movements.origin_kind` to include `import`. Nothing else
changes.

### The guide

[`docs/migration/leaving-a-reference-product.md`](../migration/leaving-a-reference-product.md) says in what order to bring a company
over (registers, entries, stock), what each importer does and does not do, and how to check the result against the old product's own
reports. It names no product: it speaks of "a reference product's export", as `scripts/export-to-entries-csv.py` does, and it says
which parts were never tried against a real export of any vendor.

## Proof

`contract::imports_stock_balances`, on the in-memory and the Postgres adapters: a file with an unknown SKU, a negative quantity and a
repeated SKU is refused row by row and writes nothing; a zero opens nothing; a check writes nothing; the import leaves 10 at 5 (50, average
5) and 3 at the product's cost price 4 (12, average 4) with `import` origins; the same file, and the same rows in another file, add
nothing; another reference finds the product with movements and is refused; another company's file finds no such SKU; an empty
reference and an impossible day are refused. `opens_the_stock_from_a_csv_file_once_however_often_it_is_sent` goes through the API
(decimal commas, a blank cost, the positions, the second send, a missing reference `400`, a stranger `404`), and two web tests hold the
page (the buttons wait for a name; only writers see it). The stock tests of ADR 0054 are untouched and pass: the ledger and the average
cost are unchanged.

## Consequences

- The origin kind is new, so `stock_movements.origin_kind` has a sixth value that old binaries do not know: this is a forward-only
  migration like any other ([ADR 0092](0092-one-point-zero-hardening-plan.md)).
- A person who sent a wrong file cannot take it back with one click: the movements are reversed one at a time (a reversal is how the ledger
  undoes anything), or the product is corrected with a count. A whole-file undo is not built.
- The checks read every product of the company once and make a few queries per row (the origin and the ledger of the product), which is
  fine for 1,000 rows; a bulk read of the ledger would be the step if files grow.
- Opening a product that has **no SKU** is not possible, because the file names products by SKU. The guide says to give the products their
  SKUs first (the products import does).

## Revisit when

A migration needs an opening balance with a **date per row**, a **warehouse**, or a **lot** (ADR 0052 keeps one stock per company), or
a file of more than 1,000 products becomes usual.
