---
title: "ADR 0060: Password reset by an e-mailed link"
status: accepted
date: 2026-11-06
tags: [adr, auth, email]
---

# ADR 0060: Password reset by an e-mailed link

## Context

An account whose password is lost is lost: there is no way back in ([ADR 0007](0007-authentication-and-authorization.md) left it out on
purpose). The e-mail queue of [ADR 0059](0059-job-queue.md) now makes sending the message safe, so this is the first
thing built on it.

## Options considered

- **Ask an administrator to set a password.** Another person holding a way into the account, and no help for an owner.
- **Send the new password by e-mail.** A password in plain text in an inbox, chosen by us.
- **An e-mailed, single-use, short-lived link to choose a password.** The usual shape; the e-mail only carries a token
  that proves control of the address.

## Decision

- `POST /auth/password-reset` `{email}` always answers `202` with no body, whatever the address: it tells nobody who has
  an account (an address that is not an address included). When the address has an account it makes a link and e-mails it.
- `POST /auth/password-reset/confirm` `{token, password}` chooses the new password (the same rules as a sign-up) and answers
  `204`. A link that does not exist, has expired or was used is `410`.
- **The token** is 256 bits from `SecretService::new_token`; only its SHA-256 digest is stored (`password_resets`), like a
  session or an invitation. It lives **one hour** and works **once**: the digest row is deleted in the same transaction
  that changes the password.
- **One link per account** (primary key on the user): asking again replaces the last link, and **no new link is made less
  than a minute after the last**, so the form cannot be used to fill someone's inbox. The refused request still answers
  `202`.
- **Spending a link ends every session of the account**, so whoever held the old password is out. The person signs in again.
- The e-mail goes through the queue (`EmailSender` is `QueuedEmailSender`, ADR 0059). With no mail server configured the
  message is only logged, and the log then holds the link; nothing about it reaches the client.
- The use cases live in `MemberService`, which already has accounts, e-mail, tokens and the public address; the page
  routes are `/forgot-password` and `/reset-password/:token`.
- Account and session writes are not audited ([ADR 0008](0008-audit-trail.md)); neither is this.

## Consequences

- A lost password can be recovered by anyone who controls the inbox, which is what the account's address already means.
- A request for an address that has an account does slightly more work (a token, a write, a queued message) than one for an
  address that does not, so a patient observer could tell them apart by timing. The login's own timing is equalized; this
  is not, yet.
- Login rate limiting is still not built ([CLAUDE.md](../../CLAUDE.md)): the reset link is one more thing an attacker could
  ask for, but only the inbox's owner can use it.

## Revisit when

- Login rate limiting or two-factor authentication is built: both touch the same flow.
- An address needs to be confirmed at sign-up (the next e-mail slice).
