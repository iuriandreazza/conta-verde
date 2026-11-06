---
title: "ADR 0038: Quotes, answered through a public link"
status: accepted
date: 2026-11-06
tags: [adr, sales, phase-3]
---

# ADR 0038: Quotes, answered through a public link

## Context

A quote is what a company offers before it sells: items with discounts, a validity, and a way for the customer
to say yes without an account. It is the first slice of Phase 3 ([ADR 0037](0037-sales-module-plan.md)), which
already decided the shape of a line, the one rounding rule, the numbering, the `sales` area and the public routes.

## Decision

### The register

A quote (`/companies/{id}/quotes`, `sales` area, audited) has a company-wide number, a customer (a party of the
company), the day it was issued and the last day it is valid (not before the issue day), an optional payment
term (shown, not applied: applying one is the sale's), notes (up to 2000 characters, any number of lines) and
1 to 100 lines ([ADR 0037](0037-sales-module-plan.md)). It is replaced as a whole by `PUT`; a draft that changes
nothing writes and records nothing. Lists take `q` (the number, the notes or a line's description), `status`,
`customer` and `sort` (`-number`, the default, or `number`). What it points at is checked when it is written and
only when it changes (a product deactivated since does not stop the rest of a quote from being fixed).

### Status

`draft` → `sent` → `approved` or `declined`; `converted` is set by the sale that is made from it (ADR 0039).

- A draft or a sent quote may change. **Changing a sent quote makes it a draft again** and takes its link back:
  the customer was shown something else.
- Sending (`POST .../send` with the address) makes a new link, so **sending again takes the earlier link back**.
  A quote that was answered, or converted, is fixed (`409`).
- The write that changes a quote says the status it was made from, and is refused (`409`) if the quote is in
  another by then. **The customer's answer is a write of its own** that changes only the status, and only while the
  quote is still sent **through the same link**: a quote that was changed and sent again meanwhile has another link, so
  an answer to what the customer saw before is refused, never an approval of something else, and the edit is never
  overwritten. Concurrent edits by people are last write wins, like the entries.
- Whether a quote changed is decided on the quote, not on what the trail would say, so a change the trail has no field
  for (a line's description, which the trail does record after the product, quantity, price and discount) is never
  dropped in silence.

### The link

The link is `{PUBLIC_URL}/quote/{token}`: a 256-bit token made by `SecretService::new_token`, of which only the
SHA-256 digest is stored (a unique index, cleared when the link is taken back). It is shown **once**, in the answer
of the send; the e-mail carries it too. The e-mail has the link only (`EmailMessage` has no attachments); the PDF is
served by the public page. Not being able to send the e-mail never fails the quote (the log adapter, an unreachable
server): the answer says `emailed: false` and the person passes the link on.

The routes outside the company, which need no session, are the second place the API answers without one
(after the invitation preview): `GET /public/quotes/{token}`, `GET .../pdf`, `POST .../approve` and `POST
.../decline`. Every public answer carries `Cache-Control: no-store` and `Referrer-Policy: no-referrer`. The public view shows what the customer needs (company and customer names, the lines, the validity,
the term's name, the notes, the status) and **no id of the company's books**. A token that leads nowhere is `404`.
Approving or declining is allowed once, on a sent quote, and **a quote past its validity cannot be approved** (it can
still be declined, and it cannot be sent: nobody could approve the link)
(`410`): the validity's last day is still good, in the evening too: the day it is judged against is **yesterday by the
server's UTC clock**, since UTC turns the day at 21:00 in Brazil. This is the one place besides the recurrence job where
the server reads its clock, because a customer has no date of their own to send; `expired` in the view tells the page.

The answer is recorded in the audit trail under **whoever sent the quote**, with a field that says it was the
customer through the link (`answered_by`), because the trail's actors are users ([ADR 0008](0008-audit-trail.md))
and a customer is not one. A send records a `link` change (that a link was made and sent, **not to which address**: the trail is kept for good and the address is a third party's), so sending twice shows twice.

### The PDF

`GET /companies/{id}/quotes/{id}/pdf` and the public one write the same document: a hand-made PDF 1.4 with two standard
fonts in WinAnsi encoding (a few lines of text, a table and the total need no library, and an extra dependency is more
to keep than a hundred lines). Money is written the Brazilian way, a description goes over as many rows as it needs and a figure is never cut. The
characters of Windows-1252 that are not Latin-1 (euro, ellipsis, curly quotes, dashes) are written as such; text the
fonts cannot encode becomes a question mark, never a broken file. The tests check the cross-reference table byte by byte.

### Numbering

A number is taken from `document_counters` inside the write of the quote ([ADR 0037](0037-sales-module-plan.md)): the
counter row is locked until the write commits, so a failed write gives its number back and concurrent writes are
serialized per company.

## Consequences

- A customer can approve without an account, and an attacker with no token can do nothing: the token has 256 bits and
  nothing about the response lets one probe another quote.
- Anyone who has the link can answer once; forwarding it forwards that power. A sent quote can be changed (taking the
  link back) if it went to the wrong place.
- A quote cannot be deleted; a draft that is not wanted is left, or declined by sending nothing.

## Revisit when

A customer needs to be asked for more than yes or no (a counter-offer), a quote needs a PDF attached to the e-mail, or
the audit trail gets an actor that is not a user.
