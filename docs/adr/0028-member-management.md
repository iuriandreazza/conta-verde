---
title: "ADR 0028: Changing and removing members"
status: accepted
date: 2026-11-05
tags: [adr, security, tenancy]
---

# ADR 0028: Changing and removing members

## Context

[ADR 0026](0026-membership-roles-and-permissions.md) gave a membership a role and
[ADR 0027](0027-invitations.md) brought people in. Nothing yet changed what a member is, or took
them out, so a wrong role or a person who left stayed until someone edited the database.

## Decision

- `PATCH /companies/{company_id}/members/{user_id}` with `{ "role": ... }` changes a member's role,
  and `DELETE` on the same path removes them. Both answer `204`. Who may call them is decided as
  for every company route ([ADR 0026](0026-membership-roles-and-permissions.md)): the area is
  `members` and both are writes, so only an owner or an admin reaches them (`403` for any other
  member, `404` for a non-member).
- **A company has exactly one owner, and the owner is not touched.** Registering a company makes its
  owner, nobody is invited or set as one, and the owner cannot be changed or removed (`409`, with
  the reason) by anyone, an admin or the owner included. That is what keeps a company from being left
  without an owner, with no count to race on. Handing a company over is for a later ADR.
- The use case checks the target before writing (`404` for someone who is not a member of this
  company, `409` for the owner), and the adapters refuse the owner themselves (`role <> 'owner'` in
  the `UPDATE` and the `DELETE`, the same `NotFound` as a stranger), so a write that races the check
  still cannot reach the owner. A write that finds no row after the check means the member was
  removed in the meantime and answers `404`.
- Anything else is allowed: an admin may change or remove another admin or themselves. The owner is
  always there to put things right, so there is no rule to keep at least one admin.
- A removed member **loses access at their next request**: the role is read from the membership on
  every request, and sessions are not touched, so their account, their sessions and their other
  companies stay as they were. They can be invited again. A changed role counts from the next
  request too; the web app of the person changed reads it again on its next load of the profile.
- Like every membership write, neither is audited ([ADR 0008](0008-audit-trail.md)). Entries the
  person wrote earlier keep their name.
- Their open invitation, if any, is not affected: it is for an address that is not yet a member, and
  a member cannot be invited.

### The web app

On the members page each member except the owner has a role selector and a "Remover" button that
asks for confirmation. A refusal is shown above the list, which is read again after every change.

## Consequences

- An admin can take the other admins out. That is intended: only the owner is the anchor.
- Someone who was removed keeps their session cookie and gets `404` from the company's routes, like
  any non-member. Their web app still lists the company until it reads the profile again.
- Not here: handing a company over, a member leaving on their own (they would ask an admin),
  disabling an account, and the web app hiding what a role cannot use.
