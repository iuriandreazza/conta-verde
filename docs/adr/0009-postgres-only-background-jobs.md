---
title: "ADR 0009: PostgreSQL is the only infrastructure; jobs run on it"
status: accepted
date: 2026-11-04
tags: [adr, infrastructure, jobs]
---

# ADR 0009: PostgreSQL is the only infrastructure; jobs run on it

## Context

From Phase 2 on, the product needs work that happens without a request: generating
recurring entries, sending reminders, polling payment and bank providers, delivering
webhooks, scheduling reports, syncing fiscal status. The deployment target is a small
business or an accountant's server: one binary and one database must be enough.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Redis-backed queue | Mature, fast | A second service to run, back up and secure |
| External scheduler (cron calling endpoints) | No code | No retries, no visibility, host-specific setup |
| In-process timers only | Simplest | Lost on restart, no retries, no multi-instance safety |
| PostgreSQL-backed queue with `SKIP LOCKED` (chosen) | Transactional with business writes, no new service, horizontal workers | Throughput limited by the database; fine for this scale |

## Decision

Background work is a `jobs` table consumed by worker tasks inside the same binary
(`conta-verde-api --worker` or a dedicated process), using `SELECT … FOR UPDATE SKIP
LOCKED`. Jobs are enqueued in the same transaction as the business change that needs
them, so a sale and its "generate receivables" job cannot disagree. Each job has a type,
payload, run-at time, attempt count, exponential backoff and a dead-letter state.
Recurring schedules (monthly recurrence, dunning cadence) are rows that enqueue jobs.

Jobs are idempotent by key, because webhooks and provider polling can repeat.

PostgreSQL also serves file storage metadata; attachments themselves go to the local
filesystem or an S3-compatible bucket behind a `BlobStore` port, the only optional extra
service.

## Consequences

- One service to operate; Docker Compose for production is `api` + `db`.
- No Redis or message broker ever enters the dependency list without superseding this ADR.
- Workers must be safe to run in several instances; locking is on the database.
- A `jobs` admin view (list, retry, dead-letter) is part of Phase 2.

## Revisit when

A job type needs more than a few hundred executions per second, or sub-second latency,
neither of which is expected for this product.
