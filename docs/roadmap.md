---
title: Roadmap
status: living
date: 2026-11-04
tags: [roadmap]
---

# Roadmap

conta-verde is an open-source ERP for small businesses, inspired by cloud ERPs such as Conta Azul.
Phases are ordered by dependency: each one builds on the data created by the previous one.

## Phase 0 — Foundation (this PR)

- [x] AGPL-3.0-or-later license
- [x] Cargo workspace with `domain`, `application`, `infrastructure`, `api` crates ([ADR 0002](adr/0002-layered-cargo-workspace.md))
- [x] PostgreSQL migrations, CI, Docker Compose for local development
- [x] First vertical slice: parties (customers and suppliers) with CPF/CNPJ validation, scoped by company

## Phase 1 — Registers and finance

- [ ] Authentication and the `companies` table, then enforce tenancy ([ADR 0004](adr/0004-company-scoped-tenancy.md))
- [ ] Bank accounts, categories and cost centers
- [ ] Accounts payable and receivable, installments, settlement
- [ ] Cash flow report
- [ ] Bank statement import (OFX) and reconciliation
- [ ] Web app (React SPA, [ADR 0005](adr/0005-spa-frontend.md)) with a "Source code" link in the footer to honor AGPL section 13

## Phase 2 — Sales and quotes

- [ ] Products and services catalog
- [ ] Quotes, sales orders, generation of receivables
- [ ] Recurring sales

## Phase 3 — Inventory

- [ ] Stock movements, balances, average cost
- [ ] Purchases feeding stock

## Phase 4 — Fiscal

- [ ] NF-e (state level, SEFAZ)
- [ ] NFS-e (municipal, see [ADR 0006](adr/0006-fiscal-last.md))
- [ ] Tax regimes: Simples Nacional first
