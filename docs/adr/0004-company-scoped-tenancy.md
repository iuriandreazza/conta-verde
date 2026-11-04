---
title: "ADR 0004: Company-scoped data from day one"
status: accepted
date: 2026-11-04
tags: [adr, tenancy]
---

# ADR 0004: Company-scoped data from day one

## Context

Accountants and small groups manage several companies (CNPJs) in one installation, and the
reference product offers that. Retrofitting tenancy later means rewriting every query and
every unique constraint. At the same time, the foundation phase has no authentication, so
the tenancy boundary cannot yet be enforced from a session.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| One database per company | Hard isolation, simple queries | Migrations and backups per tenant; cross-company accountant views need federation |
| One schema per company | Isolation at the schema level | Same operational cost as databases; connection pooling per schema |
| Shared tables with `company_id` (chosen) | One migration, one backup, cross-company queries for accountants are possible | Isolation depends on discipline in every query |
| Shared tables + PostgreSQL row-level security | Second barrier enforced by the database | Needs a session variable per request; adds friction early |

## Decision

Every business table carries a `company_id`, every repository method takes a `CompanyId`,
and unique constraints are scoped by it (the same CPF/CNPJ may exist in two companies, but
not twice in one).

Isolation is enforced in the application layer first:

- Phase 0: `company_id` is an unchecked identifier supplied in the URL. The API binds to
  `127.0.0.1` by default and **must not be exposed to untrusted networks**.
- Phase 1: the `companies` table, foreign keys from every business table, and the
  `company_id` resolved from the authenticated session's membership
  ([ADR 0007](0007-authentication-and-authorization.md)). The URL parameter stays for
  explicitness but must match a membership of the caller.
- Phase 9: PostgreSQL row-level security policies keyed on a per-request session variable
  as a second barrier, so that a missed `WHERE company_id` cannot leak rows.

Reference data shared by all companies (CEP tables, NCM lists, tax tables) lives in tables
without `company_id` and is read-only for tenants.

## Consequences

- Cross-company queries are impossible by construction in application code, which also
  simplifies the later move to row-level security.
- Since Phase 1 a company route gets its access check by declaring the `CompanyAccess` extractor before any body or query
  extractor. The compiler does not force a handler to declare it, so reviewers must look for it on every new
  `/companies/{company_id}` route.
- Databases created in Phase 0 may hold parties of companies that do not exist, which makes the foreign-key migration fail;
  `docker compose down -v` resets a development database.
- Accountant views across companies are implemented as a loop over memberships, not as
  cross-tenant SQL.
- Every index on a business table starts with `company_id`.

## Revisit when

A tenant needs physical isolation (regulatory or contractual), which would mean a
per-database deployment option on top of the same schema.
