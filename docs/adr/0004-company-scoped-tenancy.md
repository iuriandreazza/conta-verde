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

Row-level isolation is enforced in the application layer for now. The `companies` table and the foreign keys are added
together with authentication in Phase 1; until then `company_id` is an unchecked identifier supplied in the URL,
so the API **must not be exposed to untrusted networks yet**.

## Consequences

- Cross-company queries are impossible by construction, which also simplifies a future move to PostgreSQL row-level security.
- Until authentication lands, any caller can read or write any company's parties.
