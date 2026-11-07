---
title: "ADR 0064: A limit and a log for documents sent by e-mail"
status: accepted
date: 2026-11-07
tags: [adr, email, purchases, finance]
---

# ADR 0064: A limit and a log for documents sent by e-mail

## Context

[ADR 0061](0061-document-emails.md) lets a member e-mail a purchase order or a payment receipt to any address, as often as
they like, and keeps no record. The message leaves from the company's sender address, so a stolen session or a loop could use
it to send mail to anyone and burn its reputation, and nobody could tell afterwards what was sent.

## Decision

A **log** of what was sent, and a **limit** judged against it.

- `document_mails` (`DocumentMail`): the kind (`purchase_order`, `payment_receipt`), the document (the order, or the settlement a
  receipt proves), the address, who sent it and when. It is a log: never edited, **not audited** (the document did not change),
  and a row is deleted only to take back a send that went nowhere. `document_id` is polymorphic, so the database cannot hold a
  foreign key to it; the use case finds the document in the company before it records the send.
- **The limits are the same for everyone** (constants in the domain): the same document is **not sent again to the same address
  within five minutes**, and a company may e-mail **a hundred documents over a day**, whoever sends them. Another address, another
  document or another company is not held back by the first.
- **Check and record are one atomic step** (`DocumentMailRepository::reserve`): in Postgres a transaction holds an advisory lock on
  the company while it counts and writes, so two sends racing for the last place cannot both have it. A refusal stores nothing and is
  `429` saying which limit.
- **Order**: reserve, then send; a send that is not taken (no mail server configured) **releases** the record, so a message that went
  nowhere uses up no place.
- **The clock is the caller's**: the API passes `now_millis()` in a `MailAsk`, so the limits are counted over minutes and a day against a
  moment a test can move. This is one more place the server reads a clock.
- `GET /purchase-orders/{id}/mails` and `GET /entries/{id}/settlements/{id}/mails` list what was sent (address and moment, newest
  first, at most 20), in the area of the document and readable by anyone who can read it. The web `MailPanel` shows them ("Já enviado: …").

## Consequences

- A member can no longer use the form to mail one address over and over, and an inbox full of one order stops at the first message
  within five minutes.
- The cap is per company, not per member: one busy member can use the day's places. A per-member limit needs more than this table
  holds (who sent it is there, so it is a query away).
- The log keeps the addresses mails went to for as long as the company exists; it needs a retention rule if it grows.

## Revisit when

- A company needs a different cap (a setting), or a per-member one.
- Other documents are e-mailed (a sale, a quote's PDF): they use the same table with a new kind.
