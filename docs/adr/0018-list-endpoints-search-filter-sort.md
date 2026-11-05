---
title: "ADR 0018: List endpoints share one search, filter and sort convention"
status: accepted
date: 2026-11-05
tags: [adr, api, registers]
---

# ADR 0018: List endpoints share one search, filter and sort convention

## Context

Every register (parties, then products, categories, bank accounts, entries) needs the same
three things on its list: find a row by typing, narrow by an attribute, choose the order.
Keyset pagination was already decided ([ADR 0015](0015-public-api-and-webhooks.md)) and built
for parties. If each register invents its own parameters, the SPA, the public API and the CSV
export end up with different vocabularies, and every new list reopens the questions below.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| One generic filter expression (`filter=name~ana,kind=supplier`) | Any register supports any filter with no new code | A mini query language to parse, document and keep injection-safe; every column becomes filterable by accident |
| A fixed, small set of named parameters per register (chosen) | Each parameter is a decision; trivial to validate; SQL stays static | A new filter is a code change |
| GraphQL | Clients pick fields and filters | A second API style for a CRUD-heavy product; see ADR 0015 |

## Decision

A list endpoint takes, besides `limit` and `after`:

- `q`: free text. Matches anywhere in the register's text fields, ignoring case; a document
  matches ignoring punctuation (`11.222.333/0001` finds `11222333000181`). Blank means no
  search; at most 100 characters.
- one named parameter per attribute worth narrowing by, with a closed set of values
  (parties: `kind=customer|supplier`).
- `sort`: a field name, `-` in front for descending, a closed set per register (parties:
  `name`, `-name`). Every sortable field is never empty and ends in the row id, so the order is
  total and the cursor can always be built from the last row.

A value a known parameter does not accept (including an empty one) is `400` with a JSON message that
names the parameter. Parameter names the endpoint does not know are ignored, like any other query string.

In code, the use case takes a `PartyQuery` (built with `searching`, `of_kind`, `sorted`, which
normalize their input) and both adapters implement it; the contract tests run the cases
against Postgres and the in-memory adapter, paged so the cursor is exercised. A text search uses `strpos`, not
`LIKE`, so `%` and `_` mean themselves and need no escaping.

## Consequences

- A cursor only continues the list that produced it. It holds the sort key of the last row,
  not the filter, so the client sends the same parameters with it; a cursor reused under other
  parameters resumes at that position in the new order instead of failing. The SPA does this
  by dropping the cursor whenever a parameter changes.
- Search is a substring scan within one company, ordered by the existing
  `(company_id, lower(name), id)` index. That is fine for thousands of rows per company; at
  hundreds of thousands, add a trigram index (`pg_trgm`) behind the same port.
- Names are ordered by `lower(name)` under the database collation, which is what people expect
  (`Ana Maria` before `Anabela`). The in-memory adapter orders by code point, so the two agree
  only for names whose order does not depend on spaces, punctuation or accents. The contract tests
  use such names; do not read the in-memory order as a model of Postgres for anything else.
- Search does not ignore accents (`Jose` does not find `José`) and compares case with the
  database's `lower`, which can differ from Rust's for exotic characters. `unaccent` is the
  fix when users ask for it.
- Sorting by a nullable column (e-mail, document) needs a decision about where nulls go and a
  cursor that can hold "null"; it is deliberately not offered yet.
- A register that also lists soft-deleted rows ([ADR 0008](0008-audit-trail.md)) adds its own
  named filter; it does not change this convention.

## Revisit when

A register needs filters that combine with `or`, ranges (dates, amounts) or saved views:
those belong to the entries list in Phase 3 and may justify a structured filter there.
