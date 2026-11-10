---
title: "ADR 0088: Webhooks: subscriptions, signed deliveries, retries and a log"
status: accepted
date: 2026-11-10
tags: [adr, api, integrations, webhooks, security, phase-8]
---

# ADR 0088: Webhooks: subscriptions, signed deliveries, retries and a log

## Context

[ADR 0086](0086-integrations-and-mobile-plan.md) planned the webhooks and [ADR 0087](0087-public-api-v1.md) built the `v1` API a
subscriber reads from. This slice makes the company able to be told: sale created, completed and cancelled, a charge paid, stock
below its minimum, and a test. It records what was built and the places where building it changed the plan (**Changed from ADR
0086**).

## Decision

### Subscriptions and their secret

- A **subscription** (`WebhookSubscription`, `WebhookService`, table `webhook_subscriptions`) is the company's: an address, the
  events it asks for (none is every one) and whether it is on; at most **20** per company; created, changed and its secret replaced
  only by owners and admins (`/companies/{id}/webhook-subscriptions`, the `members` area, in the web app's API and not in `v1`).
  Audited **without the secret**: the trail says `secret: set` or `changed`.
- The **address** is checked when it is written (`clean_url`): `https`, a host that is a name (not `localhost`) or a **public** IP
  literal, an optional port in 1 to 65535, no credentials, no white space or control character, at most 2,000 characters. A `CHECK`
  repeats the shape the database can see. Checking it again at every delivery is the sender's job (below).
- The **secret** is **editable in the interface** and **write-only**, like every secret of the product ([ADR 0085](0085-gateway-settings-per-company.md)):
  made at random when the subscription is (`whsec_` and 256 bits) and shown once, or typed by the person (8 to 200 printable
  characters, no spaces: `SecretText`); replaced at any time; never read back. It is **sealed with the existing `SecretCipher`**
  (AES-256-GCM, company and field `webhook_secret:{id}` as associated data) and the sealed bytes are the only thing stored. **Without
  `FISCAL_SECRETS_KEY` webhooks are off**: nothing can be made or replaced (`409 WebhooksOff`) and the page says so. Replacing a
  secret moves the current one to `previous_sealed` with `previous_until` = now + 24 hours in the same statement, so a receiver can
  switch without a gap; **a secret that does not open** (the server's key changed) fails the try with a reason that says to replace it.
  The master key does not rotate here either (ADR 0085's limit).

### The events

`sale.created`, `sale.completed`, `sale.cancelled`, `charge.paid`, `stock.below_minimum` (subscribable) and `webhook.test` (only sent
by hand). The list only grows; a client ignores a type it does not know.

- **Who raises them: the adapters, after the write.** **Changed from ADR 0086** (which said "the use case"): the
  `InMemory*`/`Postgres*` repositories of sales, charges and stock hold an optional `SharedEvents` (`with_events`) and publish a
  `DomainEvent` **after their write succeeded** (after the commit in Postgres; `save`, `set_status` and `mark_paid`, `record`). One
  place per adapter covers **every path** that makes the event: the web app, `v1`, a quote that becomes a sale, a contract's cycle,
  the point of sale, a payment provider's notice. A refused write raises nothing, and a retried step that the write refuses
  (`DuplicateOrigin`, `NotFound`) raises nothing again. No use case and no port signature changed. `stock_events` turns a stock
  write into one event per product (the quantity before and after).
- **`stock.below_minimum` is a crossing**: the publisher reads the product's minimum and tells only a move from a quantity at or
  above it to one below it. It is derived and never stored.
- **The body** (`v1::events::V1Events`): `{ id, type, created_at (RFC 3339), company_id, data: { object } }`, `object` being the
  `v1` type of the resource (`Sale` with its `external_reference`; `ChargePaid` = the charge, the receivable and the sale when the
  receivable is a sale's; `Stock`). The `receivable_id` of a `sale.completed` is `null`: the receivable is written after the status.
  The document describes them (`WebhookEvent`, and `webhooks` of OpenAPI 3.1) and a test checks the bodies against it.
- **The id of an event is deterministic** (`event_id`, UUID v5 of the company, the kind, the record and a qualifier), and the
  **unique key `(subscription, event)` is the claim**: publishing the same event twice makes one delivery per subscription.
- **Publishing cannot fail the write it follows**: an event that cannot be stored is logged and lost. That is the window ADR 0086
  accepted (a crash between the commit and the publish), said again in the document: a subscriber reconciles with the reads.

### Deliveries

- A **delivery** (`webhook_deliveries`) is one event for one subscription: its body, its status (`pending`, `delivered`, `failed`),
  the tries, the status code of the last one and **a short reason (at most 200 characters, one line)**. **The subscriber's answer is
  never read or kept.** The log is paged newest first, shown in the web app and kept **30 days** (an hourly prune in each worker).
- It is sent as a `POST` of the body with `Webhook-Id` (the event's id, the same on every try), `Webhook-Timestamp` (Unix seconds,
  new on every try) and `Webhook-Signature`: `v1,` and the base64 of the HMAC-SHA256 of `{id}.{timestamp}.{body}` under the secret's
  own text, **the headers and the layout of the Standard Webhooks specification, with one difference said plainly: the key is the
  secret's own text** (the specification's libraries take a base64 key after `whsec_`, so with one of them the person passes the
  base64 of the text, or computes the HMAC by hand: one line in any language). Several values are separated by a space while a
  replaced secret still counts. Replay protection is the receiver's, and the document says how: verify in constant time, refuse a timestamp more than
  five minutes off, remember the ids. Success is a `2xx` within **10 seconds**.
- **A delivery that was made but never queued** (a failure between the two writes) is queued again by the hourly pass of each worker:
  a pending delivery nobody has written to for two hours has no job, since a retry waits an hour at most and every try writes it.
  **A pending delivery cannot be sent again by hand** (it has a job of its own), and a job that was taken more than the kind allows
  (a worker that died on it every time) is given up on, as e-mail's is.
- **A company with no subscription for a kind** costs the write one small read (`is_wanted`) and nothing else: no reference, no
  entry, no product is read for an event nobody wants.
- **Retries go through the job queue** ([ADR 0059](0059-job-queue.md)): `JobKind::DeliverWebhook` carries only the **delivery id**.
  The attempts of a kind are now the kind's (`JobKind::max_attempts`: 5 for e-mail, **15 for webhooks, about nine hours** with the
  waits of 1, 2, 4, 8, 16, 32 and then 60 minutes), the runner's crash guard uses it, and the `jobs.kind` CHECK was widened by a
  migration. The last failed try marks the delivery `failed`; **a person sends a delivery again by hand** (it goes back to pending
  with no tries, same body, same event id). A delivery that finds its subscription **off** is given up on at once.
- **A slow subscriber must not hold up the queue**: `JobQueue::claim_next_of(now, lease, kinds)` takes only some kinds
  (`claim_next` is the same over all of them), the e-mail worker takes `send_email` only, and **four webhook workers** run in
  `main.rs`, each its own loop (`WebhookService::run_due`), so a subscriber that takes 10 seconds holds up one worker.
- **A test event** (`webhook.test`) goes to one subscription, on or off, and is a delivery like any other: signed, in the log,
  retried. The page has "Enviar teste".

### The target, checked at every try (SSRF)

`HttpWebhookSender` resolves the host with a 5 second timeout, **refuses the name if any address it resolves to is blocked**, and
connects **to the address it checked** (the name is pinned to it in a client made for that delivery: it is never resolved twice).
**No redirect is followed and no proxy of the environment is used.** Blocked (`is_blocked_address`, one table with a test for each
row): `0.0.0.0/8`, loopback, `10/8`, `172.16/12`, `192.168/16`, link-local (the metadata address), `100.64/10`, `192.0.0.0/24`,
`198.18/15`, the documentation ranges, multicast, `240/4` and broadcast; for IPv6 the unspecified address, loopback, unique-local,
link-local, multicast, `2001:db8::/32`, NAT64 `64:ff9b::/96` and 6to4 `2002::/16`; **an IPv4 inside an IPv6 is judged as the IPv4**.
Only `https`; `WEBHOOK_ALLOW_PRIVATE_TARGETS=true` is the operator's **development and test opt-in** (it allows `http` and
private addresses, logs a warning at start, and is what the exit-criterion test uses; the other tests hold the strict policy
against private targets without opening a connection).

## The exit criterion, proved

`a_third_party_makes_a_sale_over_v1_and_receives_its_signed_webhooks` (in `crates/api/src/v1/webhook_tests.rs`): the company
subscribes a **real HTTP listener on this machine** and sets a secret the client knows; a key that may only read is refused; a client
with a `sales:write` key makes the sale over `/v1` (sending it twice makes one), completes it and cancels it; the **real
`HttpWebhookSender`** posts to the listener; the test verifies the signature with the client's secret (and that a wrong secret, a
moved timestamp or a changed body does not verify), checks the bodies against the document and the log, and shows another company's
key reaches none of it. The adapters' behavior is `contract_integrations::runs_webhooks` (subscriptions, the trail without the
secret, once per event, the waits and the give-up after the fifteenth try, sent again by hand, secrets replaced with the two
signatures, the log's pages and its prune), run on **both adapters**, and `claims_jobs_by_kind` on both queues.

## Consequences

- **Webhooks are a hint, not the record**: an event can be lost if the server fails between the commit and the publish; a delivery
  can arrive twice and out of order. The `v1` reads are the truth. `GET /v1/sales?updated_since=` and the sale's `updated_at`,
  promised by ADR 0087 for this slice, are **not built** (a sale has no `updated_at`, and a status written by `set_status` would
  have to bump it in both adapters): the sales can be reconciled by `external_reference` and by number, and `updated_since` is an
  additive change to `v1` for a later step.
- Adding an event means a variant of `WebhookEventKind`, the `CHECK`s of two tables, a place in the adapter that raises it and its
  type in the document.
- The body of a `sale.*` event is the sale as the write that raised it had it: the status is the news, and a concurrent edit of the
  lines of an open order may not be in it (read the sale for the rest).
- The charge and stock adapters share the sale's wrapper; the sale's events are asserted on both adapters
  (`sales_tell_what_happens_to_them`), the charge and stock ones through the API tests (`charge.paid` once for a notice the provider
  sends twice, `stock.below_minimum` only on a crossing).
- **Not validated against anything real**: deliveries to a subscriber over the internet (tests use a listener on this machine
  with the private-target opt-in, and the strict policy is held only by refusing private targets before any connection).

## Revisit when

- An event is seen lost: move events to an outbox written in the business transaction.
- A subscriber needs more than nine hours, or a different wait: the attempts and the waits are the kind's.
- The master key must rotate (ADR 0085's limit applies to these secrets too).
