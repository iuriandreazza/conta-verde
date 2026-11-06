---
title: "ADR 0050: Stripe, the second real payment provider"
status: accepted
date: 2026-11-06
tags: [adr, collection, payments, stripe, phase-4]
---

# ADR 0050: Stripe, the second real payment provider

## Context

[ADR 0049](0049-asaas-provider.md) compared the candidates and chose Asaas first, because the port wants a
webhook that is read without calling the provider back. Of the rest, **Stripe** is the one whose events are signed
by a documented scheme and carry the amount and the time, whose Brazilian methods (boleto, Pix) and card checkout
are documented, and which is not tied to a Brazilian-only account; Mercado Pago, Iugu and Pagar.me notices carry an
id to be looked up (or their documentation could not be checked), and Efí needs client certificates.

## Decision

`StripeProvider` (`infrastructure`), `PAYMENT_PROVIDER=stripe`, configured by `STRIPE_SECRET_KEY`,
`STRIPE_WEBHOOK_SECRET` (the endpoint's signing secret, `whsec_…`; the server does not start with anything else, nor
with one missing) and optionally `STRIPE_API_URL`. A card page sends the customer back to `PUBLIC_URL`. Keys are
never logged, stored or returned; of a Stripe error only the status and its own `code` are logged (its message can
echo what was sent) and the first 200 characters of the message reach the person only for a refusal of the request.

**The payer's address.** A Stripe boleto needs the customer's CPF/CNPJ **and address**, a Pix and a boleto their
e-mail; `Payer` now carries the party's address (street and number, city, state, CEP), and a charge without them is
refused before asking, with the reason.

**Making a charge** (form-encoded, `Idempotency-Key` = our charge id, our charge id in `metadata[charge_id]`):

- **Boleto**: a PaymentIntent in `brl`, confirmed at once, with the billing details and `boleto[tax_id]`. The voucher
  is read from `next_action.boleto_display_details`: `number` is the digitable line and `hosted_voucher_url` the
  paper. The days to the due date are `expires_after_days` (0 to 60), counted by the clock in Brazil; a due day further than
  that is **refused** with the reason, not quietly cut (it would expire the charge before it is due), and so is a
  document that is not digits (Stripe does not take an alphanumeric CNPJ).
- **Pix**: a PaymentIntent confirmed at once, its copy-and-paste text from `next_action.pix_display_qr_code.data`;
  it expires at the end of the due day in Brazil (`expires_after_seconds`, from one minute to fourteen days; a
  later due day is refused).
- **Card page**: a Checkout Session in `payment` mode (`url` is the page), carrying the charge id as client
  reference and metadata; it lasts **a day** (23 hours, so a clock a little ahead does not get the request refused), the most Stripe allows.

Amounts are the whole number of centavos Stripe takes, computed from the exact decimal. The adapter reads the
clock for the relative expiries above and for the webhook's tolerance, as ADR 0044 allows for a provider's own needs;
a test sets its own.

**Calling off**: a PaymentIntent is cancelled and a session expired. One already cancelled or expired, or gone, is not
an error; one Stripe refuses (paid) is a rejection, and so is a **boleto, which Stripe does not cancel before it
expires**: calling it off here is refused, with Stripe's reason, so the person is not told it no longer can be paid.

**Webhooks.** `Stripe-Signature` (`t=…,v1=…`): only `v1` counts (a `v0` is ignored, against downgrades), the signed
text is `{t}.{raw body}` under the endpoint secret (HMAC-SHA256, compared in constant time), and a timestamp more
than five minutes from now, either way, is refused (replays), all before the body is read.
`payment_intent.succeeded` (boleto and Pix, `amount_received`) and `checkout.session.completed` with
`payment_status = paid` (a card, `amount_total`) are payments, dated by the event's time in Brazil;
`payment_intent.payment_failed` (a boleto or a Pix that was not paid by its time) and `checkout.session.expired`
expire the charge, `payment_intent.canceled` cancels it. Everything else, including the PaymentIntent of a card page
(whose id we do not know), is acknowledged (ADR 0046) and left. The register is
`{host}/api/public/payments/{company}/webhooks/stripe`.

**The fee is not in the notice** (it is on the balance transaction, another call), so a Stripe payment records
**none** and enters no fee payable; the settlement is of what was received.

**Tests** use a local stand-in for the Stripe API (key checked, forms kept, refusals on demand), never the real one,
and sign webhooks with the test secret; a test against Stripe in test mode (`STRIPE_TEST_SECRET_KEY`) is `#[ignore]`d
and does nothing without the key, as ADR 0049's.

## Consequences

- **A boleto is paid in the notice's day, not the payer's.** Stripe sends `payment_intent.succeeded` for a boleto the
  next business day, and the payment is dated by the event: a boleto paid on a Friday is settled on the Monday. Its
  own date would need another call (the charge).
- A boleto that cannot be cancelled stays open here until it expires, so another charge for the same installment
  (a Pix, say) cannot be issued meanwhile; if the answer to its creation is lost, or storing it fails, it cannot be
  taken back and a payment of it arrives as that of an unknown charge (acknowledged and left, ADR 0046).
- The three methods work with Stripe, with the limits above: no fee, a card page that lasts a day (a reminder weeks
  later leads to a page that has expired; the charge is then to be called off and made again), a boleto that cannot be
  cancelled.
- The adapter was written from Stripe's public documentation and has not been run against Stripe: parameters the
  documentation does not show for the server-side Pix (the payer's tax id) are not sent, and the sandbox test is the
  check.
- Payments from Stripe settle in the account holder's Stripe balance, paid out by Stripe's own schedule.

## Revisit when

A fee is wanted (a call for the balance transaction when a payment arrives), card links must outlive a day (Payment
Links), or a company needs its own Stripe account.
