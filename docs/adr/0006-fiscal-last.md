---
title: "ADR 0006: Fiscal documents come last"
status: accepted
date: 2026-11-04
tags: [adr, fiscal]
---

# ADR 0006: Fiscal documents come last

## Context

Issuing invoices is part of the target scope and is the module users ask about first, but
it is the riskiest area:

- **NF-e** (products) goes through each state's SEFAZ web services, with A1/A3 digital
  certificates, XML signing, schema versions, contingency modes, cancellation windows,
  correction letters and number invalidation.
- **NFC-e** (consumer) adds point-of-sale timing requirements and offline contingency.
- **NFS-e** (services) is municipal. Each of the 5,000+ municipalities historically had its
  own layout and web service; the national standard (NFS-e Nacional) is being adopted
  incrementally, so coverage is partial.
- Tax rules (ICMS, ISS, PIS/COFINS, IPI, regimes, the ongoing consumption-tax reform)
  change often and need review by an accountant.
- Getting it wrong has legal consequences for the user, not just a bug report.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Fiscal first (it is what users ask for) | Early differentiation | Blocks everything else behind the hardest integration; no stock or costs to build on |
| Implement SEFAZ/municipal protocols ourselves | No third-party cost; full control | Enormous surface; every state and municipality is a maintenance tail |
| Emission gateway behind a port | One HTTP integration per provider; providers track protocol changes | Dependency on a paid provider for most users; a fake and at least one real adapter needed |
| Fiscal last, gateway first, native adapters where cheap (chosen) | Product is useful long before fiscal; risk isolated | Users who need invoices at launch must pair the product with another tool |

## Decision

Build identity, registers, finance, sales, collection, purchases and inventory first, then
fiscal (Phase 7), in this order: NF-e for Simples Nacional, NFS-e through the national
standard, NFC-e with a minimal point of sale.

Emission is a port in `application` (`FiscalEmitter`: authorize, cancel, correct,
invalidate, query status) with a fake adapter for tests and a gateway adapter as the first
real one. Municipal NFS-e adapters are written only for municipalities not covered by the
national standard, and only on demand.

The domain model leaves room for fiscal data from the start (tax IDs and addresses on
parties, NCM and unit on products, CFOP on operation natures, tax regime on companies) so
that Phase 7 does not require breaking changes to earlier tables.

Tax calculation lives in `domain` as pure functions with worked examples reviewed by an
accountant before the fiscal release.

## Consequences

- The project is useful for financial control from v0.3, long before it can issue
  invoices (v0.8).
- Users who need invoices at launch must pair it with another tool; the purchase XML
  import (Phase 5) and the accountant exports (Phase 6) at least keep incoming and
  outgoing documents in one place.
- A paid gateway is the default path; a native SEFAZ adapter may follow for NF-e only,
  as a separate ADR, if demand and maintainers exist.

## Revisit when

The consumption-tax reform (IBS/CBS) changes document layouts, or the national NFS-e
standard reaches coverage that makes municipal adapters unnecessary.
