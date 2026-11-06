---
title: "ADR 0033: Recurring entries generated ahead by a scheduled job"
status: accepted
date: 2026-11-06
tags: [adr, finance, jobs, phase-2]
---

# ADR 0033: Recurring entries generated ahead by a scheduled job

## Context

Rent, subscriptions and salaries come back every week, month or year. The roadmap asks for recurring
entries "generated ahead by a scheduled job" ([ADR 0009](0009-postgres-only-background-jobs.md)): a person
describes it once, and the entries that fall due in the next weeks are already there to be settled and to
show in the cash flow ([ADR 0032](0032-cash-flow.md)).

## Decision

### The recurrence

A **recurrence** is a register of its own (`/companies/{id}/recurrences`, `finance` area, audited): a kind
(receivable or payable), a one-line description, a category, an optional cost center and party (the rules
of an entry, [ADR 0029](0029-financial-entries.md)), an `amount` above 0 in cents, a `frequency` (`weekly`,
`monthly` or `yearly`), the day it `starts_on`, an optional day it `ends_on`, and whether it is `active`.
It is replaced as a whole by `PUT`, like the other registers. Its occurrences are `starts_on` plus
0, 1, 2… periods: every 7 days; every month, **counted from `starts_on`** with the day clipped to the end of
a short month (the 31st gives the 28th and then the 31st again, never sliding to the 28th for good); every
year likewise (29 February gives 28 February in common years).

### Generation

An occurrence becomes an ordinary entry with **one installment** that falls due on the occurrence day, whose
competence date is that day too, with the recurrence's kind, description, category, cost center and party
and an amount that is the recurrence's at the moment of generation. The entry remembers where it came from
(`recurrence_id` and `occurrence_on`), and **a recurrence has at most one entry per occurrence day**: a unique
index, so generating twice, or two workers at once, cannot duplicate one. After that it is an entry like any
other: it is settled, edited and reversed by the rules of ADR 0029 and ADR 0030, and changing or pausing the
recurrence never touches what was generated.

Occurrences are generated up to **60 days ahead** of today (and never past `ends_on`), at most 120 at a time.
A recurrence remembers the day through which it was generated (`generated_through`), so each pass only looks
at what is new. A change of frequency or of first day **applies from that day on** and does not start over:
the unique index only stops the same occurrence day, and a new schedule has other days, so starting over
would make the whole history again (two rents a month). An inactive recurrence generates nothing, and one
that is brought back goes on **from today**: what fell due while it was paused is skipped, not made up. One
that starts in the past generates what has fallen due since, which is what backfills a recurrence that was
set up late.

The entries are made **in the name of whoever last saved the recurrence** (the audit trail says who), because
there is no system user in the trail yet. If an entry cannot be made (the category was deactivated since),
that recurrence is left as it was and tried again on the next pass; the others go on. A failure while
saving a recurrence is not shown to the person, since the recurrence is already stored: the hourly pass
tries again. The pass reads the recurrences in batches by id, so one that keeps failing cannot keep the
others from being looked at.

### The scheduled job

A pass runs when a recurrence is created or replaced, so the person sees the entries at once, and **every
hour** in a task inside the API process, for every company. The only clock is the server's, and only for
this: the horizon is 60 days, so a zone's few hours never matter. The pass is idempotent, so it is safe in
several instances and after a restart; **it needs no queue**. The `jobs` table of ADR 0009 arrives with the
first job that needs retries with backoff or has side effects that must not repeat (reminders, webhooks,
reconciliation providers); this one has neither, and a table of jobs for it would only be one more thing to
keep in step. The ADR keeps its decision for those.

### Roles

The `recurrences` segment is in the `finance` area ([ADR 0026](0026-membership-roles-and-permissions.md)).

## Consequences

- A recurring bill shows in the entries list and in the forecast of the cash flow two months before it falls
  due, and can be settled like any other.
- A recurrence cannot be deleted yet; it is paused (`active: false`) or given an `ends_on`.
- What was generated is not rewritten when the recurrence changes (a new rent amount applies from the next
  occurrence that is generated, not to those already there).

## Revisit when

A job needs retries, a queue or a dead-letter view; a recurrence needs more than one installment or an
interval other than every one period; or entries have to be generated in the name of a system user.
