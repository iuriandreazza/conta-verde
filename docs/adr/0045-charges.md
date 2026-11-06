---
title: "ADR 0045: Charges for installments, made at a payment provider"
status: accepted
date: 2026-11-06
tags: [adr, collection, payments, phase-4]
---

# ADR 0045: Charges for installments, made at a payment provider

## Context

[ADR 0044](0044-collection-module-plan.md) plans the collection phase and [ADR 0010](0010-payment-providers.md)
puts payment providers behind a port. This slice is the port, a fake adapter and the charges themselves: asking
the provider to collect an installment and keeping what the customer needs to pay. A provider's webhook that
settles the installment, the customer's page, reminders and the real adapters are the next slices.

## Decision

### The port

`PaymentProvider` (in `application`) has `key()` (its name, which a charge records), `is_enabled()`,
`create_charge(request)` and `cancel_charge(external_id)`. A request carries our charge id (the provider keeps
it as its reference, so a webhook can be traced back), the method, the amount (exact decimal, in cents), the due
day, the payer (name, document without punctuation, e-mail) and a description. The answer is the provider's id
for the charge and the **payment details**: a boleto's digitable line, a Pix copy-and-paste text, the address of
the provider's page (the boleto's paper, or the card checkout). Asking the provider for a status and webhook
parsing are not here: the first is polling, which Phase 4 does not build (ADR 0010 amendments), the second is
the next slice's.

`ProviderError` says why it failed: `Disabled` (the operator configured none), `Rejected` (the provider refused:
a payer it cannot bill) or `Unavailable` (network, timeout). The text is for the server's log and is never sent
to a client, since a provider's message can carry what a person should not see; the API answers `409`, `422`
and `502` with a fixed sentence.

### Adapters

`FakePaymentProvider` answers from memory and keeps what it was asked, so a test can look at the calls and make
it fail; what it makes cannot be paid. `NoPaymentProvider` is the off switch. The API picks one from
`PAYMENT_PROVIDER` (`off`, or unset, and `fake` for now; a name it does not know stops the server instead of
silently collecting nothing). `.env.example` and `pnpm dev` set `fake`.

### A charge

`Charge` is a request to be paid for one installment of a receivable, never edited or deleted: it moves
`open` → `cancelled` here (`paid` and `expired` are set by the webhooks of the next slice) and each move is
audited. It has the entry and installment number (no foreign key to the installments: they are replaced as a
whole when an entry is edited, like a settlement's), the method (`boleto`, `pix`, `card_link`), the amount, the
due day, the provider's key and id, the payment details and **who issued it** (a user and a name snapshot, so a
settlement made from it later is made in their name, ADR 0044).

Rules of one row, in the domain and as `CHECK`s: the amount is above 0 and in cents; the provider key is
`[a-z][a-z0-9_]{0,31}`; the provider's id is a trimmed line without control characters; **each method has the
details it needs and nothing of another's** (a boleto a digitable line of digits, spaces and dots; a Pix a text
of printable characters without spaces; a card link an `https://` address without spaces). The address is
https only, so a page cannot be given a `javascript:` link by a provider.

Rules that span rows: **an installment has at most one open charge** (a unique partial index, and the adapter
maps its violation to `ChargeAlreadyOpen`, `409`); the provider's id is unique per company and provider (the
webhook finds a charge by it). Nothing in the database forces the installment to exist or to have a balance.

### Issuing

`ChargeService::issue` refuses, before asking the provider: no provider configured (`409`); an entry that is not
a receivable, has no customer, or is cancelled (`422`/`409`); an installment the entry lacks (`422`, like any
reference that is not there); an installment with nothing left (`422`). The charge is for **what is left of the
installment**: its amount less what the settlements that are not reversed settled (ADR 0030). It falls due on
the installment's due date unless the person gives another day (a boleto cannot be issued for a day in the past
at most providers, and the server reads no clock, so the person chooses the day). The provider is asked, the
answer is checked by the domain (an answer that is not a charge is a `Rejected`), the charge is stored with its
audit entry in one transaction, and **if storing fails the provider's charge is called off** (best effort,
logged), so nothing is left that could be paid without us knowing.

An open charge is **not** called off by itself when the entry is cancelled, or the installment is settled or
edited meanwhile: the person cancels it, and the webhook (next slice) refuses to settle what no longer fits.

### Calling off

`cancel` calls the charge off **at the provider first** and then records it; with another provider configured
than the one that made it (or none) it is refused, since the boleto would stay payable. Cancelling a charge that
is not open is `409`.

### Routes and roles

Nested under the entry, like attachments and settlements, so they are in the `finance` area
([ADR 0026](0026-membership-roles-and-permissions.md)): `GET`/`POST /companies/{id}/entries/{entry_id}/charges`
(`{installment, method, due_on?}`), `POST .../charges/{charge_id}/cancel` and `GET .../charges/{charge_id}/history`.
`GET /companies/{id}/collection` (segment `collection`, `finance`) says which provider is configured
(`{provider: "fake" | null}`), so the web app can offer to charge or say it is not set up.

### The web app

An entries row gets a "Cobranças" action (receivables that are not cancelled) that opens a panel: the charges with
their status and what the customer pays with (copy buttons for the line and the Pix text, a link to the provider's
page), the form to issue one for an installment that has a balance and no open charge, and "cancel".

## Consequences

- The product still never touches money: the provider keeps it, and a charge only says what was asked.
- A charge outlives edits to its installment; a mismatch is caught when the provider reports a payment.
- Provider-specific extras (split, instalments on the card) are not expressed by the port.

## Revisit when

A provider needs data the port does not carry (a payer address for a boleto, which the next slices will meet),
or charges need to be called off along with the entry.
