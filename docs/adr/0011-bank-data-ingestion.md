---
title: "ADR 0011: Bank data: OFX first, aggregator later"
status: proposed
date: 2026-11-04
tags: [adr, finance, integrations]
---

# ADR 0011: Bank data: OFX first, aggregator later

## Context

Reconciliation needs bank statement lines. Users of the reference product get them by
importing OFX files or by an automatic connection to their bank. Automatic connections in
Brazil go through Open Finance, which requires a regulated participant; aggregators expose
it through an API.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| OFX/CSV import only | No external dependency; every bank exports it | Manual step for the user |
| Open Finance direct | No intermediary | Requires regulatory registration; not possible for this project |
| Aggregator API (Pluggy, Belvo and similar) | Automatic sync, broad bank coverage | Paid, per-connection consent flow, provider lock-in |
| Both, behind one port (chosen) | Works offline from day one; automation for those who pay | Two code paths feeding the same tables |

## Decision

A `statement_lines` table per bank account stores imported lines with a stable
fingerprint (date, amount, description, bank id when present) to prevent duplicates.
Phase 2 ships the OFX parser (and CSV with a column mapper) in `infrastructure`.
Phase 8 adds a `BankConnector` port with an aggregator adapter that fetches lines on a
schedule ([ADR 0009](0009-postgres-only-background-jobs.md)); the consent flow is handled
by the aggregator's widget in the SPA.

Reconciliation logic (matching lines to entries by amount, date window and party, creating
entries from lines, ignoring lines) lives in `domain` and `application` and does not know
where the line came from.

## Consequences

- Users without an aggregator still get full reconciliation.
- Duplicate prevention is by fingerprint, so re-importing a file is safe.
- The aggregator adapter depends on a paid service and is optional at deploy time.

## Revisit when

Open Finance allows a non-regulated open-source participant, or a bank publishes a free
statement API worth a direct adapter.
