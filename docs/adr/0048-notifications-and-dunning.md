---
title: "ADR 0048: Reminders to customers: the dunning plan and its hourly pass"
status: accepted
date: 2026-11-06
tags: [adr, collection, notifications, jobs, phase-4]
---

# ADR 0048: Reminders to customers: the dunning plan and its hourly pass

## Context

[ADR 0014](0014-notifications.md) put notifications behind ports and promised dunning cadences as rows per
company and a record of what was sent. [ADR 0044](0044-collection-module-plan.md) said this is the first job whose
side effects must not repeat, and that the smallest form that works is a table of what was sent and no queue.
The reminders carry the link of the payment page ([ADR 0047](0047-payment-page.md)), which is why that slice
came first.

## Decision

### The plan

A company has at most one **dunning plan** (`dunning_plans` and `dunning_rules`, `GET`/`PUT
/companies/{id}/collection/dunning`, `finance` area, audited with the plan's rules as one text, history at
`.../dunning/history`): a switch and up to 20 rules, replaced as a whole. A **rule** is `days` from the due date
(negative before it, `0` on it, positive after it, from 60 before to 180 after) and a **channel** (`email` or
`whatsapp`); each `(days, channel)` once, kept in the order they fall. A company that never made a plan has an
off one with no rules.

### The pass

Every hour, in a task inside the API process like the recurrence and contract jobs, `DunningService::send_due(day)`
looks at every company whose plan is on. For each rule it takes the **installments of receivables that are not
cancelled and still have something left**, whose due day moved by the rule's days is `day` or one of the **two
days before** (so a server that was down catches up; it never reaches further back, so an old installment is not
suddenly reminded of a rule long past). The day is `customer_today()` (yesterday by the UTC clock, the one the
quotes use), so a reminder is never early for a customer in Brazil; the tests move the clock by passing the day.

A reminder is for one customer, one installment, one rule and one channel. The customer is the entry's party;
the address is the party's e-mail or the first contact's, and the phone of a WhatsApp reminder is the first
contact's phone as digits with the country code (a number written with `+` is kept as it is; a local one of 10 or 11 digits, with no leading 0, gets `55`). An installment with
nobody to reach by that channel, or an entry with no party, is left alone (counted, not sent). A rule for
WhatsApp is not tried at all while no WhatsApp provider is configured.

The text is Portuguese, in the product (the same words by e-mail and by WhatsApp): who is asking, the
description (cut to 120 characters, a template parameter is short), the installment, when it falls due in words, **by how far the due date really is** (not by the rule: the pass catches up two days) ("vence em 3 dias", "vence hoje", "venceu há 3 dias"),
what is **still owed** (the amount less what was settled), the due date and, when the installment has an **open
charge**, a fresh link to its payment page (each reminder makes its own, ADR 0047).

### Sent once

Each reminder is **claimed** in `notifications` before anything is sent: a row with a unique key on company,
entry, installment, rule days and channel, inserted or, if an earlier try **failed** and fewer than 3 were made,
made pending again with one attempt more, in one statement, so two passes at once cannot both get it. Then it
is sent, and the row is finished `sent` or `failed` with a short note. A reminder that is `pending` for good (the
server stopped after claiming and before finishing) is **not sent again**: a customer who gets a reminder twice is
worse off than one who misses it. A failure (the message, or the payment link that could not be made) is tried again by the next passes, three
times in all, which is about three hours: a longer outage loses the reminder, and a pass that meets an installment it
cannot handle logs it and goes on with the others; the web app lists what was sent and what failed under the entry's charges. Notifications are a log, not a
register: not audited, and they keep the address or phone they went to.

### The ports

`EmailSender` is the existing one (SMTP, or logged and reported as not sent without a server: such a reminder
is `failed` and tried three times, which is what the log says). `MessageSender` is the new port for WhatsApp
(`is_enabled`, `send`), with `NoMessageSender` (the default), `InMemoryMessageSender` for tests and
`WhatsAppCloudSender`, which uses the **WhatsApp Business Cloud API**: a message the business starts must be an
approved **template**, so the text goes in as the one body parameter of the template the operator names (on one
line). It is configured by `WHATSAPP_PHONE_NUMBER_ID`, `WHATSAPP_TOKEN` and `WHATSAPP_TEMPLATE`, all or none
(some without the others stops the server), and the token is never logged; only a failed status is. Tests use a
local fake server, never the real API.

### Roles and the web app

The plan and the list of reminders of an entry (`GET /entries/{id}/notifications`) are in the `finance` area. The
Cobrança page gets the plan form (a switch and rows of "when, days, channel"), and the charges panel of an entry
lists its reminders.

## Consequences

- The exit criterion's second half is a test on a moved clock: the reminder sequence of a plan reaches the
  customer once per rule, with the link, however often the pass runs, and nothing reaches what is paid,
  cancelled, not owed or a payable.
- Because the pass reaches every company, a test sharing a database with others looks at its own customer.
- There is no per-customer opt-out yet; stopping a customer's reminders means turning the plan off or leaving them
  without an address.

## Revisit when

A customer must be able to opt out, reminders need a queue with backoff, a second messaging channel is
requested (it would be another adapter behind `MessageSender`), or the template approach is too narrow for the
provider the operator uses.
