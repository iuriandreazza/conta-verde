---
title: "ADR 0059: A queue of jobs and a worker, with e-mail sent through it"
status: accepted
date: 2026-11-06
tags: [adr, jobs, email, infrastructure]
---

# ADR 0059: A queue of jobs and a worker, with e-mail sent through it

## Context

E-mail is sent from inside the request that asks for it (an invitation, a quote) or from the hourly reminder pass. A mail
server that is slow makes the request slow; one that is down loses the message, and the person only learns it because the
page shows a link to pass on instead. More e-mail is coming (password reset, purchase orders to suppliers, receipts,
account notices), and each would repeat the same weakness.

The hourly passes of [ADR 0033](0033-recurring-entries.md), [ADR 0040](0040-contracts.md) and [ADR 0048](0048-notifications-and-dunning.md) are not
the problem: they are idempotent scans of the registers. What is missing is **work asked for once, now, that must survive a
failure and be done later**.

## Options considered

- **Keep sending in the request**, adding retries in each use case. Every caller grows its own retry and its own
  "did it go out" state.
- **An external broker** (Redis, a message service). Another service to run, back up and secure, for volumes this ERP will
  not reach; the project promises a Rust API and a database.
- **A table of jobs in Postgres with a worker in the API process.** One more table, no new service; `FOR UPDATE SKIP LOCKED`
  gives each job to one worker at a time; the same code runs in several instances.

## Decision

A table `jobs` and a worker, in the API process.

- **A job** is a kind (`send_email`, to start) and its arguments as **plain text** in order (`to`, `subject`, `body`): no
  serialization format sits in the domain, and the domain checks the arity of the kind and refuses a NUL byte (`NewJob`).
- **The port** `JobQueue` has `enqueue`, `claim_next(now, lease)`, `complete` and `fail(retry_at)`. Delivery is **at least
  once**: a worker that dies holding a job lets it be taken again when its **lease** (5 minutes) ends, and the try is
  counted. A job whose tries run out because workers kept dying is given up on without running.
- **Retry** is the domain's (`Job::retry_after`): the wait doubles from one minute and stops at an hour, and the **fifth**
  failed try gives up. A job that cannot run at all (an address that is not an address) is given up on at once.
- **A finished job is deleted**: nothing of it stays, so a message with a link is not kept. A job given up on is kept,
  **without its arguments**, with the reason, so an operator can see that something failed.
- The table is **operational, not a register**: no `company_id`, no audit trail, moments in Unix seconds taken from the
  worker's clock (the database's clock is never read), so tests move the clock. This is the third place besides the
  hourly passes where the server reads a clock.
- **The worker** (`JobRunner::run_due`, a task in `main.rs`) wakes every 5 seconds and does up to 50 due jobs.
- **E-mail goes through it.** `QueuedEmailSender` implements the existing `EmailSender` port by queuing the message, so no
  use case changed: the invitation, the quote and the reminder pass hand over their message and the worker sends it with
  the configured transport. `EmailSender::is_enabled` tells whether a transport really sends: with only the log adapter
  nothing is queued and the answer stays "not sent", which is what makes the page show the link, as before. The answer
  **"emailed" now means "taken by the queue"**, and a failure after that is retried, not shown to the person.
- The reminders' own claim ([ADR 0048](0048-notifications-and-dunning.md)) is unchanged: it records that a reminder was handed over, and
  the queue now makes sure it goes out.

## Consequences

- A slow or down mail server no longer slows or fails a request, and a message is tried again for about a quarter of an
  hour before it is given up on.
- Delivery is at least once: a worker that crashes after the server took a message but before the job was deleted sends it
  twice. For e-mail that is tolerable; a future kind that is not harmless to repeat must say how it is made safe.
- A message given up on is visible only in the table and the log. There is no screen for failed jobs yet.
- Every instance runs a worker; there is no leader. More kinds are one variant in `JobKind`, one arm in `JobRunner` and
  one value in the table's `CHECK`.

## Revisit when

- A kind needs a schedule of its own (the hourly passes could become jobs that re-enqueue themselves).
- Jobs need a screen or an alert when they are given up on.
- Volume makes polling every 5 seconds wasteful: `LISTEN/NOTIFY` wakes the worker when a job is queued.
