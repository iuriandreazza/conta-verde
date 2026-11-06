---
title: "ADR 0056: Inventory count and adjustment"
status: accepted
date: 2026-11-06
tags: [adr, inventory, phase-5]
---

# ADR 0056: Inventory count and adjustment

## Context

Stock drifts from the shelf: breakage, theft, goods that arrived without a purchase, a balance that a sale without
stock left negative ([ADR 0054](0054-stock-movements.md)). A company counts what is there, and the stock is brought
to what was found by **adjustments**, which are movements like any other: never an edit of a balance.

## Decision

### The count

A count (`/companies/{id}/inventory-counts`, `purchases` area, audited) has a number (the company's own sequence), the
**day of the count**, optional notes and 1 to 500 lines, each a product that has stock (a service is refused) counted
once, with **what was found**: 0 or more, up to three decimals. Its status is `open` (the only state in which it
changes, replaced as a whole by `PUT`), `applied` or `cancelled`, both final. The count holds **only what was found**,
never what the books held: that is read when it matters.

- **Looking at an open count** (`GET .../{id}`) shows, for each line, what the stock holds **now** (`expected`) and the
  `difference` (found less held), derived on every read, so people can count over days and see the books move.
- **Applying** (`POST .../apply`) writes, for each line whose found quantity differs from what the stock holds **when
  the write is valued**, one `adjustment` for the difference, on the day of the count, with the origin
  `inventory_count`, the count's id and the line, all lines or none. An adjustment up is valued at the average cost
  (0 when there is nothing on hand to average) and one down takes the average, the rules of ADR 0052/0054; a line that
  agrees writes nothing. The count is then `applied`.
- **Looking at an applied count** shows what each adjustment was: `difference` is the movement's quantity and
  `expected` is found less difference.
- **Cancelling** an open count writes nothing.

### Order and repeats

The adjustments are written **first and once** (the origin and line are unique, and the use case writes only what is
missing), the status **after**: asking again for a count a failure left open finishes it, and two applications at once
write the adjustments once. A count whose lines all agreed writes nothing, so it cannot tell whether it was applied
until the status is written; asking again after a failure **may** adjust a line to what was found if the stock moved
in between. A count is not edited or cancelled in the instant between its adjustments and its status.

### Roles

`inventory-counts` is in the `purchases` area ([ADR 0052](0052-purchases-and-inventory-plan.md)).

## Consequences

- `CountStock` is a second narrow trait that `StockService` implements, so `InventoryCountService` knows the stock only
  by it; `Ports` gains `Counts`.
- Counting a product with a negative balance up to a positive quantity values the adjustment at 0: the cost of what
  was found is unknown, and a purchase or a manual entry with a cost is how it is given.

## Revisit when

A count has to be made by several people at once (a count per sheet, summed), has to be frozen against sales while it
is made, or has to be valued at a cost other than the average.
