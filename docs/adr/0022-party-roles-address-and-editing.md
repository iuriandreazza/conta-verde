---
title: "ADR 0022: Party roles, address and editing"
status: accepted
date: 2026-11-05
tags: [adr, registers, parties]
---

# ADR 0022: Party roles, address and editing

## Context

Parties were registered once and never changed: there was no way to correct a name, a document or
an e-mail, and nowhere to say that a party is a carrier or a seller, or where it is. The Phase 1
item "addresses, contacts, carrier and seller types, tags" is split in three so that each piece
can be reviewed on its own: **this ADR** covers editing, the two new roles and one address;
contacts and tags, and the CEP lookup, are separate steps (see Consequences).

## Decision

### Editing

- `PUT /companies/{id}/parties/{id}` **replaces** every field, like the other registers: what the
  body leaves out (the document, the e-mail, the address) is cleared. The roles are the exception
  that must be said: `is_carrier` and `is_seller` may be left out when registering (no) but not
  when replacing, since a body from a client that does not know them would otherwise drop them
  silently. A field the API does not know is refused, on registering as well as on replacing, so a
  typo in `address` is an error, not a cleared address (a client that posts back a whole fetched
  party, with its `id` and `document_check`, is refused too).
- **The registry is asked about a document only when it changed** ([ADR 0017](0017-cnpj-registry-lookup.md)).
  The same document keeps what the registry said: asking again on every edit would cost a call
  from a rate-limited service for nothing, and "Verificar de novo" exists for a deliberate
  re-check. A new CNPJ is looked up like a new registration and never blocks the edit; a document
  removed or replaced by a CPF has nothing to check.
- **Only what differs is recorded**, in the same transaction as the write ([ADR 0008](0008-audit-trail.md)).
  An edit that changes nothing writes nothing and adds no entry. When the document changed, the
  registry's answer is part of the same entry.
- One port method writes everything about a party, `update`, and the re-check of a CNPJ uses it
  too. Because that write replaces the whole row, the re-check reads the party again after the
  registry answers (the call can take seconds) and drops an answer about a document that is no
  longer the party's, so an edit made meanwhile is not undone.
- **Concurrent edits are last-write-wins**, as in every register: two overlapping `PUT`s each
  compute their audit entry against the party they read, so the trail of the loser's `from` values
  can be stale. A guard (a version column, or `WHERE` on the previous values answered by `409`)
  belongs to all registers at once, not to this one.
- A document that another party of the company already has is `409`, as when registering.

### Roles

A party can be any of customer, supplier, **carrier** and **seller**, at least one. They are
columns, not a join table: four fixed roles do not need one, and a list filter is `AND is_x`.
Filtering the list by `kind=carrier` or `kind=seller` follows [ADR 0018](0018-list-endpoints-search-filter-sort.md).
Existing parties are none of the new roles. Sellers are the people commissions will be paid to in
Phase 3; nothing about commissions is stored here.

### Address

One optional address per party: CEP (8 digits, stored without the hyphen), street, city and state
(one of the 27 abbreviations, stored in upper case) are always together; number, complement and
district are optional. The number is free text because "S/N" and "km 12" exist.

- **Only the shape is checked.** Whether a CEP exists, or belongs to that street, is a lookup, not
  a rule, and an unreachable service must not stop data entry.
- It is **all or nothing** in the database as well (a `CHECK`), so a row cannot hold a CEP with no
  city. Text fields are trimmed, not empty and bounded (150 for the street, 80 for the others, 20
  for the number); NUL bytes are refused, as in names. The `CHECK` refuses white space at either
  end with a pattern, not `btrim`, which would let a tab through that the domain trims (the names
  of the other registers still use `btrim`).
- One address, not a list: delivery and billing addresses are a Phase 3 question (a quote or a
  sale will say which address it uses), and a second address is a new table, not a rewrite.
- The audit trail records each address field separately (`postal_code`, `street`,
  `street_number`, ...), so "the city changed" reads as such.

## Consequences

- `PUT` is the first way to correct a party. The web form is shared by registering and changing,
  like the other registers.
- The API response gains `is_carrier`, `is_seller` and `address`; the request gains the same, with
  the two roles optional when registering.
- `parties.is_carrier` and `is_seller` have no default in the schema: every writer says them.
- **Not here, and next:** contacts and tags are in [ADR 0023](0023-party-contacts-and-tags.md); the
  CEP lookup is in [ADR 0024](0024-postal-code-lookup.md).

## Revisit when

- A party needs more than one address (delivery, billing): replace the columns by a table, keeping
  the first address as the main one.
- Sellers need data of their own (commission rules): that is a register of its own that points at
  the party.
