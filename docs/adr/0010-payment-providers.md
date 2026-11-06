---
title: "ADR 0010: Payment providers behind ports"
status: accepted
date: 2026-11-04
tags: [adr, payments, integrations]
---

# ADR 0010: Payment providers behind ports

## Context

The reference product's strongest feature for service businesses is collection: registered
boleto, Pix and card links issued from a receivable, reminders, and automatic settlement
when the payment clears. The reference product does this through its own regulated
payment institution. We are an open-source project with no license to hold funds.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Become a payment institution | Full control, revenue | Regulatory impossibility for this project |
| Hard-code one provider | Fastest | Lock-in; the provider's pricing becomes ours |
| Bank direct integrations (boleto registration APIs of each bank) | No intermediary fees | One integration per bank, certificates, homologation per bank |
| Provider port with several adapters (chosen) | Users pick a provider and account; fakeable | Lowest common denominator of features across providers |

## Decision

`application` defines a `PaymentProvider` port: create charge (boleto, Pix, card link),
cancel charge, fetch status, parse and verify webhook. A `charges` table links an
installment to a provider charge id. `infrastructure` ships a fake adapter and at least
two real adapters at Phase 4, chosen among Brazilian providers with public REST APIs and
sandbox environments; the candidates are evaluated and recorded in the PR that adds them.
Providers are configured per company with their own credentials; funds settle in the
user's account at the provider, never with us.

Webhooks land on a public endpoint, are verified, stored raw, and processed by a job
([ADR 0009](0009-postgres-only-background-jobs.md)) that settles the installment, records
the provider fee as a separate expense entry, and triggers notifications. Processing is
idempotent on the provider event id. Polling is a fallback when webhooks are unavailable.

Bank-direct boleto adapters can be added behind the same port later.

## Amendments (accepted with Phase 4, [ADR 0044](0044-collection-module-plan.md))

- Credentials and webhook secrets are those of the **installation** (environment variables), not of each
  company, because the product keeps no encrypted secrets yet; a company chooses its bank account and fee
  category in its collection settings. Per-company credentials are the revisit point.
- Without `PAYMENT_PROVIDER` the feature is off; `fake` is for development and tests.
- Webhook events are stored raw and applied **inline** by the request, idempotently, instead of by a queued
  job: the settlement path is idempotent on its own, and a failure answers `5xx` so the provider retries.
  Polling as a fallback is not built in Phase 4.

## Consequences

- The product never touches money; the user keeps their provider relationship and fees.
- Feature set is bounded by what the port expresses; provider-specific extras are not
  exposed.
- A customer-facing payment page is served by the API (boleto PDF, Pix copy-and-paste) so
  reminders can link to it ([ADR 0014](0014-notifications.md)).
- Homologation per provider sandbox is part of the adapter's test suite and runs on demand,
  not in CI.

## Revisit when

A provider offers split or marketplace features users ask for, or Pix automatic debit
(Pix Automático) becomes relevant for recurring contracts.
