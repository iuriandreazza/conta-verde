---
title: "ADR 0063: Confirming the address of an account"
status: accepted
date: 2026-11-06
tags: [adr, auth, email]
---

# ADR 0063: Confirming the address of an account

## Context

An ordinary sign-up takes any address on trust: the person may have mistyped it, or used someone else's. Every other
message the app sends (a password reset, an invitation, a notice) goes to that address, so it matters that it is the person's.
An account that joins a company by an invitation has already shown it: the link reached the mailbox ([ADR 0027](0027-invitations.md)).

## Decision

- `users.email_verified_at` (null until confirmed). **Accounts that existed before are unconfirmed** and say so.
- **A sign-up that makes a company of its own** e-mails a link, through the queue ([ADR 0059](0059-job-queue.md)):
  `{PUBLIC_URL}/confirm-email/{token}`. A sign-up through an invitation is **marked confirmed at once** and sends nothing.
- **Confirming does not block anything.** It is a request, not a gate: the app shows a banner above the page until the
  address is confirmed, with a button to send the link again. Locking features behind it is a product decision for later.
- The token is 256 bits; only its SHA-256 digest is kept (`email_confirmations`), for **3 days**, single use. Up to five links
  may be out at once, **none is made less than a minute after the last**, and spending one deletes the others. This is the
  shape of the password reset links ([ADR 0060](0060-password-reset.md)).
- `POST /auth/email-confirmation` (session) asks for a new link and answers `202` whether or not one was made (one a minute,
  none once confirmed). `POST /auth/email-confirmation/confirm` `{token}` (no session: the token is the proof) answers `204`,
  or `410` for a link that is unknown, expired or spent. `GET /auth/me` carries `email_verified`.
- **The web page asks for a button press.** A mail program that opens links to check them must not use up the person's link,
  so the page does not confirm on load.

## Consequences

- Existing accounts see the banner until they ask for a link and confirm.
- The identity port gained `email_verified`, `mark_email_verified`, `create_email_confirmation` and `confirm_email`; the SQL
  for the links repeats that of the password reset (a table each). Both could become one table of account tokens.
- Changing the address of an account is not possible yet; when it is, it must reset the confirmation.

## Revisit when

- Something has to wait for a confirmed address (sending documents, inviting people).
- An account can change its address.
