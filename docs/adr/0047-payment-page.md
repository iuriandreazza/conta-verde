---
title: "ADR 0047: The customer's payment page"
status: accepted
date: 2026-11-06
tags: [adr, collection, payments, public, phase-4]
---

# ADR 0047: The customer's payment page

## Context

A charge ([ADR 0045](0045-charges.md)) is only useful once the customer can see how to pay it. The reminders of
the next slice link to a page the customer opens without an account, the way a quote's link does
([ADR 0038](0038-quotes.md)): charge details, the boleto, the Pix text to copy, or the card link.

## Decision

### The link

A link is `{PUBLIC_URL}/pay/{token}`. The token is 256 bits from `SecretService::new_token`; **only its digest is
kept** (`payment_links`, one row per link: digest, company, charge, with a composite foreign key so a link is to
the company's own charge). A link is a capability and not a register: it is not audited, like an invitation's
token. A charge may have **several links**, and they all work: each reminder makes its own, and the older ones
a customer may still hold keep leading to the page. There is no expiry in this version; a page of a charge that
is not payable says so (below), and a leaked link shows no more than the page does.

`POST /companies/{id}/entries/{entry_id}/charges/{charge_id}/link` (`finance` area) makes one for an **open**
charge of the company (`404` for another company's, `409` for one that is not open) and answers `201 {url}`. The
address is **shown once**: the web app shows it under the charge with a copy button, since nothing keeps the
token to show again.

### The page

`GET /public/charges/{token}` needs no session; it is the third route family that does not
([ADR 0027](0027-invitations.md), [ADR 0038](0038-quotes.md), [ADR 0046](0046-webhooks-and-automatic-settlement.md)
for webhooks), and it is answered with `no-store` and `no-referrer` like a quote's. A token that leads nowhere
is `404`. It shows **no id of the company's books**: the company's name, the description of the receivable as the
company typed it, the number of the installment and how many there are, the method, the amount, the due day,
and a **state**:

- `payable`: the charge is open, the entry is not cancelled and the installment still has something to pay. Only
  here does the page carry what to pay with (the boleto's digitable line, the Pix copy-and-paste text, the
  provider's address for the boleto's paper or the card checkout).
- `paid`: the charge was paid; the page says so with the day.
- `unavailable`: called off, expired, or the installment was settled or cancelled another way. The page says
  there is nothing to pay and to talk to the company. It never shows what to pay with.

The state is told on every read from the charge, the entry and the settlements, so a payment made another way
closes the page too. The boleto's paper is the provider's own page (`payment_url`): the product does not draw a
bank slip. A QR image of the Pix text is not drawn either (the customer copies the text in their app); it can be
added in the web app without touching the API.

### The web app

`/pay/:token` is a page outside the signed-in shell's guard, with the same layout as the quote page: the heading
with the company, the description and the installment, the amount and due day, and the copy buttons
(`CopyButton`, shared with the charges panel, which also gets a "Gerar link" action).

## Consequences

- A leaked database gives no usable link; a leaked link gives the page of one charge and nothing else.
- Descriptions of receivables are shown to the customer as typed: a company that writes an internal remark in
  one is showing it. The form says what it is for.
- Many links per charge means nothing to revoke except by cancelling the charge, which makes the page say so.

## Revisit when

Links must expire or be revoked one by one, the page needs the company's logo or the payer's name, or a
provider offers no hosted page for the boleto and the product has to draw it.
