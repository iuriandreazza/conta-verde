---
title: "ADR 0008: Audit trail on every business write"
status: accepted
date: 2026-11-04
tags: [adr, audit, data]
---

# ADR 0008: Audit trail on every business write

## Context

Accountants and owners need to answer "who changed this entry and when". The reference
product shows a history on records. Period locks (Phase 6) and fiscal documents (Phase 7)
require that changes to closed data be visible. Adding auditing after the fact means
touching every repository again.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| `created_by`/`updated_by` columns only | Cheap | Loses history; only the last change is known |
| Database triggers writing to an audit table | Catches every write, including manual SQL | Logic outside the Rust code; harder to test; no use-case context |
| Application-level audit in repositories (chosen) | Records actor, use case and diff with domain meaning | A write that bypasses the repository is not audited |
| Full event sourcing | Complete history by construction | Large upfront complexity for CRUD-heavy registers |

## Decision

Every repository write to a business table also inserts a row into `audit_log` within
the same transaction: `company_id`, `actor` (user or API key), `entity`, `entity_id`,
`action` (`created`, `updated`, `deleted`, `settled`, …), a JSON diff of the changed
fields, and the timestamp. Reads are not audited.

Business records are soft-deleted (`deleted_at`) when they are referenced by other
records; hard delete is only allowed for unreferenced rows and is itself audited.

The audit log is append-only: no update or delete grants on it for the application role.
It is exposed read-only per record in the API and the SPA ("history" panel), and in bulk
for the accountant export ([ADR 0013](0013-accountant-access-and-exports.md)).

## Consequences

- Each repository write carries the actor through the use case; ports take an
  `AuditContext` alongside the `CompanyId`.
- The in-memory adapters also record audits, so use-case tests can assert on them.
- Storage grows with activity; retention beyond a configurable number of years is a
  Phase 9 concern.
- Direct SQL fixes in production are invisible to the trail; operators are told so in the
  upgrade guide.

## Implementation notes

First slice (`feat/audit-trail`), covering parties. Where it differs from the decision above, the decision stays the target:

- **The use case builds the entry, the repository stores it.** Writes take an `AuditEntry` (company, actor, entity, entity id,
  action, field changes) instead of an `AuditContext`: the use case already knows the before and after, so neither adapter has
  to read the old row, and the Postgres and in-memory adapters behave identically. `PartyRepository::save` and
  `update_document_check` store the write and its entry **atomically** (a transaction in Postgres; in memory the entry is
  recorded last, so a refused or missed write leaves nothing). Reading the history is `PartyRepository::history`; it moves to
  its own port when a second entity is audited.
- **The actor comes from the session.** `CompanyAccess` yields the company id and an `Actor::User { id, name }`. The name is a
  snapshot stored with the row (there is no foreign key on the actor), so history reads the same after a rename or removal.
- **A change is a field with a before and an after**, as text (`from`/`to`, `None` meaning not set). A creation lists every set
  field; a re-check of the CNPJ lists only the check fields that changed, and a re-check that gets the same answer writes
  nothing, not even an entry: the trail records changes, not attempts.
- **Append-only by trigger, not by grant.** The application connects with one database role, so "no update or delete grants"
  cannot be enforced; a trigger raises on `UPDATE`, `DELETE` and `TRUNCATE` of `audit_log`. It protects against bugs, not
  against anything that can run arbitrary SQL as the application's role: that role creates the table through the migrations,
  so it owns it and can disable or drop the trigger, as can a superuser. A separate migration role that owns the table, with
  the runtime role holding only `INSERT` and `SELECT`, is the way to enforce what the ADR asks for.
- Writes refuse an entry that does not describe the party being written (another company, another record), so a caller's
  mistake cannot put a row into the wrong trail. The API does not expose the store's counter: it is shared by every company
  and would show how busy the installation is.
- **Parties created before the audit trail have no history** until they change; the panel says so. The first change then
  appears as an "updated" entry without a "created" before it. There is no backfill.
- **Personal data in an append-only table.** Creation entries keep the document and e-mail for good and nothing can erase
  them, which an erasure request under LGPD would need. Every member of the company already sees those fields in the list, so
  nothing new is exposed, but a policy (redaction by a privileged job, or keeping only changed field names) must come before
  production use. Actor names are snapshots for the same reason.
- A row whose action this binary does not know (written by a newer version, then rolled back) makes that party's history
  fail with a server error instead of being skipped: hiding entries from an audit trail is worse. Migrations only go forward.
- Order is `audit_log.sequence` (an identity column), not the timestamp, so rows written in the same millisecond keep a stable
  order. The sequence is assigned when the row is inserted, not when the transaction commits, so two concurrent writes can
  commit in the opposite order; for one party's trail (writes are rare and a CNPJ re-check is throttled) this does not matter,
  but a consumer that follows the whole log by sequence, such as the accountant export, must not assume it never skips ahead.
  A re-check reads the previous check before asking the registry and then replaces it unconditionally, so its recorded "from"
  could be stale if something else changed the check in between; the throttle makes that practically impossible, and a
  conditional update would close it if more writers appear. Time is exposed as epoch milliseconds; the SPA formats it in the user's time zone.
- `GET /companies/{company_id}/parties/{party_id}/history` returns the entries oldest first; a party of another company is a
  `404`, like everywhere else. The SPA shows it under "Histórico" in each row, loaded when opened.
- **Not done yet:** soft delete and audited hard delete (there is no delete endpoint), a history for entities other than
  parties, the accountant export, and the account, session and membership writes, which are not business tables.

## Revisit when

A module needs full history replay (for example stock valuation at any past date), which
would justify event sourcing for that bounded context only.
