---
title: "ADR 0053: Purchase orders, their receipts and the payables they make"
status: accepted
date: 2026-11-06
tags: [adr, purchases, finance, phase-5]
---

# ADR 0053: Purchase orders, their receipts and the payables they make

## Context

Buying is the mirror of selling ([ADR 0039](0039-sales.md)): goods are ordered from a supplier, arrive in one or
more deliveries, and are paid by a payment term. [ADR 0052](0052-purchases-and-inventory-plan.md) settled the
`purchases` area and that stock is moved by receipts; this slice is the order, its receipts and the payable each
receipt makes. The stock side of a receipt comes with the ledger ([ADR 0054](0054-stock-movements.md)).

## Decision

### The register and its life

A purchase order (`/companies/{id}/purchase-orders`, `purchases` area, audited) has a number (the company's own
sequence, taken from `document_counters` as for quotes and sales), a supplier (a party with the supplier role),
the day of the order, an optional expected day (not before the day of the order), an optional payment term, the
**category of the payables** (an active category that is not the transfers one, with an optional cost center),
notes and the lines of [ADR 0037](0037-sales-module-plan.md) (`Line` is shared; a line's `unit_price` is the unit
cost, so the one rounding rule and the one way of writing a quantity are the same everywhere), and a status:

- `draft`: being written, the only state in which it changes (`PUT` replaces it as a whole).
- `approved`: agreed with the supplier; its items are fixed. It must be worth more than 0.
- `partially_received` and `received`: derived from the receipts (every line arrived whole means `received`).
- `cancelled`: final. Only a draft or an approved order that **nothing arrived for** is cancelled.

A partly received order can be **closed** (`POST .../close`): what has not arrived is given up and it becomes
`received`. What the order points at (supplier, term, category, cost center, products) is checked when it is
written, and only what an edit changes. A transition writes **only the status** (`set_status`, under the order's
row lock, naming the status it was made from, `PurchaseChanged` = `409`), so an edit that landed meanwhile is never
overwritten with an older copy.

### Receipts

A **receipt** records what arrived of an order that is approved or partly received: a day, an optional note and a
quantity per line (a line once, more than 0, at most 3 decimals, never more than was ordered and not received yet).
It is never edited or deleted. The value of what arrived is taken **cumulatively** (the rule of a return's refund):
for each line, `round_to_cents(line total × received so far / ordered)` less what was valued before, so the receipts
of a line add up to the line's total exactly and a discount lowers the cost. The value is stored with each item,
as a fact of the day, and is what enters stock ([ADR 0052](0052-purchases-and-inventory-plan.md)).

The adapter stores a receipt with the order **locked**, after checking that the order can still receive, that it has
exactly the receipts the use case worked the values out from (`PurchaseChanged`: ask again) and that no line takes in
more than was ordered (`ReceiptTooMuch`, `409`); the order's new status is written in the same transaction, with an
audit entry only when it changes. Two receipts of the same line at the same moment cannot both land.

### The payable of a receipt

Each receipt makes **one payable**: kind payable, the supplier as party, the order's category and cost center,
competence date the day of the receipt, the description "Compra nº N", and installments from the payment term
(`PaymentTerm::split`, counted from the day of the receipt) or one due on that day when there is no term. Its origin
is the receipt (`EntryOrigin::PurchaseReceipt`, a unique index per receipt), so it is made once. A receipt worth
nothing (goods the supplier gave) makes none.

The payable is validated **before** the receipt is stored; if its write fails after that, the receipt stays and says
`payable_id: null`, and `POST .../receipts/{id}/finish` makes what is missing and is safe to repeat (the step the sale
completion has by asking again). Like a sale's, what a receipt made is **not edited as an entry** of its own (`409`).

### Roles

`purchase-orders` is in the `purchases` area: the `stock` role writes it, `financial`, `sales`, `accountant` and
`viewer` read it, and a user with `purchases` write who receives goods writes a payable as its consequence (the areas
are coarse, [ADR 0026](0026-membership-roles-and-permissions.md)).

## Consequences

- Payables appear in the cash flow forecast through the entries' port with no change to it.
- `SalesReferences` becomes `DocumentReferences` (and `RegisterDocumentReferences`): it answers for every document
  that points at registers, and gains `supplier_is_usable`.
- A purchase is not cancelled after goods arrived; giving goods back is an adjustment of stock for now.
- The entries page does not yet link a payable back to its purchase (the entry says which receipt made it).

## Revisit when

A purchase return exists, a supplier's invoice must be matched to an order (ADR 0057 enters one from its XML), or an
order needs an approval by someone other than who wrote it.
