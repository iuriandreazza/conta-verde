---
title: "ADR 0004: Company-scoped data from day one"
status: accepted
date: 2026-11-04
tags: [adr, tenancy]
---

# ADR 0004: Company-scoped data from day one

## Context

Accountants and small groups manage several companies (CNPJs) in one installation. Retrofitting tenancy later means rewriting every query.

## Decision

Every business table carries a `company_id`, every repository method takes a `CompanyId`, and unique constraints are scoped by it
(for example, the same CPF/CNPJ may exist in two companies, but not twice in one).

Row-level isolation is enforced in the application layer. Phase 1 added the `companies` and `memberships` tables, the foreign key
from `parties`, and authentication ([ADR 0007](0007-cookie-sessions.md)): every company route first checks that the session's user
is a member of the `company_id` in the URL.

## Consequences

- Cross-company queries are impossible by construction, which also simplifies a future move to PostgreSQL row-level security.
- A new company route must call the membership check before touching data; the compiler does not enforce it.
- Before Phase 1 the API trusted `company_id` blindly. Databases created back then may hold parties of companies that do not exist,
  which makes the foreign-key migration fail.
