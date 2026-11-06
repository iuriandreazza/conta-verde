---
title: "ADR 0052: Purchases and inventory: plan, shared rules and what it changes underneath"
status: accepted
date: 2026-11-06
tags: [adr, purchases, inventory, phase-5]
---

# ADR 0052: Purchases and inventory: plan, shared rules and what it changes underneath

## Context

Phase 5 lets a company buy and know what it has: purchase orders that become payables, stock that moves when goods
are received, sold, returned or counted, the average cost of what is on the shelf, and a purchase entered from the
supplier's NF-e XML. It touches what earlier phases settled: a sale has no effect on anything but money
([ADR 0039](0039-sales.md) names stock as its revisit point), the products carry a minimum stock that nothing reads
([ADR 0019](0019-products-and-services.md)), and an entry's origin is a recurrence, a sale, a return or a charge fee
([ADR 0044](0044-collection-module-plan.md)). Those decisions are made once here; each slice's ADR says only what
is new. No `proposed` ADR covers stock or purchases, so nothing needs to be accepted first.

## Decision

### The slices

Each is one pull request with its own ADR, a migration, both adapters, the routes and the pages.

1. **Purchase orders** (ADR 0053): the register, its life, receipts (partial allowed) and the payable each makes.
2. **Stock movements and balances** (ADR 0054): the ledger, balances and average cost, and what receipts, sales,
   returns and cancellations do to it.
3. **Minimum stock alerts and the sale block** (ADR 0055).
4. **Inventory count and adjustment** (ADR 0056).
5. **Purchase entry from NF-e XML** (ADR 0057).
6. **Stock reports** (ADR 0058): position, movements and valuation.

The roadmap lists the XML before the movements; it comes after because an entry from XML is a purchase, a payable and
a stock entry, and so needs the three before it.

### A `purchases` area, and the stock role

A new area, `Purchases` (`purchases` in the permissions the web app reads), holds the route segments
`purchase-orders`, `purchase-imports` (the NF-e), `stock`, `stock-reports` and `inventory-counts`; each is added to
`area_of` and to its test ([ADR 0026](0026-membership-roles-and-permissions.md)). One area, not two, because what buys
is what stocks (a receipt is both) and the `stock` role has always meant the person who does both:

| role       | parties | products | finance | sales | purchases | members |
|------------|---------|----------|---------|-------|-----------|---------|
| owner      | write   | write    | write   | write | write     | write   |
| admin      | write   | write    | write   | write | write     | write   |
| financial  | write   | read     | write   | read  | read      | -       |
| sales      | write   | read     | read    | write | read      | -       |
| stock      | read    | write    | read    | read  | write     | -       |
| accountant | read    | read     | read    | read  | read      | -       |
| viewer     | read    | read     | read    | read  | read      | -       |

As for sales and returns ([ADR 0039](0039-sales.md)), the areas are coarse: a user with `sales` write who completes a
sale writes stock movements as its consequence, and one who receives a purchase (`purchases` write) writes a payable.

### One warehouse, no transfers

The macro plan says "one warehouse at first". A company has **one stock**; there are no locations and so **no
transfer movement** (a transfer between two stocks of one company changes nothing in total, and the first version has
one). The movement kinds are `in`, `out` and `adjustment`. Locations are the revisit point below: a `location_id`
column on the ledger and a transfer as an out and an in with one origin.

### The ledger

Stock is a **ledger of movements**, one row per product per event, and a balance is **added up on every read**, never
stored (the same rule as a bank account's balance, [ADR 0031](0031-transfers-and-balances.md)). A movement has:

- a **product** that is a `product`, never a service (a service has no stock);
- a **kind** (`in`, `out`, `adjustment`) and a **quantity**, signed and never zero, up to three decimals like a line's
  (`in` is positive, `out` negative, an `adjustment` takes the sign of what it corrects);
- a **value**, signed like the quantity, in **cents**: the cost of what moved (see below);
- the **day** it happened (`CalendarDate`, chosen by the person or the document, never read from the clock);
- an **origin**: what made it (`purchase_receipt`, `sale`, `sale_return`, `inventory_count`, `manual`), the id of that
  thing and the line it came from, so each event writes each of its movements **once** (a unique index over the origin
  and its line, and `DuplicateOrigin` is the success of a repeat, as for sales);
- an optional `reverses`, and a note.

A movement is **never edited or deleted**. Undoing one writes **another** movement, with the opposite quantity and the
opposite value, that names the one it undoes (a unique index, so a movement is reversed once, and a reversal is
itself never reversed). The balance is the sum, so the trail and the arithmetic agree and no row is rewritten.
The audit trail records each movement's creation ([ADR 0008](0008-audit-trail.md)), in the same transaction.

### Average cost, and the one rounding rule

The cost method is the **weighted (moving) average**. For each product the ledger gives its **quantity** `Q` and its
**value** `V` (the sums of both columns); the average cost is `V / Q`, derived on every read, shown with six
decimals, never stored. A movement is valued like this, **in the domain** (`StockBalance`), by the one function both
adapters call:

- an **in** (a receipt) is valued by the document: its cost in cents, as the purchase line's net value for the quantity
  received (so a discount lowers the cost, and the receipt's lines add up to what the payable pays);
- an **out** takes the **average cost at that moment**: `round_to_cents(V × q / Q)` (half away from zero, the rule of
  [ADR 0037](0037-sales-module-plan.md)), and **the out that takes the whole balance takes all of `V`**, so a product
  sold down to zero holds no cents of value; an out from a balance that is zero or negative has no value to take and
  is valued at 0 (a known distortion, below);
- an **adjustment** upward is valued at the average cost at that moment (at 0 when there is no balance), one downward
  is an out of its quantity; a count (ADR 0056) says which;
- a **sale return** puts back the cost the sale took: the value of the sale's out for that line times the part of the
  quantity returned, taken **cumulatively** like a refund, so the returns of a line add up to what the sale took exactly;
- a **reversal** carries exactly the opposite value of what it undoes.

Because the value of an out depends on the balance at that moment, **writing a movement locks the product's stock,
reads the balance in a statement of its own, values the movement with the domain function and inserts it, in one
transaction in the movements' port**; the lock-then-read split is the one of settlements
([ADR 0030](0030-settlements.md)). Two sales of the same product at the same moment are valued one after the other.

### How a sale and a purchase move stock

- **A receipt of a purchase** (ADR 0053) writes one `in` per line that is a product, for the quantity received, valued
  by the cumulative rule over the purchase line's net value (partial receipts add up to the line).
- **Completing a sale** (ADR 0039) writes one `out` per product line (services skip), after the sale is marked
  completed, atomically for all lines of the sale (**all or none**, so the block of ADR 0055 never leaves half a
  sale), idempotently (a repeat only finishes what is missing). It runs for a contract's cycle too, since that is a
  completed sale.
- **A return** writes an `in` per returned line, at the cost the sale took; **cancelling a completed sale** reverses
  its outs (cancelling is refused while a return stands, so the two never meet).
- A purchase is **not cancelled after a receipt** and a receipt is final: giving goods back to a supplier is an
  adjustment (or a purchase return, which a later phase may add).
- The stock steps are the same kind of step as the receivable's: they are **separate, idempotent writes composed from
  the ports**, never a query across tables, so a failure leaves the sale completed and **asking again finishes it**.

### Where the NF-e XML is read

Reading the XML is **parsing and validation only, with no network and no SEFAZ**: it lives in `infrastructure`
(`nfe_xml.rs`, a function from bytes to a plain draft, like `parse_ofx`), built on `roxmltree`, which never loads an
external entity and refuses a document type declaration. The size is limited before the parser sees it. The **rules**
that say whether an invoice is acceptable (the access key's check digit, that the items add up to the totals, that a
CNPJ is valid) are in the domain; use cases take drafts and know nothing of XML ([ADR 0025](0025-csv-import.md) is the
model). ADR 0057 has the details.

### Tests

The exit criterion is two tests, each on both adapters and through the API: **a purchase XML creates payables and
stock**, and **a sale reduces stock and the average cost is right against a worked example** that is written out
in ADR 0054 and held by a domain test, the contract test and the API test.

## Consequences

- The sales module now depends on a stock port to complete a sale: the stock steps are one more generic of
  `SaleService`, behind a narrow trait, and one more associated type of `Ports`.
- A migration per slice; none edits one already merged.
- A product's `minimum_stock` becomes meaningful (ADR 0055); `cost_price` stays the price the person typed and is not
  the average cost, which is derived.
- The web app gains a "Compras" and an "Estoque" group in the sidebar.

## Revisit when

A company needs more than one stock (locations and transfers), another cost method (FIFO), unit conversion between a
supplier's unit and the product's, a purchase return or a fiscal note for goods (stock then comes from NF-e emission
too), or the cost of a purchase must include freight and taxes apportioned over its items.
