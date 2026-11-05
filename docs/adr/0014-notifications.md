---
title: "ADR 0014: Notifications behind ports"
status: accepted
date: 2026-11-04
tags: [adr, notifications, integrations]
---

# ADR 0014: Notifications behind ports

## Context

The product sends e-mails from Phase 1 (invitations, password reset) and, from Phase 4,
customer-facing messages: quotes, charges, reminders before and after due date, and
receipts. The reference product sends these by e-mail and WhatsApp. WhatsApp requires an
approved business account and a provider; e-mail only requires SMTP.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| SMTP only | Works everywhere, no vendor | No WhatsApp, which Brazilian customers expect for reminders |
| One messaging SaaS for everything | One integration | Vendor lock-in for the most basic feature (e-mail) |
| Ports per channel with templates in the product (chosen) | SMTP works for self-hosters; WhatsApp optional; templates versioned with the code | Two channels to keep consistent |

## Decision

`application` defines `EmailSender` and `MessageSender` (WhatsApp) ports. `infrastructure`
ships an SMTP adapter and a WhatsApp adapter for the Cloud API or a provider with a public
REST API, plus fakes. All sends go through jobs ([ADR 0009](0009-postgres-only-background-jobs.md))
with retries and a `notifications` table recording status per recipient, so the SPA can
show "reminder sent on …" on an installment.

Templates are files in the repository (Portuguese copy, per-company variables such as
logo and signature), rendered by the API. Dunning cadences are rows per company
(days before and after due date, channels) that enqueue reminders.

Customer-facing links (payment page, quote approval) are signed tokens served by the API
([ADR 0010](0010-payment-providers.md)).

## Consequences

- Self-hosters need only an SMTP account; WhatsApp is opt-in per company.
- Sending is never done inline in a request handler.
- Opt-out and delivery status per recipient are first-class, which matters for
  anti-spam and for support.

## Implementation notes

Phase 1 ships the e-mail half only, and not yet the jobs: the `EmailSender` port, a **log** adapter
that writes the message to the server's log (the default, for an installation without a mail
server), and fakes for tests. Invitations ([ADR 0027](0027-invitations.md)) send inline in the
request, which this ADR says not to, because there are no background jobs yet; a send that fails
never fails what asked for it. The SMTP adapter, the `notifications` table and the WhatsApp port
come as they are needed.

## Revisit when

SMS or a second messaging channel is requested; it would be a new adapter behind
`MessageSender`.
