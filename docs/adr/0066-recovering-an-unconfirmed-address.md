---
title: "ADR 0066: Recovering an address held by an account nobody confirmed"
status: accepted
date: 2026-11-07
tags: [adr, auth, email]
---

# ADR 0066: Recovering an address held by an account nobody confirmed

## Context

An address can be held by only one account. Signing up takes any address on trust ([ADR 0063](0063-email-confirmation.md)), so
someone can register with another person's address and keep the real owner from signing up. Freeing the address when it was
never confirmed sounds easy, but **releasing it to whoever asks would let anyone take an address from an account that is
only slow to confirm**: the same attack, reversed.

## Decision

An address is released **only to whoever proves the mailbox**, and nothing is deleted.

- A sign-up that finds its address taken asks the use case whether the holder was **never confirmed**
  (`MemberService::recover_unconfirmed_address`). If so, it sends the address the ordinary **password reset link**
  ([ADR 0060](0060-password-reset.md)) and the sign-up answers `409` with a message that says so; if the holder is confirmed the
  answer is the plain refusal and nothing is sent.
- Whoever has the mailbox opens the link and chooses a password: the account is theirs, every session of it ends, the address
  becomes confirmed, and the squatter is out. Nobody else gets anything, since the link only reaches the address.
- The reset link is not spent by the sign-up, and the usual cooldown (one a minute per account) keeps the sign-up form from being
  used to mail an address over and over.
- The web sign-up page, on a `409`, adds "Esse e-mail já tem conta. Entrar ou recuperar a senha."

Two small changes of the same review ride along:

- **`reset_password` looks at the link before it hashes the new password** (`IdentityRepository::password_reset_open`): the route
  needs no session, so a made-up token is turned away before the costly Argon2 work. The write that spends the link repeats the check
  atomically.
- The invitation and quote pages say the e-mail is **in the queue** ("na fila de envio"), which is what `emailed` has meant since
  [ADR 0059](0059-job-queue.md), not that it arrived.

## Consequences

- The real owner of an address held by a squatter recovers it without help from anyone, and with the proof the system can check.
  What they get is the squatter's account, with whatever company it holds: they can leave it alone and make their own.
- A sign-up for any held address reveals whether its holder confirmed it (the `409` text differs); the plain `409` already revealed
  that the address has an account.
- An account that is never confirmed and never recovered keeps its address for good. There is no expiry by design.

## Revisit when

- An account can change its address, or a company can be deleted: an unconfirmed account with nothing in it could then be released
  without a proof.
