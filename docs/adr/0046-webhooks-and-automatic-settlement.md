---
title: "ADR 0046: Provider webhooks settle the installment and enter the fee"
status: accepted
date: 2026-11-06
tags: [adr, collection, payments, webhooks, phase-4]
---

# ADR 0046: Provider webhooks settle the installment and enter the fee

## Context

[ADR 0045](0045-charges.md) makes charges; this slice closes the loop that
[ADR 0044](0044-collection-module-plan.md) and [ADR 0010](0010-payment-providers.md) describe: the provider says
a charge was paid, the installment settles itself through the one settlement path
([ADR 0030](0030-settlements.md)), and the fee the provider kept is entered. It is the first route that is
authenticated by a signature and not by a session, and the first write that must be safe to run twice.

## Decision

### The route

`POST /public/payments/{company_id}/webhooks/{provider}` needs no session and is outside `/companies/…`. The
company in the path is the one the provider was told to call (the web app shows the address on the Cobrança
page). It answers `200 {"received": n}`; `401` for a signature that is missing or not the provider's; `400` for a
signed body that is not a notice; **`404` for a provider that is not the configured one, a company or a charge
that does not exist, and for a charge of another company**, which look the same and store nothing. A storage
failure answers `500`, which is what makes the provider send the notice again.

The port gains `verify_webhook(headers, body)`: it checks the signature **over the body as it came, before
anything of it is parsed** (HMAC-SHA256 with the installation's secret, compared in constant time by the `hmac`
crate; each real adapter says which header and secret) and reads the notices in it: an id of the notice, the
provider's id of the charge, and `Paid {amount, day, fee}`, `Cancelled` or `Expired`. The fake signs with a
secret (`x-fake-signature`): public in the tests, and in a running server the operator's
`FAKE_PAYMENT_WEBHOOK_SECRET` (a random one nobody knows when unset), so that `fake` left on by mistake does not
let anyone who read the source mark a charge as paid. A notice of a payment of nothing is kept and flagged.

### Notices are kept, once

Each notice is stored as it came (`payment_events`: the raw body up to 64 KB, the provider and its id for the
notice, the charge, the kind, an outcome `pending`/`applied`/`not_applied` and a short note) under a unique index
on company, provider and the provider's id. The table is a log and not a register: it is not audited and never
edited, only finished. Receiving a notice whose id is stored and finished does nothing and answers `200`; one
stored and still `pending` was interrupted, and is applied again.

### Applying a payment, idempotently

The steps are individually idempotent, so a notice that arrives twice, at the same time, or after a crash
between steps, ends in the same state:

1. **Settle** what is left of the installment, up to the charge, through `FinancialEntryService::settle_charge`,
   in the name of whoever issued the charge, from the company's collection account, on the day the provider
   gives. What was paid over the applied amount is `interest`, what was paid under it is `discount`, so **the
   cash of the settlement is exactly what the customer paid**. The settlement remembers the charge
   (`entry_settlements.charge_id`, a unique index) so **a charge settles at most once, even after the settlement
   is reversed**; the adapter asks for it first, inside the lock, so a repeat is `DuplicateOrigin` and not an
   overpayment.
2. **Enter the fee** when there is one and the company chose a category: a payable (`EntryOrigin::ChargeFee`, a
   unique index per charge) whose one installment falls due on the payment day, settled the same day from the
   same account, so the account moves by what the provider deposited. Without a category the fee stays on the
   charge. The fee is entered even when the installment could not be settled: the provider kept it all the same.
3. **Mark the charge paid** with its receipt (day, what was paid, the fee, and a note), from whatever status it
   had: a customer can pay a boleto that was called off a moment before the provider heard of it. The move is
   audited in the issuer's name. A paid charge is not paid again.
4. **Finish the notice** with its outcome.

### What cannot be applied is kept and shown

A payment that finds the entry cancelled, the installment gone or already settled (by hand, or by another
payment), or the collection account unset is **not lost**: the charge becomes paid with a **note** saying why,
the notice is finished `not_applied`, nothing else is written, and the Cobranças panel shows a warning on the
charge for the person to look at (a refund is the provider's to give). A second notice that says the same
charge was paid is kept and flagged the same way, never settled again. Part of the installment settled by
hand meanwhile: only what is left is settled, and the cash is still what was paid.

`Cancelled` and `Expired` notices move an open charge accordingly (audited) and are otherwise ignored.

### Collection settings

A company says where the money lands: `collection_settings`, one row per company (`PUT/GET
/companies/{id}/collection/settings`, segment `collection`, `finance` area, audited): the **bank account**
(active, the company's own) and the optional **fee category** (active, not the transfers one; only a reference
the change makes is checked). **A charge cannot be issued without it** (`409`), so none exists that could not be
settled. The web app has a Cobrança page for it, which also shows the provider and the address to give it.

### Where it runs

Inline in the request, not in a queued job: the steps are idempotent and a failure is retried by the provider, as
the ADR 0010 amendments say. The sender of reminders, which has side effects that must not repeat, is the first
thing that needs the `notifications` table (next slices).

## Consequences

- The exit criterion's first half is a test: a Pix charge settles its installment through a signed webhook (in
  both adapters, and through the API).
- A provider that retries is safe; one that does not retry a `5xx` loses the payment until someone asks it again.
  Polling as a fallback is not built.
- A refund or a chargeback is not a notice this slice understands.
- Settlements and entries gain an optional origin (a charge); the entries list shows the fee as an ordinary
  payable.

## Revisit when

A provider needs a handshake, an IP allow-list or a challenge before its first webhook, refunds and chargebacks
must reverse the settlement, or webhooks arrive in volumes that need a queue.
