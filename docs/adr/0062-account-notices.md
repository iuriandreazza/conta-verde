---
title: "ADR 0062: Notices to the owner of an address when the account changes"
status: accepted
date: 2026-11-06
tags: [adr, auth, email]
---

# ADR 0062: Notices to the owner of an address when the account changes

## Context

Some changes to an account matter to a person who may not have made them: a password that is replaced, a stranger who is in
the company. With the queue of [ADR 0059](0059-job-queue.md) a notice is cheap to send.

## Decision

Two notices, sent by `MemberService` through the queue, never waited for and never failing what caused them (a notice that
cannot be queued is the mail server's to log; what happened stands):

- **The password was changed** (after a reset, [ADR 0060](0060-password-reset.md)): to the account's address, saying the
  password changed and every session ended, and that if it was not them they should ask for a reset again. This is how the
  real owner of an address finds out that someone else held a link.
- **An invitation was accepted**: to **whoever sent it**, naming the person, their address and the company. It is sent when
  the membership is made, whether the person signed in or signed up through the link, and not for a second accept that
  raced the first.

`IdentityRepository` gained `find_user` (to write to the inviter) and `reset_password` now says whose password it replaced.

**A notice of every sign-in is not sent.** Without knowing which devices a person uses, every login would mail, which
teaches people to ignore the mail. It needs device tracking (a cookie that marks a known browser), which is a decision of its
own.

## Consequences

- A person who sends several invitations gets a message per acceptance.
- Names typed by people go through `one_line` before they reach a subject or a body.

## Revisit when

- Sign-ins record the device or the address they came from: a notice of a new one then has something to say.
- An e-mail address has to be confirmed at sign-up (the next slice).
