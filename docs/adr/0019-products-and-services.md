---
title: "ADR 0019: Products and services as one register"
status: accepted
date: 2026-11-05
tags: [adr, registers, money]
---

# ADR 0019: Products and services as one register

## Context

Quotes, sales, purchases, stock and fiscal documents all hang from the thing that is sold. The
roadmap asks for SKU, unit, NCM, prices, minimum stock and an active flag. A product and a
service share most of this and differ in two ways: a service is not stocked and has no NCM (its
tax classification is a different code, handled with the fiscal phase).

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Two tables and two registers | Each has exactly its fields | Every list, quote item and report needs a union; the sale screens treat them alike |
| One table with a `kind` and nullable columns (chosen) | One list, one id space, items on a quote point at one table | The rules that differ must be enforced in the domain and again by CHECK constraints |
| One table with a JSON column for kind-specific data | No nullable columns | Loses types and constraints exactly where fiscal data will live |

## Decision

One `products` table and one `Product` entity with `kind` (`product` or `service`).

- **Kind is fixed at registration.** Stock and fiscal data hang from it, so an edit that changes
  it is refused (`422`). A service has no NCM and no minimum stock; sending them is refused
  instead of stored and ignored.
- **SKU** is optional, kept in upper case (`abc-1` and `ABC-1` are the same SKU), limited to
  letters, digits and `. - _ /`, at most 60 characters, unique per company when present. A
  duplicate is `409`.
- **Unit** is required: 1 to 6 letters or digits in upper case (`UN`, `KG`, `M2`), the width of
  the NF-e unit field. There is no table of units yet.
- **NCM** is eight digits, written with or without dots. Only the shape is checked; whether the
  code exists is a table that changes and belongs to Phase 7.
- **Prices and the minimum stock** are decimals (`NUMERIC(19, 6)`, [ADR 0003](0003-money-as-decimal.md)),
  optional, never negative, and travel as strings in both directions. See below.
- **Inactive** products stay in lists unless `active=true` is asked for, because someone who
  deactivated a product must find it again to bring it back. There is no delete.
- **Editing** is a full replacement (`PUT`) and is audited as one entry with the fields that
  changed. A request that changes nothing writes nothing and records nothing, so the trail
  lists changes, not attempts ([ADR 0008](0008-audit-trail.md)).
- **Lists** follow [ADR 0018](0018-list-endpoints-search-filter-sort.md): `q` over name and
  SKU, `kind=product|service`, `active=true|false`, `sort=name|-name`.

### Decimals

- Parsing is strict: digits with an optional fraction, at most 13 integer and 6 fractional
  digits. No sign, exponent, thousands separator, comma or spaces; more precision is refused,
  never rounded, because each rounding is a named rule of its own.
- The value is kept without trailing zeros (`12.50` is read back as `12.5`), so it reads the
  same from Postgres, which returns `12.500000`, and from memory, in the API and in the audit
  trail. Nothing downstream formats a number by hand.
- The API answers a price sent as a JSON number with the usual JSON error. The web app converts
  what a Brazilian types (`1.234,56`) into the plain form before sending and formats the answer
  with string operations; it never turns a price into a `Number`. A number written with dots and
  no comma that could be thousands (`1.234`) is refused until the person says which they meant,
  since both readings are valid decimals and either would be stored silently.
- Amounts are bound to Postgres as text and cast, and read back as text, which avoids a
  database feature for the decimal type.

## Consequences

- Concurrent edits of one product are last write wins, and the "from" of the second entry is what
  that person saw. A `version` column with a `409` on mismatch closes it when more than one person
  edits the same registers routinely.
- Prices are one list price and one cost. Price lists, per-customer prices and units of
  conversion belong to the sales phase.
- Stock balances, average cost and movements come with Phase 5 and reference this table; the
  minimum stock here is only a threshold.
- Other registers (categories, bank accounts) can copy this shape: a domain type that validates
  and normalizes, a port with a contract test, a PUT that replaces and audits the difference.

## Revisit when

Services need their own fiscal classification (LC 116 item, NBS), or a product needs variants
(size, colour), which would add a parent table rather than more columns here.
