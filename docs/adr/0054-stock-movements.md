---
title: "ADR 0054: Stock movements, balances and the average cost"
status: accepted
date: 2026-11-06
tags: [adr, inventory, phase-5]
---

# ADR 0054: Stock movements, balances and the average cost

## Context

[ADR 0052](0052-purchases-and-inventory-plan.md) decided the model: one stock per company, a ledger of movements
that are never edited, a balance added up on every read, and the weighted average cost with one rounding rule. This
slice builds it and plugs the documents into it: the receipt of a purchase ([ADR 0053](0053-purchase-orders.md)), a
completed sale, a return and a cancelled sale ([ADR 0039](0039-sales.md)).

## Decision

### The ledger

A **movement** (`/companies/{id}/stock/movements`, `purchases` area, audited on creation) has a product, a kind (`in`,
`out`, `adjustment`), a signed quantity (never 0, up to 3 decimals), a signed **value** in cents, a day, an optional
origin (`purchase_receipt`, `sale`, `sale_return`, `inventory_count`, with the document's id and the line of it), an
optional `reverses` and a note. Only a product of kind `product` has stock: a service is refused (`422`). Nothing is
edited or deleted. The migration holds a `CHECK` for each rule one row shows (kind and sign agree, the value has the
quantity's sign or is 0, quantity at most 3 decimals and value 2, origin all or nothing) and **two unique indexes**:
one movement per origin and line (`WHERE reverses IS NULL`), and one reversal per movement. A composite foreign key
keeps a movement to its company's product.

### Valuation, in the domain

`StockBalance` (the sums of a product's quantity and value) and `StockMovement::new` hold the rules of ADR 0052 and
nothing else values a movement: an `in` is valued by the figure the document gives (a total in cents, or a unit cost of
up to six decimals times the quantity, to the cent); an `out` takes `round_to_cents(V × q / Q)`, or all of `V` when it
takes the whole balance, or 0 from a balance at or below zero; an adjustment up is valued at the average, one down is
an out. **The worked example** the tests hold (domain, both adapters and the API): 10 bought for 50 and 10 for 70 are 20
worth 120, average 6; selling 5 takes 30 and leaves 15 worth 90, still average 6; selling 15 takes 90 and leaves
nothing. Three bought for 10.00 and sold one at a time take 3.33, 3.34 and 3.33: exactly 10.00.

### Concurrency: the balance a movement was valued against

The value of an `out` depends on the balance at that moment, so the use case reads the balances of the products
involved (`balances`), values the movements and writes them with the balances it used (`record`). The adapter **locks
the products' stock** (`SELECT ... FOR NO KEY UPDATE` on the product rows in id order, a statement of its own), reads the
balances in the next statement, and refuses with `StockChanged` if any differs from what the use case saw; the use case
then reads again and retries (a few times, then `409`). The write is all or nothing for every movement of the call, with
its audit entries, so a sale with three lines never leaves one out. The in-memory adapter does the same under its lock.
This keeps the valuation in one place, the domain, with both adapters only comparing and storing.

A movement that repeats an origin is refused with `DuplicateOrigin` and a second reversal of the same movement with
`DuplicateReversal`; the use case reads what already exists and writes only what is missing, so a document's stock step
is **idempotent** and asking again finishes what a failure left undone.

### What the documents do

A narrow trait, `DocumentStock`, is what sales and purchases ask of the stock (implemented by `StockService`):

- **Completing a sale** writes an `out` per product line (a service skips), for the sale's day, after the sale is
  completed and its receivable made; all lines of the sale in one call. Asking again finishes what is missing.
  If the sale was cancelled meanwhile its outs are reversed at once.
- **Cancelling a completed sale** reverses its outs, after its receivable is cancelled and before the status is written,
  in the same half-done window as the receivable (asking again finishes it). Cancelling is refused while a return stands,
  so the two never meet.
- **A return** writes an `in` per returned line **at the cost the sale took**: the value of the sale's out of that line
  times the returned part of its quantity, cumulatively, so the returns of a line add up to what the sale took exactly.
  The return is stored first; if its stock write fails the return stays (like the refund, it is put right by hand, with
  a manual movement).
- **A receipt of a purchase** writes an `in` per product line, valued by the value the receipt stored for the item.
  `POST .../receipts/{id}/finish` makes the payable **and** the stock entries, so one idempotent step finishes both.

### Manual movements and reversals

A person writes an `in` (with a total or a unit cost), an `out` or an `adjustment` (signed) by hand
(`POST /stock/movements`); a manual `out` or adjustment down that would take the balance below zero is refused
(`StockInsufficient`, `409`). `POST /stock/movements/{id}/reverse` undoes a movement the person wrote by hand, once; a
movement a document made is undone through the document. An `in` is not reversed if the goods are no longer there.

### Reading

`GET /stock/movements` lists the ledger, newest day first, by `product`, `kind`, `from` and `to` (the keyset cursor of
dates, ADR 0018). `GET /stock/positions?on=&q=` lists the products that have stock with quantity, value and average
cost, **on a day the client sends** (movements through that day; the server reads no clock) or now when none, composed
from the products' port and the movements' `balances`, never one query across tables.

## Consequences

- `SaleService` and `ContractService` gain one generic (the stock behind `DocumentStock`), `PurchaseService` too, and
  `Ports` gains `StockMovements`.
- Selling without a balance is allowed (it leaves the balance negative, and the exit takes no value); ADR 0055 adds the
  per-company block.
- A negative balance distorts the average of what comes after it (a purchase into a negative balance averages the new
  cost against nothing); counting and adjusting (ADR 0056) is how it is put right.
- A sale's stock exits can be refused by the block (ADR 0055) after the sale is marked completed, if the balance changed
  in between: asking again, once there is stock, finishes it.

## Revisit when

A company has more than one stock (a location on the ledger), costs must follow FIFO or include freight and taxes, or the
balance must be read faster than adding up the ledger (a snapshot table per product and month).
