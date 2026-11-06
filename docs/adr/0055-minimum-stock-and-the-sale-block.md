---
title: "ADR 0055: Minimum stock alerts and the block of a sale without balance"
status: accepted
date: 2026-11-06
tags: [adr, inventory, phase-5]
---

# ADR 0055: Minimum stock alerts and the block of a sale without balance

## Context

A product has carried a minimum stock since [ADR 0019](0019-products-and-services.md) and nothing read it; and
[ADR 0054](0054-stock-movements.md) lets a sale take the balance below zero. This slice makes the minimum tell
someone when stock runs low, and lets a company refuse a sale it cannot cover.

## Decision

### Alerts

`GET /companies/{id}/stock/alerts` lists the **active products that have less than their minimum now** (balance
below `minimum_stock`; a product without a minimum, a service and an inactive product are never listed), by name,
with the quantity on hand and the shortfall. It is derived on every read from the products' port and the movements'
`balances`, never stored; nothing is sent to anyone (a reminder by e-mail is a later slice of the notifications).
The list is not paged: it holds only what needs buying, and it scans the products 200 at a time.

### The block, a per-company setting

`stock_settings` (one row per company, audited as a whole, `GET/PUT /stock/settings` and `/stock/settings/history`,
`purchases` area) has `block_sale_without_balance`, **off until a company says so**, so the default behaviour of
ADR 0054 does not change. Replacing it with what it already is writes nothing.

When it is on:

- **Completing a sale** first asks the stock whether it can cover the sale (`DocumentStock::ensure_available_for_sale`:
  the quantity of each product, **summed over the lines that name it**, against its balance), before anything about
  the sale changes. A refusal is `409` in the API's JSON shape with a message that names the product and what is on
  hand (`not enough stock of Café: 2 on hand`), the order stays open and no receivable is made.
- The exits are then written with the **floor at zero** (`StockFloor::Zero`), atomically for all lines, so another
  sale that took the stock between the check and the write is refused at the store; if that happens after the sale
  was marked completed, asking to complete again finishes the exits once there is stock.
- A contract's billing job completes sales too: a cycle that the stock cannot cover is not billed, it counts as
  failed in the job's report and is tried again at the next pass.
- Returns and cancellations only put stock back and are never refused.

A company that turns it off again makes the sales it could not before; their exits leave the balance negative, which
counting and adjusting (ADR 0056) puts right.

## Consequences

- `StockService` gains the settings port as its third generic, and `DocumentStock` one method.
- The Estoque page shows the alerts and the setting.

## Revisit when

An alert has to reach a person without them opening the page, a product has a maximum or a reorder point that is not
the minimum, or the block has to be per product or per kind of sale.
