---
title: "ADR 0006: Fiscal documents come last"
status: accepted
date: 2026-11-04
tags: [adr, fiscal]
---

# ADR 0006: Fiscal documents come last

## Context

Issuing invoices is part of the target scope, but it is the riskiest area:

- **NF-e** goes through each state's SEFAZ web services, with digital certificates (A1/A3), XML signing and contingency modes.
- **NFS-e** is municipal. Each of the 5,000+ municipalities historically had its own layout and rules; the national standard
  (NFS-e Nacional) is still being adopted, so coverage will be incremental.
- Tax rules (ICMS, ISS, PIS/COFINS, regimes) change often and need legal review.

## Decision

Build registers, finance, sales and inventory first, then fiscal, starting with NF-e for Simples Nacional and the national NFS-e standard.
The domain model leaves room for fiscal data (tax IDs, addresses, products' NCM) so that Phase 4 does not require breaking changes.
Where a hard case appears, prefer integrating an open-source or third-party emission gateway behind a port over reimplementing it.

## Consequences

The project can be useful for financial control long before it can issue invoices. Users who need invoices at launch must pair it with another tool.
