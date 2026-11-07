---
title: "ADR 0061: A purchase order and a payment receipt sent by e-mail"
status: accepted
date: 2026-11-06
tags: [adr, email, purchases, finance]
---

# ADR 0061: A purchase order and a payment receipt sent by e-mail

## Context

A purchase order only reaches the supplier if someone copies it out of the app, and a customer who pays has no proof from
the company that the payment was taken. With the queue of [ADR 0059](0059-job-queue.md) a message can be handed over
without making a request wait for, or fail with, the mail server.

## Decision

Two things are e-mailed **on request**, as plain text through the queue, by one service, `DocumentMailService`. They are
reads: nothing is written, so there is no audit entry, and sending twice sends twice.

- **A purchase order to its supplier** (`POST /purchase-orders/{id}/send`, area `purchases`): only an `approved` or
  `partially_received` order (`409` otherwise). The message lists the lines (quantity, description, unit cost), the total,
  the expected day and the notes.
- **The proof of a payment to the customer** (`POST /entries/{id}/settlements/{settlement_id}/receipt`, area `finance`): only
  for a settlement of a **receivable** that was **not reversed** (`409` otherwise). It says what it refers to, the
  installment, the day, interest, fine and discount when there are any, and the amount paid (the cash).
- Both take `{ "to": ... }`; **left out, the address is the party's own**, or else the first contact's that has one
  (`Party::address_to_write_to`, shared with the reminders). With no address anywhere the answer is `422` and says to
  register one or give one; an address that is not one is `422` too. The answer is `{ "to", "emailed" }`, `emailed` being
  "taken by the queue" (false when no mail server is configured: nothing went out).
- Names typed by people are put on one line before they reach a subject or a body (`one_line`, shared). Amounts are written
  as people read them in Brazil (`R$ 10,25`), days as `dd/mm/aaaa`; the text is fixed in Portuguese like the other messages.
- The web app has a `MailPanel` (an address field that is optional, a send button and what happened) opened from the
  purchase's row actions and from a settlement of a receivable.

## Consequences

- A customer's address can be a contact's, so a receipt may go to someone the company wrote down for another reason; the
  person sending sees the address it went to.
- Nothing stops sending the same order repeatedly; there is no record that it was sent.
- Other documents (a sale, a quote's PDF) are not mailed yet; the quote already has its own e-mail with a link.

## Revisit when

- Someone needs to know that a document was sent and when (a record per send), or to send it automatically (when an order is
  approved, when a payment is taken).
- The messages need the company's own wording or a PDF.
