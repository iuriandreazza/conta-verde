---
title: "ADR 0044: The collection module: plan, shared rules and what it changes underneath"
status: accepted
date: 2026-11-06
tags: [adr, collection, payments, notifications, phase-4]
---

# ADR 0044: The collection module: plan, shared rules and what it changes underneath

## Context

Phase 4 gets receivables paid: a charge (registered boleto, Pix or a card link) is issued for an installment,
the provider tells us when it was paid and the installment settles itself, the customer is reminded before and
after the due date, and the company sees who is late. It touches what Phase 2 settled (a settlement has no
origin, an entry's origin is a recurrence or a sale), it adds the first public route that is authenticated by a
signature and the first job with side effects that must not repeat (ADR 0033 said the `jobs` table waits for
exactly that). Those decisions are made once here; each slice's ADR says only what is new.

The `proposed` ADR this phase depends on is [ADR 0010](0010-payment-providers.md): it is **accepted** with the
amendments below. [ADR 0014](0014-notifications.md) is already accepted.

## Decision

### The slices

Each is one pull request with its own ADR, a migration, both adapters, the routes and the pages.

1. **Charges** (ADR 0045): the `PaymentProvider` port, the fake adapter, charges attached to installments
   (issue, cancel, list) and their page section.
2. **Webhooks** (ADR 0046): the public webhook route, signature check, raw events stored, idempotent
   settlement of the installment and the provider fee as a payable, and the company's collection settings.
3. **The payment page** (ADR 0047): what the customer opens without an account, behind an unguessable token.
4. **Notifications and dunning** (ADR 0048): the `notifications` table, the `MessageSender` port (WhatsApp),
   dunning rules per company and the job that sends the reminders (it carries the payment page link, which
   is why the page comes before it; the roadmap lists them the other way round).
5. **A real provider** (ADR 0049) and 6. **a second one** (ADR 0050), behind the same port.
7. **The delinquency report** (ADR 0051).

### Providers: installation-wide credentials, per-company choice

ADR 0010 says providers are configured per company with their own credentials. Storing a company's API key
needs encryption at rest with a key of the installation, and nothing else in the product keeps a secret in
the database. So the first version does the simpler thing: **credentials and webhook secrets come from the
environment of the installation** (`PAYMENT_PROVIDER` names the adapter, each adapter reads its own
variables), are never logged, never stored and never returned by the API. A company that wants its own
provider account runs its own installation. A per-company credential is the revisit point below. Without
`PAYMENT_PROVIDER` the feature is **off**: issuing a charge answers `409` and the web app hides it, the way
`CNPJ_LOOKUP=off` switches the registry lookup off; `fake` is the adapter for development and tests.

Each charge records the provider that made it, so a company that changes adapter keeps what it issued; the
webhook route names the provider too.

### A charge pays one installment, once

A **charge** belongs to one installment of a **receivable** that is not cancelled, is worth what was left of
the installment when it was issued, and has a method (`boleto`, `pix`, `card_link`) and a provider's own id.
At most one charge per installment is **open** (a unique index, partial on the status), so a second issue is
refused until the first is cancelled or expires. A charge is never edited or deleted: it is cancelled
(`cancelled`), expires (`expired`) or is paid (`paid`), and each move is audited.

A charge that is paid **settles its installment through `FinancialEntryService::settle`**, the one path of
[ADR 0030](0030-settlements.md): the balance check, the lock, the audit entry and the cash flow all stay
where they are. The settlement is stored with the charge it came from (`charge_id`, a unique index, so **a
charge settles at most once** even when its settlement is later reversed), which is what makes a webhook that
arrives twice, or half-processed after a crash, safe to run again. What the provider reports as paid and what
the installment was worth are told apart as ADR 0046 says. Settlements are made **in the name of whoever
issued the charge**, like the entries of a recurrence (ADR 0033): there is still no system user.

### The provider's fee

The fee the provider keeps is a **payable entry** made by the webhook (an `EntryOrigin::ChargeFee` with a
unique index per charge, like a sale's), settled the same day from the same account, so the account's balance
moves by what the provider deposited. It needs a category the company chooses (collection settings, ADR 0046);
without one the fee is only recorded on the charge.

### Webhooks are the second route that is signed, not sessioned

`POST /public/payments/{company_id}/webhooks/{provider}` needs no session. It is authenticated by **the
provider's signature over the raw body** (HMAC with a secret of the installation, compared in constant time),
before anything is parsed or stored; an unsigned or wrongly signed body is `401` and leaves no trace. The
company in the path is the one the provider was told to call, so **an event for a charge of another company
is refused** (`404`, nothing recorded). Events are stored raw with their provider id under a unique index;
the same event twice is answered `200` and processed once. A transient failure answers `5xx` so the provider
retries; an event that cannot be applied (the installment was settled by hand meanwhile) is **kept and
marked**, never lost, and shown on the charge.

### The customer's link and the job

The payment page (ADR 0047) uses the pattern of the quote link: a 256-bit
token of which only the SHA-256 digest is kept, shown once. Reminders are sent by an **hourly job** like the
recurrence and contract ones, but with side effects that must not repeat, so each reminder is a row of
`notifications` with a unique key (installment, rule, day): the job inserts the row before sending and a
second pass finds it. This is the `jobs` table of ADR 0009 in the smallest form that works: a table of what
was sent, no queue; retries are the next pass. Tests move the clock, as for contracts.

### Roles

Charges, collection settings, dunning rules and the delinquency report are in the `finance` area
([ADR 0026](0026-membership-roles-and-permissions.md)): the segments `charges`, `collection` and `delinquency`
are added to `area_of` and to its test. The `/public/payments/...` routes are outside `/companies/…`.

### Tests never reach a provider, a mail server or WhatsApp

Every adapter has a fake or a local server (as the CNPJ registry and SMTP tests have). A real adapter's
homologation against the provider's sandbox is on demand, not in CI ([ADR 0010](0010-payment-providers.md)).

## Consequences

- The exit criterion is a test: a boleto or Pix charge settles its installment through a signed webhook, and
  the dunning job, with a moved clock, sends the reminder sequence once each.
- A settlement now has an optional origin (a charge) and an entry has a third origin kind.
- One installation talks to one provider at a time.

## Revisit when

A company needs its own provider account (encrypted per-company credentials), a second provider must run in
the same installation at once, Pix Automático matters for contracts, or reminders need a queue with backoff
rather than the next hourly pass.
