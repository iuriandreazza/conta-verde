---
title: "ADR 0042: Price lists"
status: accepted
date: 2026-11-06
tags: [adr, sales, pricing, phase-3]
---

# ADR 0042: Price lists

## Context

A product has one sale price. Many businesses sell the same product for different prices: wholesale and retail, a partner,
a promotion. Phase 3 asks for price lists that a quote, a sale or a contract can use. The lines of
[ADR 0037](0037-sales-module-plan.md) already carry their own unit price, so the question is only where that price comes from.

## Decision

### The register

A **price list** (`/companies/{id}/price-lists`, `sales` area, audited, replaced as a whole by `PUT` that must say `active`) has a
name (unique per company, ignoring case), a flag and **1 to 1000 prices**: a product and a unit price (zero or more, at most six
decimals, a string like every decimal of this API), each product at most once. The prices are replaced as a whole with the list in
one transaction, like the installments of a payment term (ADR 0021), and are listed with the list. The list endpoint takes `limit`,
`after`, `q`, `active` and `sort` like the other registers (ADR 0018).

Only a product a change adds must be usable (active, the company's) when the list is written; a list can still be edited after one of
its products is deactivated. The database refuses a product of another company with a composite foreign key and checks the shape of
a row (price from 0 to under 10^13 with at most six decimals, positions 1 to 1000, a product once per list); that a list has a price
and the count are the domain's.

### Choosing a list fills a price, nothing more

A list is **not part of a document**. A quote, a sale or a contract stores its lines with the unit price they were filled with, and
nothing remembers which list that came from: changing or deactivating a list never changes a document that was already written,
and the documents' API and tables do not change. The web form's lines editor loads the active lists and, when one is chosen, sets
the price of a line to the list's price for its product, **falling back to the product's sale price** when the list does not name
it; choosing another list, or none, prices the lines that already have a product again, and a new line takes the price of the list
chosen. The price stays a starting point the person can type over, as the product's price always was.

Not here: a default list per customer, discounts by quantity, validity dates, or prices in a currency other than the company's.
Each is a rule on top of this register and none of them changes what a line stores.

## Consequences

- A document cannot be traced to its list. If that is ever needed, a list id on the document is an additive change.
- Prices are filled on the client, so a client that skips the lists simply uses the product's price; nothing server-side depends on it.
