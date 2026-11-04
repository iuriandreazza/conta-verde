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

## Revisit when

A module needs full history replay (for example stock valuation at any past date), which
would justify event sourcing for that bounded context only.
