---
title: "ADR 0023: Party contacts and tags"
status: accepted
date: 2026-11-05
tags: [adr, registers, parties]
---

# ADR 0023: Party contacts and tags

## Context

[ADR 0022](0022-party-roles-address-and-editing.md) left two pieces of the parties item for their
own step: the people to talk to at a party, and free labels to group parties. Both have several
values per party, so both need a table of their own, and both raise the audit question that a
single column does not: what does the trail say when a list changes?

## Decision

### Contacts

- A party has up to **20 contacts**, in the order they were given (the first is the one to call
  first). A contact has a **name** (required, up to 100 characters), and optionally a **job title**
  (up to 80), an **e-mail** (the same shape and lower-casing as the party's) and a **phone**.
- A phone is **kept as written** (`(81) 99999-0000` and `+55 81 9999-0000` are both normal) once its
  shape is checked: only digits, spaces and `+ - ( ) .`, with 8 to 15 digits (the longest
  international number has 15) and at most 30 characters. It is not normalized to digits, because
  nothing matches on it yet; when something does (WhatsApp, Phase 4), that is a new column or a
  migration, not a guess made now. Whether the number exists is never checked.
- There is no "main contact" flag and no limit of one per kind: the order is the priority.

### Tags

- A tag is a short free label (`vip`, `atacado`). It is **trimmed and in lower case**, so `VIP` and
  `vip` are one tag and the filter finds either; 1 to 30 characters, no comma (the audit trail
  writes the tags on one comma-separated line) and no control characters. A party has up to **10
  tags**, each once, in alphabetical order.
- There is **no register of tags**: a tag exists while some party carries it. A label that names
  nothing else (no colour, no description) does not need a table, and renaming a tag is a search
  and replace the user can do with the filter.
- `GET /parties?tag=vip` lists the parties that carry the tag, combined with the other filters
  ([ADR 0018](0018-list-endpoints-search-filter-sort.md)). Unlike the other filters this one is
  free text, not a closed list, so a tag that no party has answers with an empty page, and one that
  could never exist (too long, a comma) is `400`.

### Writing

- Both lists are **replaced as a whole**, by the same `PUT` that replaces the party, in the
  transaction that writes the party and its audit entry, like a payment term's installments
  ([ADR 0021](0021-bank-accounts-payment-methods-and-terms.md)): a refusal anywhere leaves the old
  lists in place. There is no endpoint for one contact or one tag.
- `contacts` and `tags` must be said when replacing, as `is_carrier` and `is_seller` must: a body
  from a client that does not know them would otherwise clear them silently. When registering,
  leaving them out means none. An empty list is how a replacement clears them.
- A party and its lists are read in one `REPEATABLE READ` transaction (the same snapshot the
  payment terms use), so a write committed between the statements cannot give a party with the old
  name and the new contacts.

### Audit

- The trail has two more fields, `tags` (the tags on one line: `atacado, vip`) and `contacts` (one
  entry per contact, `Ana (Compras) <ana@x.com> (81) 99999-0000`, separated by `; `). Each shows
  the whole list before and after, not a diff of items: it reads as a sentence and needs no
  vocabulary for "moved" or "edited a contact".
- Whether the contacts changed is decided on the contacts themselves, not on that text: a name can
  hold the separator, so two different lists could read alike, and an edit that changed something
  must never be dropped as "nothing changed". The same lists written another way (`VIP` for
  `vip`, an upper-case e-mail) change nothing and record nothing.

### Database

`party_contacts` and `party_tags` hold the rows, keyed by the party and the position (contacts) or
the tag, with a foreign key on `(company_id, party_id)` so a row is in its party's company. The
checks repeat what one row can show (lengths, trimmed, lower case, the phone's characters and digit
count, no comma or control character in a tag); the limits of 20 and 10, which span the rows of a
party, are the domain's, as the installments' sum is for a payment term. `parties` gains a
`UNIQUE (company_id, id)`, only as the target of those keys.

## Consequences

- The party response gains `contacts` and `tags`, and so does the request (required when replacing).
  The list shows the tags under the name, as buttons that filter by the tag, and the number of
  contacts under the e-mail.
- Searching `q` does not look in contacts or tags: finding a party by the e-mail of one of its
  contacts is a reasonable wish, but it widens the search for every list and is left until someone
  needs it.
- The last piece of the parties item, the CEP lookup that fills an address in, is an external
  service and gets its own ADR.

## Revisit when

- Something has to match on a phone number or send to one (WhatsApp notifications, Phase 4):
  store the digits in a column of their own.
- Tags need a colour, a description or a rename: make them a register.
- A party needs a flagged main contact, or contacts that belong to a delivery address.
