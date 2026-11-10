---
title: "ADR 0089: Automatic bank statement sync through an aggregator"
status: accepted
date: 2026-11-10
tags: [adr, finance, integrations, bank, phase-8]
---

# ADR 0089: Automatic bank statement sync through an aggregator

## Context

[ADR 0011](0011-bank-data-ingestion.md) decided that bank data comes by OFX first and by an aggregator later, behind one port, and
[ADR 0034](0034-statement-import-and-reconciliation.md) built the OFX half: `statement_lines` with a fingerprint, so a file sent twice adds
nothing, and the reconciliation that does not know where a line came from. [ADR 0086](0086-integrations-and-mobile-plan.md) planned this
slice. It makes the statement come in on its own, every day, into the same table. This record says what was built, what it holds and
what it does **not** know, because the adapter was written without an account at the aggregator.

## Decision

### The port and the one adapter

- `BankAggregator` (in `application`) is what the product asks of a provider: a short-lived token for the provider's connection widget,
  the accounts a connection holds, the connection's state (`Ready` or `NeedsAttention(reason)`), the movements of an account from a day
  and ending a connection. Errors are `Disabled`, `Unavailable` and `Refused`; **their text goes to the log and never to a client**.
- Adapters: `NoBankAggregator` (off), `FakeBankAggregator` (tests) and **`PluggyAggregator`**, the first real one.
  `BANK_AGGREGATOR=off|pluggy` picks it; `pluggy` needs `PLUGGY_CLIENT_ID` and `PLUGGY_CLIENT_SECRET` (the server does not start without
  them) and takes an optional `PLUGGY_API_URL` that must be `https`, or `http` on this machine. Another aggregator is another adapter
  and a value of that variable.
- **No credential of a company is stored.** The aggregator's keys are the installation's, in the environment, never in the database, the
  log or an answer. A company's **connection** (`BankConnection`, `bank_connections`) is only: its bank account, the provider's key, the
  aggregator's connection id (`item_id`) and account id, a status, the last day a pull reached, a short reason and who connected it.
  **Changed from ADR 0011**: the port is named `BankAggregator`, not `BankConnector` (the word now means the e-commerce packages of
  ADR 0086), and the consent flow is **not** embedded in the SPA (below).
- A connection is `connected`, `needs_attention` (the bank asks something of the person: the login failed, a code is waited for) or
  `disconnected` (ended: the bank account is free to be connected again). A partial unique index allows **one live connection per bank
  account**; the bank account must be usable and the company's (a composite foreign key), the aggregator must hold the account in the
  connection (asked at connecting) and a company has at most **50** connections. Audited (provider, bank account, ids and status; the
  day of the last pull and the reason are bookkeeping, as `generated_through` is for recurrences).

### The pull

- **What is pulled**: the movements of the account **posted** (never the ones the bank still holds as pending) from a day, each as a
  `StatementLineDraft` (day, signed amount as exact decimal text, description, the aggregator's movement id as the bank reference). They
  go through the **same import** as an OFX file, so the fingerprint makes a movement come in once however many times it is read, and
  the lines are reconciled by the same screens.
- **The window**: the first pull reaches **90 days back**; later ones start **7 days before the last day reached**, so a movement the
  bank posted late is still found, and the fingerprint keeps the overlap from adding twice.
- **A movement that cannot be a line** (a zero amount, a description with nothing in it) is **counted as refused and never blocks the
  rest**: the import refuses a whole file for one bad row, so the sync validates each draft first (`StatementLine::new`). The answer of
  a pull says `created`, `skipped` (already known), `refused` and whether the bank `needs_attention`.
- **Whose name**: the lines are made, and audited, in the name of **whoever connected** the account (as a recurrence is made in the name
  of whoever saved it). There is still no system actor in the trail.
- **When**: a pull is the job kind `SyncBankConnection` of the queue of [ADR 0059](0059-job-queue.md) (one argument, the connection's id,
  **5 tries**; the `jobs.kind` CHECK was widened); **connecting queues the first pull at once**. An **hourly pass** in `main.rs` queues the connections whose last pull is before today
  (the day is `customer_today()` so tests move it) and a worker takes at most 10 jobs a minute. A pass may queue a connection again
  while an earlier job still waits; a pull is safe to repeat, so that costs a read and nothing else. A pull that fails is tried again
  as the queue says, records **why** on the connection in a few words (never what the aggregator answered), and a connection that
  reports `needs_attention` stops being pulled until the person looks at it. **"Sincronizar" on the page pulls now** with the server's UTC
  day (a read of the clock besides the job queue and the hourly passes, like the quote's expiry).
- **Ending**: marks the connection ended and frees the bank account, then tells the aggregator (a failure there is logged and never keeps
  the connection alive here; the consent is then ended at the provider by hand). Ending twice is the same answer.

### Routes and screens

`/companies/{id}/bank-connections` (`finance` area: who reads finance lists, who writes it connects, pulls and ends): `GET` the list
(with `available` and the provider, so the page can say there is none), `POST` to connect, `POST connect-token`, `GET accounts?item_id=`,
`POST {id}/sync`, `POST {id}/disconnect`, `GET {id}/history`. An aggregator that is off is `409`, one that is down `502` with a fixed
text. The page **Conexões bancárias** (under Financeiro, next to Conciliação) lists the connections with state, last pull and why it
stopped, and connects an account by the connection's id (**Buscar contas** asks the aggregator which accounts it holds).

## Not validated, and what the page does not do

- **The Pluggy adapter has never run against the real service.** It was written from Pluggy's public documentation and is tested
  against a local stand-in (`pluggy.rs`) that holds the same reading: the API key (cached 45 minutes, asked again once on a `401`/`403`),
  the v2 transactions listing followed by its cursor only to the same host, only `POSTED`, the amount signed by `type`, and the item
  states `LOGIN_ERROR`, `WAITING_USER_INPUT` and `WAITING_USER_ACTION` read as "needs attention". **What is unverified**: the key's
  lifetime, whether a bank account's `CREDIT`/`DEBIT` follows the sign the stand-in assumes, the time zone of a movement's date, the
  full list of item statuses, the exact form of the `next` link and the widget's address and version.
- **The consent widget is not embedded.** The page takes the connection's id (Item ID) that the person gets from the provider's
  widget or dashboard. `POST connect-token` exists and is tested so a later step can load the widget; this one does not load a
  third-party script into a page that holds the company's books without having been able to see it work.
- **A movement whose id changes at the aggregator comes in again as a new line**; the old one stays and the person ignores it. Fixing
  it would mean guessing which two lines are one.
- The aggregator's own consent lasts as long as the bank's (Open Finance consents expire): the pull then reports `needs_attention` and the
  person renews it at the provider. The product does not renew a consent.
- Tests never reach a real aggregator. `FakeBankAggregator` and the stand-in are the only ones used; a sandbox test is not written because
  it needs an account this project does not have.

## Consequences

- An installation with no aggregator loses nothing: OFX import is unchanged, and the page says so.
- The adapters' behavior is `contract_integrations::syncs_bank_connections` on **both** adapters (connecting refused for a foreign or
  unusable account, once per movement however many times it is pulled, the overlapping windows, a bank that needs the person, ending, and
  the queue-driven pull with the aggregator down and back); the routes are held by the API tests (roles, another company's connection is
  404, no leak of an aggregator's answer).
- A new aggregator is a struct implementing `BankAggregator`, an arm in `bank_aggregator_from_env`, and its own stand-in test.

## Revisit when

- A real account at the provider exists: run the adapter against the sandbox, fix the unverified list above and say so here.
- Movements are seen to change ids: derive the reference from what the bank guarantees instead.
- A company asks for its own aggregator credentials: that needs the sealed secrets of [ADR 0085](0085-gateway-settings-per-company.md).
