---
title: "ADR 0049: Asaas, the first real payment provider"
status: accepted
date: 2026-11-06
tags: [adr, collection, payments, asaas, phase-4]
---

# ADR 0049: Asaas, the first real payment provider

## Context

[ADR 0010](0010-payment-providers.md) asks for at least two real adapters behind the `PaymentProvider` port, chosen
among Brazilian providers with a public REST API and a sandbox, the candidates evaluated in the pull request that
adds them. The port is deliberately small ([ADR 0045](0045-charges.md)): it makes a charge, calls it off, and reads
a **self-contained** webhook ([ADR 0046](0046-webhooks-and-automatic-settlement.md)): the request is verified and
read without calling the provider back. That excludes providers whose notice carries only an id to be looked up.

## Candidates

| Provider | Boleto, Pix, card link | Webhook | Verdict |
|----------|-----------------------|---------|---------|
| Asaas | One `POST /v3/payments` with `billingType` `BOLETO`, `PIX` or `CREDIT_CARD` (the payment then has a page, `invoiceUrl`); Pix text and boleto line by a `GET` each | JSON with the payment (`value`, `netValue`, `paymentDate`), an event id, and a **token header** (`asaas-access-token`) the account holder chooses | **First.** One key, one token, a documented sandbox, the notice has what the settlement needs, and the fee is `value - netValue` |
| Iugu | One invoice, several methods | Form-encoded, an id and a status only | Needs a call back; no |
| Mercado Pago | Payments, and preferences for the card page | A notice with an id, signed | Needs a call back; no |
| Efí | Pix with client certificates (mTLS) | Pix by mTLS, boleto by id | Certificates per company; no |
| Stripe | Boleto and Pix in Brazil need an account there | Signed, full event, but the fee is another call | Later |

The second adapter is chosen in its own slice (ADR 0050).

## Decision

`AsaasProvider` (`infrastructure`), chosen by `PAYMENT_PROVIDER=asaas`, configured by `ASAAS_API_KEY`,
`ASAAS_WEBHOOK_TOKEN` (at least 32 characters, the one registered with the webhook at Asaas; the server does not
start with less, nor with one of the two missing) and optionally `ASAAS_API_URL` (`https://api.asaas.com` by
default, `https://api-sandbox.asaas.com` for the sandbox). Nothing of the key or the token is logged, stored or
returned; of an Asaas answer only the status and the first reason it gives are logged, and a failure of Asaas never
reaches a client but as the fixed sentences of ADR 0045.

**Making a charge.** The payer must have a CPF or CNPJ (Asaas bills one; without it the charge is refused before
asking). The customer at Asaas is found by that document (`GET /v3/customers?cpfCnpj=`) or made (`POST
/v3/customers` with the name and the e-mail), so a customer who is charged twice is one customer there. The
payment (`POST /v3/payments`) carries the amount **written as the decimal text, never a float**, the due day, the
description (at most 500 characters) and **our charge id as `externalReference`**. A Pix then reads its
copy-and-paste text (`GET .../pixQrCode`, `payload`), a boleto its digitable line (`GET .../identificationField`)
and `bankSlipUrl` for the paper, a card its `invoiceUrl`. If the details cannot be read the payment just made is
called off, so it cannot be paid in vain.

**Calling off** is `DELETE /v3/payments/{id}`; one that is already gone is not an error, and one Asaas refuses
(already received) is a rejection.

**Webhooks.** The token header is compared in constant time before anything is read. `PAYMENT_RECEIVED` is a
payment (the day is `paymentDate`, what was paid is `value`, and the **fee is `value - netValue`**, the part Asaas
did not pass on; the amounts are JSON numbers read through serde_json's f64, exact for any amount in cents of a realistic size, and
an amount with more decimals is refused, never rounded); `PAYMENT_CONFIRMED` is a
payment only for a card, which is confirmed weeks before it is received (the later `PAYMENT_RECEIVED` is then flagged
as a second notice of a paid charge, which is harmless); `PAYMENT_DELETED` cancels. Every other event (overdue,
updated, refunded, created) changes nothing here and is **acknowledged with `200`**, because Asaas pauses a queue
after fifteen failures; so a body whose notices are none is `200`, and so is one for payments made elsewhere in the account (a charge that is
not the company's is left out, ADR 0046). The webhook address to register is `{host}/api/public/payments/{company}/webhooks/asaas`,
shown on the Cobrança page.

**Tests** use a local stand-in for the Asaas API (the key checked, the bodies kept, refusals and failures on
demand), never the real one. A test against the **sandbox** is in the suite but `#[ignore]`d (it needs
`ASAAS_SANDBOX_API_KEY`): it is the homologation ADR 0010 asks for, run by hand.

## Consequences

- Funds are the account holder's at Asaas; fees are Asaas's, and the product enters them as the payable of
  ADR 0046.
- One installation talks to one provider (ADR 0044): the key is the installation's.
- What the fake cannot show (Asaas's own limits, a customer it refuses) is found with the sandbox test.
- A card payment that is chargebacked, or a refund, is not understood (ADR 0046).

## Limits

- One Asaas account is **one installation's**: its webhook has one address, so an installation with several companies
  cannot give each its own Asaas account (per-company credentials, ADR 0044), and a payment of another company's
  charge arriving at this one's address is left out.
- A payment that Asaas made but whose answer was lost (a timeout) is not known here and nothing takes it back; it
  shows up in Asaas, and its webhook is left out as a payment made elsewhere.
- A card is a payment at `PAYMENT_CONFIRMED`, about a month before Asaas pays the money out: the account balance
  counts it from then, and the fee is the confirmation's `value - netValue`.
- Asaas also notifies customers by itself (the new charge), besides the reminders of ADR 0048.
- A 404 on calling off is read as already gone, so a wrong `ASAAS_API_URL` would say every cancel worked; the
  sandbox test catches that.

## Revisit when

A company needs its own Asaas account (per-company credentials), Asaas changes how a boleto's amount with interest
is reported (`value` is read as what was paid), or the Pix QR image is wanted.
