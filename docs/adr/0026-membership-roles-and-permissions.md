---
title: "ADR 0026: Membership roles and what each may do"
status: accepted
date: 2026-11-05
tags: [adr, security, tenancy]
---

# ADR 0026: Membership roles and what each may do

## Context

[ADR 0007](0007-authentication-and-authorization.md) decided that a membership has one of seven
roles and that permissions are coarse, by area, not by record. Until now a membership had no role:
whoever belonged to a company could do everything in it, which was true only because nobody but its
creator could belong. Invitations, the next step, make that false, so roles and what they allow
come first. This ADR fixes the table and the way it is enforced.

## Decision

### The table

A role reaches each **area** as far as reading or writing (writing includes reading); a blank is
not at all. An area is a family of routes: `parties` is customers, suppliers, carriers and sellers
(and the CEP lookup that fills their address in); `products` is products and services;
`finance` is the registers money hangs from (categories, cost centers, bank accounts, payment
methods and payment terms, and the entries and settlements that join them in Phase 2); `members`
is who belongs to the company and their invitations.

| role       | parties | products | finance | members |
|------------|---------|----------|---------|---------|
| owner      | write   | write    | write   | write   |
| admin      | write   | write    | write   | write   |
| financial  | write   | read     | write   |         |
| sales      | write   | read     | read    |         |
| stock      | read    | write    | read    |         |
| accountant | read    | read     | read    |         |
| viewer     | read    | read     | read    |         |

Why these, since the choice is a product one and easy to change (it is one function and one test):

- **`owner` and `admin` can do everything.** They differ in what only the owner may do with a
  company, which the next step defines (handing it over, removing the last owner); this step has no
  such action yet, so they are the same row.
- **`financial` writes parties as well as the finance registers**: a receivable needs a customer and
  a payable a supplier, and sending the person elsewhere to register one is the friction a
  financial role exists to avoid. Products are read: it needs them to value an entry, not to change
  the catalog.
- **`sales` writes parties** (a seller registers the customer) **and reads the rest**: it sees
  products and prices to quote, and the finance registers, but changes none of them.
- **`stock` writes products** and reads the rest.
- **`accountant` and `viewer` read everything but members.** They differ only later: the
  accountant gets period locks and exports (Phase 6, [ADR 0013](0013-accountant-access-and-exports.md)).
- **Nobody below `admin` reaches `members`**, not even to read: who else belongs to a company and how
  they were invited is for those who run it.

### How it is enforced

- A role is a column on `memberships` (`owner`, ... , `viewer`, in a `CHECK`), with **no default**, so
  a membership cannot become an owner by leaving the role out. Existing memberships are the owners'
  that came with their companies. Registering a company creates its owner.
- **The route decides what is needed, not the handler.** `CompanyAccess`, the extractor every
  company route already takes first, reads the **area** from the route's pattern (the segment after
  `{company_id}`) and the **access** from the method (`GET`, `HEAD` and `OPTIONS` read; anything else
  writes), and refuses a role that does not reach it. A handler therefore cannot forget the check,
  and importing, checking a CNPJ again or applying the default categories are writes because they
  are `POST`s, with no list to keep in sync. A segment that no area claims is **refused to
  everybody** until it is given one: a new family of routes fails closed.
- **`403`, not `404`.** A non-member still gets `404` (the company does not exist for them,
  [ADR 0007](0007-authentication-and-authorization.md)); a member whose role does not reach the route
  gets `403` with `your role in this company does not allow this`, since the company is theirs to see.
- `GET /auth/me` says, for each company, the caller's `role` and `permissions` (`{area: "read" |
  "write"}`, an area they cannot reach left out). The web app reads that and never knows the table.
- The table lives in the domain (`Role::access_to`), with its own test written out in full apart
  from the code, and a test through the HTTP layer that tries a read and every kind of write, with
  every role, and checks that what is refused left nothing behind.
- Membership changes are not audited ([ADR 0008](0008-audit-trail.md)), as before.

## Consequences

- Nothing changes for anyone today: every membership is an owner's. The roles matter from the
  moment there is a way to join a company with another one (invitations, next).
- `memberships_of` replaces `companies_of`, and `role_in` replaces `is_member`, in the identity
  port; `add_membership` is the one way to make a member, and never changes the role of an existing
  one.
- Reading is open to every member for the four areas but `members`; hiding what a role cannot use
  in the web app is a convenience that follows from `permissions`, and the API is what refuses.
- One role per member per company; a person who is the accountant of two companies has two
  memberships.

## Revisit when

A real company needs a profile this table cannot say (the same person selling and reading finance
but not parties): then permissions per area become a set on the membership instead of a role's row,
which the `permissions` the API already returns can carry. Also when entries arrive (Phase 2) and
"finance" needs to split into receivable and payable, or when a seller must see only their own
customers (per-record permissions, ADR 0007).
